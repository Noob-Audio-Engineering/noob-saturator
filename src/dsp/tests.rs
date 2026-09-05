//! Tests for the DSP.
//!
//! Every test here that checks a real figure asserts that figure with its
//! source named. Where nothing is published, the assertion is an **identity**,
//! an **ordering** or a **direction** rather than an invented bound — because
//! a test that asserts a value the model itself produced is not a test, it is
//! a snapshot, and an audit of this project found nine of those across five
//! plug-ins.
//!
//! The aliasing figures are not here. They are in `src/bin/benchmark.rs`,
//! which drives minutes of audio, and in the out-of-tree probe, which has
//! never seen this code. `cargo test` stays fast.

use super::adaa::{self, Adaa};
use super::alias::AliasMeter;
use super::color::{self, Color};
use super::curve::{CURVE_NAMES, Curve, Shape};
use super::engine::{self, Saturator};
use super::oversample::{self, DryDelay, MAX_FACTOR, Resampler, TAPS};
use super::*;

const SR: f32 = 48_000.0;

fn settle(e: &mut Saturator, n: usize) {
    let mut l = vec![0.0f32; 64];
    let mut r = vec![0.0f32; 64];
    for _ in 0..n {
        e.process(&mut l, &mut r);
    }
}

// ---------------------------------------------------------------------------
// The curves and their antiderivatives
// ---------------------------------------------------------------------------

/// **The closed forms have to be the real antiderivatives**, or the
/// antialiasing quietly becomes a distortion of its own: the quotient would
/// still be smooth and still sound saturated, and it would be integrating
/// the wrong function.
///
/// Differentiating `F₀` numerically has to give back `f`, and `f` is written
/// in the same file independently of `F₀` rather than derived from it, so
/// this is a check of two expressions against each other rather than of one
/// against itself.
#[test]
fn every_antiderivative_differentiates_back_to_its_curve() {
    for curve in Curve::ALL {
        for knee in [0.0f64, 0.25, 0.5, 1.0] {
            let s = Shape::new(curve, knee);
            for probe in [
                -3.7f64, -2.0, -1.3, -0.9, -0.4, -0.05, 0.05, 0.4, 0.9, 1.3, 2.0, 3.7,
            ] {
                // A central difference, with the step chosen for the usual
                // cube-root-of-epsilon trade between truncation and
                // cancellation.
                let h = 1e-5 * probe.abs().max(1.0);
                let d = (s.f0(probe + h) - s.f0(probe - h)) / (2.0 * h);
                let want = s.f(probe);
                assert!(
                    (d - want).abs() < 1e-6 * want.abs().max(1e-3) + 1e-7,
                    "{curve:?} knee {knee}: dF₀/dx at {probe} is {d:.9}, f is {want:.9}"
                );
            }
        }
    }
}

/// `F₀(0) = 0` for every curve. Parker, Zavalishin and Le Bivic recommend
/// that choice of integration constant as a free mitigation for the
/// precision lost in the quotient's difference, and it is only free if it is
/// actually taken.
#[test]
fn every_antiderivative_is_zero_at_the_origin() {
    for curve in Curve::ALL {
        let s = Shape::new(curve, 0.5);
        assert!(s.f0(0.0).abs() < 1e-12, "{curve:?}: F₀(0) = {}", s.f0(0.0));
    }
}

/// Every curve is odd and every one is bounded by 1, except that `Fold`
/// reaches its bound and turns round, which is what a wavefolder is.
#[test]
fn the_curves_are_odd_and_bounded() {
    for curve in Curve::ALL {
        let s = Shape::new(curve, 0.4);
        for probe in [0.05f64, 0.5, 1.0, 3.0, 40.0] {
            assert!(
                (s.f(probe) + s.f(-probe)).abs() < 1e-9,
                "{curve:?} is not odd at {probe}"
            );
            assert!(
                s.f(probe).abs() <= 1.0 + 1e-9,
                "{curve:?} reaches {} at {probe}",
                s.f(probe)
            );
        }
    }
}

/// Unit slope at the origin for five of the six, and **zero** for `Gate`,
/// which is the whole point of that shape. The unit slope is what makes the
/// drive control mean the same thing on every curve and what lets the device
/// null against its own input when the drive is turned down.
#[test]
fn the_slope_at_the_origin_is_what_the_menu_says() {
    let h = 1e-6f64;
    for curve in Curve::ALL {
        let s = Shape::new(curve, 0.5);
        let slope = (s.f(h) - s.f(-h)) / (2.0 * h);
        if curve == Curve::Gate {
            assert!(slope.abs() < 1e-6, "Gate's slope at the origin is {slope}");
        } else {
            assert!(
                (slope - 1.0).abs() < 1e-5,
                "{curve:?}'s slope at the origin is {slope}"
            );
        }
    }
}

/// `Gate` flattens through a **rounded corner and not a discontinuity**,
/// which is the one distinction that matters for first-order antialiasing: a
/// discontinuity in the function itself is the case it handles worst. Near
/// zero the curve behaves as `2x³`, so the second derivative vanishes there
/// too.
#[test]
fn the_gate_rounds_its_corner_rather_than_breaking_it() {
    let s = Shape::new(Curve::Gate, 0.5);
    let h = 1e-3f64;
    let second = (s.f(h) - 2.0 * s.f(0.0) + s.f(-h)) / (h * h);
    assert!(
        second.abs() < 1e-6,
        "Gate's curvature at the origin is {second}"
    );
    for x in [0.01f64, 0.02, 0.05] {
        let want = 2.0 * x * x * x;
        assert!(
            (s.f(x) - want).abs() < 0.02 * want,
            "Gate at {x} is {} against the cubic {want}",
            s.f(x)
        );
    }
}

/// At knee zero the `Clip` curve is a hard clipper, and the knee joins
/// smoothly in value **and slope** everywhere else, so nothing but knee zero
/// has a corner in it.
#[test]
fn the_clip_knee_joins_smoothly() {
    let hard = Shape::new(Curve::Clip, 0.0);
    for x in [-4.0f64, -1.0, -0.3, 0.3, 1.0, 4.0] {
        assert!((hard.f(x) - x.clamp(-1.0, 1.0)).abs() < 1e-12);
    }
    for knee in [0.1f64, 0.5, 0.9, 1.0] {
        let s = Shape::new(Curve::Clip, knee);
        let k = 1.0 - knee;
        for joint in [k, 2.0 - k] {
            let h = 1e-6;
            let lo = (s.f(joint - h) - s.f(joint - 2.0 * h)) / h;
            let hi = (s.f(joint + 2.0 * h) - s.f(joint + h)) / h;
            assert!(
                (lo - hi).abs() < 1e-3,
                "knee {knee}: slope jumps from {lo} to {hi} at {joint}"
            );
        }
        assert!(
            (s.f(9.0) - 1.0).abs() < 1e-12,
            "knee {knee} misses its ceiling"
        );
    }
}

#[test]
fn the_curve_names_match_the_menu() {
    assert_eq!(CURVE_NAMES.len(), Curve::ALL.len());
    for (i, c) in Curve::ALL.iter().enumerate() {
        assert_eq!(c.index(), i);
        assert_eq!(Curve::from_index(i), *c);
        assert_eq!(c.name(), CURVE_NAMES[i]);
    }
}

// ---------------------------------------------------------------------------
// The antialiasing
// ---------------------------------------------------------------------------

/// **The published small-signal identity.** Parker, Zavalishin and Le Bivic's
/// equation (17): at levels low enough that `f(x) ≈ x`, first-order
/// antiderivative antialiasing reduces to `y[n] = (x[n] + x[n−1])/2` — a
/// half-sample fractional delay by linear interpolation. That is a statement
/// about the method, not about this implementation, so it is the strongest
/// non-circular test available for it.
#[test]
fn at_small_signals_the_kernel_is_the_published_half_sample_interpolator() {
    for curve in Curve::ALL {
        if curve == Curve::Gate {
            // `Gate` is deliberately not transparent at small signals: its
            // slope at the origin is zero, so the premise of equation (17)
            // does not hold for it and neither does its conclusion.
            continue;
        }
        let s = Shape::new(curve, 0.5);
        let mut a = Adaa::new();
        let mut prev = 0.0f32;
        let mut worst = 0.0f32;
        for i in 0..2000 {
            // Small enough that every curve here is within a part in a
            // million of the identity, and slow enough that the step stays
            // above the ill-conditioning threshold is *not* required — the
            // identity holds on both branches.
            let x = 1e-3 * (i as f32 * 0.05).sin();
            let got = a.process(&s, x, 1.0, 0.0);
            let want = 0.5 * (x + prev);
            worst = worst.max((got - want).abs() / 1e-3);
            prev = x;
        }
        assert!(
            worst < 1e-4,
            "{curve:?}: worst departure from (x[n] + x[n−1])/2 is {worst}"
        );
    }
}

/// **The threshold and the antiderivative's precision are one decision.**
/// The rule, derived in `ANTIALIASING.md` §4.1 from the error amplification
/// `2r·|F₀|/|Δu|`, is that the threshold must be at least the square root of
/// the antiderivative's relative error.
///
/// This test fails if somebody changes either constant without the other —
/// which is the failure it exists to prevent, because it is silent: the
/// transfer curve stays right, the sound stays saturated, nothing crashes,
/// and the alias floor collapses by more than thirty decibels.
#[test]
// The two constants *are* the subject: an assertion that folds at compile
// time is exactly what is wanted here, because the failure it guards against
// is somebody editing one of them.
#[allow(clippy::assertions_on_constants)]
fn the_threshold_and_the_antiderivative_precision_are_paired() {
    assert!(
        adaa::MIN_STEP * adaa::MIN_STEP >= adaa::F0_RELATIVE_ERROR,
        "a threshold of {:e} pairs with an antiderivative good to {:e}, but the rule needs the \
         threshold at or above {:e}",
        adaa::MIN_STEP,
        adaa::F0_RELATIVE_ERROR,
        adaa::F0_RELATIVE_ERROR.sqrt()
    );
    // And the other end of the same constraint: a threshold that is large
    // against the signal degenerates to plain midpoint sampling and does
    // nothing at all. Measured, a fixed 0.01 is safe for any signal above
    // about −40 dBFS (`ANTIALIASING.md` §4.1).
    assert!(adaa::MIN_STEP <= 0.01);
}

/// The behavioural half of the same pairing, and the one that would actually
/// catch a regression: with the antiderivative spoiled to a lookup table's
/// precision, the textbook threshold destroys the antialiasing and the paired
/// one does not.
///
/// The published measurement is −4.6 dB against −44.3 dB
/// (`ANTIALIASING.md` §4.1, out-of-tree probe). This runs a shorter signal
/// than the benchmark does, so it asserts the **collapse** rather than the
/// digits, which is the honest form when the conditions differ.
#[test]
fn spoiling_the_antiderivative_without_moving_the_threshold_destroys_the_antialiasing() {
    // A slow tone, because the ill-conditioning lives where a signal turns
    // around and two consecutive shaper arguments are nearly equal. A fast
    // one never divides by anything small and shows nothing.
    let shape = Shape::hard_clip();
    let run = |threshold: f64, f0_error: f64| -> f32 {
        let mut a = Adaa::with_precision(threshold, f0_error);
        let mut worst = 0.0f32;
        for i in 0..20_000 {
            let x = (2.0 * std::f32::consts::PI * 110.0 * i as f32 / SR).sin();
            let y = a.process(&shape, x, 10.0, 0.0);
            // The clipper's output is ±1 almost everywhere at this gain, so
            // anything far from ±1 near the flat part is injected error.
            if x.abs() > 0.3 {
                worst = worst.max((y.abs() - 1.0).abs());
            }
        }
        worst
    };
    let paired = run(adaa::MIN_STEP, 1e-3);
    let textbook = run(1e-6, 1e-3);
    assert!(
        textbook > paired * 30.0,
        "the pairing is not doing anything: textbook threshold gives {textbook:.6} of error \
         against the paired threshold's {paired:.6}"
    );
}

/// The stage law is `f(g·x + b) − f(b)`, so it rests at zero however the
/// bias is set. Without the subtraction a bias control would be a direct
/// current generator.
#[test]
fn the_stage_rests_at_zero_at_every_bias() {
    for curve in Curve::ALL {
        let s = Shape::new(curve, 0.5);
        for bias in [-1.0f64, -0.4, 0.0, 0.4, 1.0] {
            let mut a = Adaa::new();
            for _ in 0..8 {
                let y = a.process(&s, 0.0, 4.0, bias);
                assert!(
                    y.abs() < 1e-6,
                    "{curve:?} at bias {bias} rests at {y} rather than zero"
                );
            }
        }
    }
}

#[test]
fn the_stage_survives_steps_and_stillness() {
    for curve in Curve::ALL {
        let s = Shape::new(curve, 0.3);
        let mut a = Adaa::new();
        for x in [0.0f32, 40.0, -40.0, 0.0, 1e-9, -1e-9, 1e30, -1e30, 0.0] {
            let y = a.process(&s, x, 63.0, 0.7);
            assert!(y.is_finite(), "{curve:?} gave {y} for {x}");
        }
    }
}

/// Caching `F₀` across the segment boundary must not change a single sample.
/// It is an optimisation that the shape, the gain and the bias all key into,
/// and a stale entry would be inaudible until it was not.
#[test]
fn the_antiderivative_cache_changes_nothing() {
    for curve in Curve::ALL {
        let s = Shape::new(curve, 0.5);
        let mut cached = Adaa::new();
        let mut worst = 0.0f32;
        for i in 0..4000 {
            // Sweep the gain and the bias every sample, so the cache misses
            // as often as it hits.
            let t = i as f32 * 0.001;
            let g = 1.0 + 3.0 * (t * 1.7).sin().abs();
            let b = 0.5 * (t * 0.9).sin();
            let x = 0.8 * (t * 11.0).sin();
            // A fresh stage cannot use the cache at all, because it has no
            // history — so drive a second one one sample behind and compare
            // the pair on the sample they share.
            let mut fresh = Adaa::new();
            fresh.process(&s, prev_of(i, 0.8, 11.0), g as f64, b as f64);
            let want = fresh.process(&s, x, g as f64, b as f64);
            let got = cached.process(&s, x, g as f64, b as f64);
            if i > 0 {
                worst = worst.max((got - want).abs());
            }
        }
        assert!(
            worst < 1e-6,
            "{curve:?}: the cache moved the output by {worst}"
        );
    }
}

fn prev_of(i: usize, amp: f32, rate: f32) -> f32 {
    if i == 0 {
        0.0
    } else {
        amp * (((i - 1) as f32 * 0.001) * rate).sin()
    }
}

// ---------------------------------------------------------------------------
// The resampler
// ---------------------------------------------------------------------------

/// The dot products in the polyphase filters run **forwards** over a history
/// stored oldest first, which is only correct because a windowed sinc is
/// symmetric. Nothing else in the file checks it and the loops are silently
/// wrong without it.
#[test]
fn the_half_band_is_symmetric_and_its_even_taps_are_exactly_zero() {
    let h = oversample::coefficients();
    for k in 0..TAPS {
        assert_eq!(h[k], h[TAPS - 1 - k], "the filter is not symmetric at {k}");
    }
    let centre = (TAPS - 1) / 2;
    for (k, hk) in h.iter().enumerate() {
        let n = k as i32 - centre as i32;
        if n != 0 && n % 2 == 0 {
            assert_eq!(*hk, 0.0, "tap {k} is {hk} and should be exactly zero");
        }
    }
    let sum: f32 = h.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6, "direct-current gain is {sum}");
}

/// The round trip is unity at the latency the module publishes, at every
/// factor. Not a published figure: it is the resampler's own contract, and
/// the whole latency claim rests on it.
#[test]
fn the_cascade_round_trips_at_unity_with_the_stated_latency() {
    for depth in 1..=oversample::MAX_DEPTH {
        let mut r = Resampler::new(depth);
        let lat = r.latency();
        let n = 2000;
        let mut out = vec![0.0f32; n];
        let mut buf = [0.0f32; MAX_FACTOR];
        for (i, o) in out.iter_mut().enumerate() {
            let x = (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / SR).sin();
            r.up(x, &mut buf);
            *o = r.down(&buf);
        }
        let mut err = 0.0f32;
        for i in 1000..n {
            let want = (2.0 * std::f32::consts::PI * 1000.0 * (i - lat) as f32 / SR).sin();
            err = err.max((out[i] - want).abs());
        }
        assert!(err < 0.002, "depth {depth}: round-trip error {err}");
    }
}

/// **Every factor's latency is a whole number of base-rate samples**, which
/// is the constraint that fixes the tap count. A fractional round trip cannot
/// be matched by an integer delay line and cannot be honestly reported to a
/// host, which would cost both of the claims this plug-in makes about its
/// dry path.
#[test]
fn every_factor_has_an_integer_latency() {
    assert_eq!(oversample::latency_for_depth(1), 64);
    assert_eq!(oversample::latency_for_depth(2), 96);
    assert_eq!(oversample::latency_for_depth(3), 112);
    assert_eq!(oversample::latency_for_depth(4), 120);
    assert_eq!(
        (TAPS - 1) % 16,
        0,
        "the tap count no longer divides for 16x"
    );
}

#[test]
fn the_dry_delay_delays_by_exactly_what_it_says() {
    for len in [0usize, 1, 5, 64, 96, 120] {
        let mut d = DryDelay::new(len);
        let mut out = Vec::new();
        for i in 0..400 {
            out.push(d.process(i as f32));
        }
        for i in len..400 {
            assert_eq!(out[i], (i - len) as f32, "len {len} at {i}");
        }
    }
}

// ---------------------------------------------------------------------------
// The colour section
// ---------------------------------------------------------------------------

/// **An identity, not a bound.** The inverse filter is the same design
/// evaluated at the reciprocal gain, which for a Robert Bristow-Johnson
/// peaking equaliser or a shelf at slope one exchanges the numerator and the
/// denominator exactly. So the cascade is the identity to the precision of
/// the arithmetic, at every setting, and that is what is asserted.
#[test]
fn the_colour_forward_and_inverse_cancel() {
    for (base, depth, q, hz) in [
        (36.0f32, 24.0f32, 0.7f32, 1000.0f32),
        (-36.0, -24.0, 0.7, 1000.0),
        (18.0, -24.0, 4.0, 120.0),
        (-6.0, 12.0, 0.1, 17_000.0),
        (0.0, 0.0, 0.7, 1000.0),
    ] {
        let mut c = Color::new(SR);
        c.configure(color::Settings {
            on: true,
            base_db: base,
            freq_hz: hz,
            q,
            depth_db: depth,
        });
        let mut worst = 0.0f32;
        for i in 0..8000 {
            // Broadband, so every part of the response is exercised.
            let t = i as f32;
            let x = 0.3
                * ((t * 0.017).sin() + (t * 0.31).sin() + (t * 1.9).sin() + (t * 2.9).sin())
                / 4.0;
            let fwd = c.forward(x);
            let y = c.inverse(fwd);
            if i > 200 {
                worst = worst.max((y - x).abs());
            }
        }
        assert!(
            worst < 1e-4,
            "Base {base} Depth {depth} Q {q} at {hz} Hz: the pair leaves {worst}"
        );
    }
}

/// Switched off, the section is a wire.
#[test]
fn the_colour_section_is_a_wire_when_it_is_off() {
    let mut c = Color::new(SR);
    c.configure(color::Settings {
        on: false,
        base_db: 24.0,
        ..color::Settings::default()
    });
    for x in [0.0f32, 0.5, -0.9, 1.0] {
        let fwd = c.forward(x);
        assert_eq!(c.inverse(fwd), x);
    }
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

/// **The headline claim, as an identity.** At 0 % wet the output is the input
/// delayed by exactly the figure the host was told, bit for bit, with every
/// other control driven hard. There is no arithmetic in the dry path, so
/// anything but bit-exact would mean the path is not what it claims to be.
#[test]
fn a_fully_dry_setting_is_bit_exact_bypass() {
    for law in 0..2 {
        for factor in 0..4 {
            let mut e = Saturator::new(SR);
            e.configure(&engine::Settings {
                drive_db: 30.0,
                bias: 0.8,
                curve: Curve::Fold.index(),
                output_db: 24.0,
                mix: 0.0,
                mix_law: law,
                color: color::Settings {
                    on: true,
                    base_db: 18.0,
                    depth_db: -12.0,
                    ..color::Settings::default()
                },
                dc_block: true,
                clip_mode: 1,
                clip_knee_db: 10.0,
                oversample: factor,
            });
            let lat = e.latency();
            let n = 1024;
            let sig: Vec<f32> = (0..n)
                .map(|i| 0.7 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / SR).sin())
                .collect();
            let mut l = sig.clone();
            let mut r = sig.clone();
            e.process(&mut l, &mut r);
            for i in lat..n {
                assert_eq!(
                    l[i],
                    sig[i - lat],
                    "law {law} factor {factor} sample {i}: {} against {}",
                    l[i],
                    sig[i - lat]
                );
            }
        }
    }
}

/// The dry delay is always exactly the latency reported to the host. These
/// are two numbers in two places and the whole dry/wet claim is that they
/// agree, so nothing is allowed to move one without the other.
#[test]
fn the_dry_delay_always_matches_the_reported_latency() {
    let mut e = Saturator::new(SR);
    for factor in [0usize, 3, 1, 2, 0, 3] {
        e.configure(&engine::Settings {
            oversample: factor,
            ..engine::Settings::default()
        });
        settle(&mut e, 4);
        let f = e.latency_frame();
        assert_eq!(
            f[0], f[1],
            "factor {factor}: wet delay {} dry {}",
            f[0], f[1]
        );
        assert_eq!(
            f[0], f[2],
            "factor {factor}: reported {} against wet {}",
            f[2], f[0]
        );
        assert_eq!(f[0] as usize, e.latency());
        // The kernel's half-sample is published and excluded, never folded in.
        assert!((f[4] - 0.5 / e.factor() as f32).abs() < 1e-9);
        assert_eq!(
            e.latency(),
            oversample::latency_for_depth(oversample::depth_for_index(factor))
        );
    }
}

/// The transfer curve the page draws has to be the curve the engine is set
/// to, including the drive, the bias, the ceiling and the trim. A panel that
/// draws a different curve from the one it is running is worse than a panel
/// that draws nothing.
#[test]
fn the_published_transfer_curve_is_the_shape_that_is_running() {
    let mut e = Saturator::new(SR);
    let s = engine::Settings {
        drive_db: 12.0,
        bias: 0.3,
        curve: Curve::Round.index(),
        output_db: -6.0,
        clip_mode: 0,
        ..engine::Settings::default()
    };
    e.configure(&s);
    let mut curve = [0.0f32; engine::TRANSFER_POINTS];
    e.transfer(&mut curve);
    let shape = Shape::new(Curve::Round, engine::knee_width(s.clip_knee_db));
    let g = 10f64.powf(s.drive_db as f64 / 20.0);
    let trim = 10f64.powf(s.output_db as f64 / 20.0);
    let rest = shape.f(s.bias as f64);
    for (i, got) in curve.iter().enumerate() {
        let x = engine::TRANSFER_IN as f64
            * (-1.0 + 2.0 * i as f64 / (engine::TRANSFER_POINTS - 1) as f64);
        let want = ((shape.f(g * x + s.bias as f64) - rest) * trim) as f32;
        assert!((got - want).abs() < 1e-5, "point {i}: {got} against {want}");
    }
}

/// The ceiling stage's guarantee, stated as precisely as it can honestly be.
///
/// Two claims, and only the first is exact. **The shape itself never exceeds
/// one**, at any knee, which is checked against the shape directly. **The
/// engine's output stays within a decibel of the Output setting**, and the
/// gap is not slack in the model: band-limiting a hard corner overshoots, so
/// a decimated clipper always rings slightly past its own ceiling and no
/// filter of finite length avoids it. Claiming an exact ceiling after a
/// resampler would be claiming something arithmetic forbids, so the
/// benchmark publishes the overshoot rather than this test hiding it in a
/// tolerance.
///
/// The stage is last in the wet path so that whatever is asserted holds
/// whatever the colour section is doing — including the case Ableton
/// themselves flag, where a negative colour base sends unsaturated low
/// frequencies through the full inverse boost.
#[test]
fn the_ceiling_holds_the_wet_path_at_the_output_setting() {
    // Exact, on the shape itself.
    for knee in [0.0f64, 0.4, 1.0] {
        let s = Shape::new(Curve::Clip, knee);
        for x in [-90.0f64, -1.6, -1.0, 0.0, 1.0, 1.6, 90.0] {
            assert!(s.f(x).abs() <= 1.0, "knee {knee} reaches {} at {x}", s.f(x));
        }
    }
    for mode in [1usize, 2] {
        for output_db in [-12.0f32, 0.0] {
            let mut e = Saturator::new(SR);
            e.configure(&engine::Settings {
                drive_db: 30.0,
                curve: Curve::Warm.index(),
                output_db,
                mix: 1.0,
                color: color::Settings {
                    on: true,
                    base_db: -30.0,
                    ..color::Settings::default()
                },
                dc_block: false,
                clip_mode: mode,
                clip_knee_db: 8.0,
                ..engine::Settings::default()
            });
            let ceiling = 10f32.powf(output_db / 20.0);
            let n = 4096;
            let mut l: Vec<f32> = (0..n)
                .map(|i| {
                    let t = i as f32 / SR;
                    0.9 * ((2.0 * std::f32::consts::PI * 60.0 * t).sin()
                        + (2.0 * std::f32::consts::PI * 3000.0 * t).sin())
                        / 2.0
                })
                .collect();
            let mut r = l.clone();
            e.process(&mut l, &mut r);
            let worst = l[512..].iter().fold(0.0f32, |a, v| a.max(v.abs()));
            let over = 20.0 * (worst / ceiling).log10();
            assert!(
                over <= 1.0,
                "mode {mode} output {output_db} dB: reached {worst}, which is {over:.2} dB over a                  ceiling of {ceiling}"
            );
        }
    }
}

/// Everything a host can ask for has to stay finite, including a sample rate
/// change and a factor change mid-stream.
#[test]
fn the_engine_stays_finite_through_anything() {
    let mut e = Saturator::new(44_100.0);
    let mut n = 0usize;
    for curve in 0..6 {
        for factor in 0..4 {
            e.configure(&engine::Settings {
                drive_db: 36.0,
                bias: 1.0,
                curve,
                output_db: 36.0,
                mix: 0.5,
                mix_law: n % 2,
                dc_block: true,
                clip_mode: n % 3,
                clip_knee_db: 0.0,
                oversample: factor,
                ..engine::Settings::default()
            });
            let mut l: Vec<f32> = (0..512)
                .map(|i| if i % 64 < 32 { 1.0 } else { -1.0 })
                .collect();
            let mut r = l.clone();
            e.process(&mut l, &mut r);
            for v in l.iter().chain(r.iter()) {
                assert!(v.is_finite(), "curve {curve} factor {factor} gave {v}");
            }
            n += 1;
        }
    }
    e.set_sample_rate(96_000.0);
    let mut l = vec![0.5f32; 256];
    let mut r = vec![0.5f32; 256];
    e.process(&mut l, &mut r);
    assert!(l.iter().all(|v| v.is_finite()));
}

// ---------------------------------------------------------------------------
// The aliasing readout
// ---------------------------------------------------------------------------

/// The readout has to say *nothing found* when a clean tone goes through a
/// path that adds nothing, and it has to notice when the same tone is
/// mangled. This is an ordering rather than a bound, because the meter's
/// floor is a property of its window and the benchmark publishes the number.
#[test]
fn the_readout_tells_a_clean_tone_from_a_distorted_one() {
    // Snapped to a bin of the meter's own window, and high enough that a
    // naive shaper's harmonics fold: the fifth of 7 kHz lands at 13 kHz at
    // this rate. A low tone through a naive shaper produces almost no
    // aliasing at all and would test nothing.
    let hz = 597.0 * SR / alias::WINDOW as f32;
    let tone = |i: usize| 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / SR).sin();
    let mut clean = AliasMeter::new(SR);
    let mut dirty = AliasMeter::new(SR);
    let n = alias::WINDOW * 2;
    let inp: Vec<f32> = (0..n).map(tone).collect();
    let same = inp.clone();
    // A deliberately naive waveshaper: no antialiasing at all, run at the
    // base rate, which is the thing this whole plug-in exists not to be.
    let naive: Vec<f32> = inp.iter().map(|x| (x * 12.0).tanh()).collect();
    clean.push(&inp, &same);
    dirty.push(&inp, &naive);
    let c = clean.reading();
    let d = dirty.reading();
    assert!(
        c.confidence > 0.99,
        "a pure tone read confidence {}",
        c.confidence
    );
    assert!(
        (c.f0_hz - hz).abs() < 20.0,
        "the fundamental came out as {} Hz against {hz}",
        c.f0_hz
    );
    assert!(
        d.alias_db > c.alias_db + 30.0,
        "a naive waveshaper read {:.1} dB against a wire's {:.1} dB",
        d.alias_db,
        c.alias_db
    );
    // And the fourth field is the *wanted* distortion, which has to rise
    // when the shaper is working and sit at the floor when it is a wire.
    // Without that pairing an alias reading of −120 dB cannot be told from a
    // shaper that has stopped.
    // An ordering and a direction, not a bound: the wire's own harmonic
    // reading is the window's leakage into the mask, which is a property of
    // the meter and is published as a measured figure in the benchmark
    // rather than guessed at here.
    assert!(
        c.harmonic_db < 0.0,
        "a wire reported {:.1} dB of harmonic distortion, which is above its own fundamental",
        c.harmonic_db
    );
    assert!(
        d.harmonic_db > c.harmonic_db + 60.0,
        "a hard-driven shaper reported {:.1} dB of harmonic distortion against a wire's {:.1}",
        d.harmonic_db,
        c.harmonic_db
    );
}

/// With no periodic content the readout says so rather than printing a
/// number that means nothing.
#[test]
fn the_readout_admits_when_the_input_is_not_a_tone() {
    let mut m = AliasMeter::new(SR);
    let mut rng = 0x1234_5678u32;
    let n = alias::WINDOW * 2;
    let noise: Vec<f32> = (0..n)
        .map(|_| {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            (rng as f32 / u32::MAX as f32) - 0.5
        })
        .collect();
    m.push(&noise, &noise);
    assert!(
        m.reading().confidence < 0.5,
        "noise read a confidence of {}",
        m.reading().confidence
    );
}

// ---------------------------------------------------------------------------
// Parameters and streams
// ---------------------------------------------------------------------------

/// **The ranges and defaults come from the target device's own serialised
/// parameter file**, not from prose — its manual publishes no numeric range
/// for any control. Where we deliberately differ, the difference is the
/// point and the test pins it so it cannot drift back by accident.
#[test]
fn the_parameters_carry_the_ranges_the_dossier_established() {
    let specs = param_specs(false);
    let by_id = |id: &str| specs.iter().find(|s| s.id == id).expect(id);
    // Reproduced from `Saturator.adv`'s serialised MidiControllerRange.
    let drive = by_id("drive");
    assert_eq!((drive.min, drive.max, drive.default), (-36.0, 36.0, 0.0));
    let freq = by_id("color_freq");
    assert_eq!((freq.min, freq.max, freq.default), (30.0, 18_500.0, 1000.0));
    let base = by_id("color_base");
    assert_eq!((base.min, base.max, base.default), (-36.0, 36.0, 0.0));
    let depth = by_id("color_depth");
    assert_eq!((depth.min, depth.max, depth.default), (-24.0, 24.0, 0.0));
    let mix = by_id("mix");
    assert_eq!((mix.min, mix.max, mix.default), (0.0, 100.0, 100.0));
    // Colour is on by default in theirs, so it is here.
    assert_eq!(by_id("color_on").default, 1.0);
    // Deliberate departures, each one an "improve" row in the dossier's
    // control table.
    let out = by_id("output");
    assert_eq!(
        (out.min, out.max),
        (-36.0, 36.0),
        "theirs only attenuates; ours has to be able to give the level back"
    );
    assert_eq!(
        by_id("dc_block").default,
        1.0,
        "theirs ships off and hidden; ours ships on and on the panel"
    );
    // And there is no quality switch, which is the point.
    assert!(
        specs
            .iter()
            .all(|s| s.id != "quality" && s.id != "hi_quality")
    );
}

#[test]
fn the_parameter_and_stream_layout_is_what_the_page_expects() {
    let (bridge, ix) = build_bridge("noob-saturator-test", SR);
    assert_eq!(ix.drive, bridge.index_of("drive").unwrap());
    for (i, id) in [
        "meter", "alias", "transfer", "color", "latency", "spec_in", "spec_out",
    ]
    .iter()
    .enumerate()
    {
        let s = &streams(SR)[i];
        assert_eq!(&s.id, id, "stream {i} moved");
    }
    assert_eq!(STREAM_IX.meter, 0);
    assert_eq!(STREAM_IX.spec_out, 6);
    assert_eq!(streams(SR)[0].capacity, METER_LEN);
    assert_eq!(streams(SR)[1].capacity, ALIAS_LEN);
    assert_eq!(streams(SR)[2].capacity, engine::TRANSFER_POINTS);
    assert_eq!(streams(SR)[3].capacity, engine::COLOR_POINTS);
    assert_eq!(streams(SR)[4].capacity, LATENCY_LEN);
    assert_eq!(streams(SR)[5].capacity, SPECTRUM_BINS);
}

/// **Every published number has to carry its unit**, because the page prints
/// them and a printed number with a guessed unit is the exact defect this
/// plug-in exists to complain about. The `alias` frame in particular holds
/// two levels and a ratio that are easy to mistake for one another.
#[test]
fn every_stream_states_the_unit_of_what_it_publishes() {
    let all = streams(SR);
    let by_id = |id: &str| all.iter().find(|s| s.id == id).expect(id);
    let alias = by_id("alias");
    assert_eq!(
        alias.meta["layout"].as_str().unwrap(),
        "alias_db,f0_hz,confidence,harmonic_db"
    );
    let units: Vec<&str> = alias.meta["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(units, ["dB", "Hz", "ratio", "dB"]);
    assert_eq!(
        alias.meta["band_hz"].as_f64().unwrap() as f32,
        ALIAS_BAND_HZ
    );
    assert_eq!(
        alias.meta["floor_db"].as_f64().unwrap() as f32,
        alias::FLOOR_DB
    );
    let lat = by_id("latency");
    assert_eq!(lat.meta["units"].as_array().unwrap().len(), LATENCY_LEN);
    // The colour shelf has no frequency control, so the panel prints the
    // constant — and it has to reach the page as a number rather than as
    // prose somebody retypes onto the face.
    assert_eq!(
        by_id("color").meta["shelf_hz"].as_f64().unwrap() as f32,
        color::SHELF_HZ
    );
}

/// The knee is a level below the ceiling, so zero decibels has to be a hard
/// corner and a larger number a softer one — not the other way round.
#[test]
fn the_knee_in_decibels_maps_the_way_the_panel_prints_it() {
    assert_eq!(engine::knee_width(0.0), 0.0);
    // Six decibels below the ceiling puts the knee at half of full scale.
    let k = 1.0 - engine::knee_width(6.0206);
    assert!((k - 0.5).abs() < 1e-4, "6 dB gave a knee starting at {k}");
    let mut last = 0.0;
    for db in [0.0f32, 3.0, 6.0, 12.0, 24.0] {
        let w = engine::knee_width(db);
        assert!(w >= last, "the knee narrowed going from {last} to {w}");
        assert!((0.0..=1.0).contains(&w));
        last = w;
    }
}

/// A settings snapshot read back from the bridge at its defaults has to be
/// the engine's own defaults, or the plug-in and the page would disagree
/// about what a fresh instance is.
#[test]
fn the_defaults_round_trip_through_the_bridge() {
    let (bridge, ix) = build_bridge("noob-saturator-defaults", SR);
    let audio = bridge.take_audio().expect("audio handle");
    let got = read_settings(&audio, &ix);
    let want = engine::Settings::default();
    assert_eq!(got.curve, want.curve);
    assert_eq!(got.oversample, want.oversample);
    assert_eq!(got.dc_block, want.dc_block);
    assert_eq!(got.clip_mode, want.clip_mode);
    assert_eq!(got.mix_law, want.mix_law);
    assert!((got.drive_db - want.drive_db).abs() < 1e-4);
    assert!((got.mix - want.mix).abs() < 1e-4);
    assert!((got.color.freq_hz - want.color.freq_hz).abs() < 1.0);
    assert_eq!(got.color.on, want.color.on);
}

/// The processor publishes without panicking and the alignment frame it
/// publishes is the one the engine reports.
#[test]
fn the_processor_publishes_a_consistent_frame() {
    let (bridge, ix) = build_bridge("noob-saturator-publish", SR);
    let mut audio = bridge.take_audio().expect("audio handle");
    let mut p = Processor::new(SR);
    let s = read_settings(&audio, &ix);
    p.configure(&s);
    let mut l = vec![0.0f32; 256];
    let mut r = vec![0.0f32; 256];
    for _ in 0..8 {
        for (i, v) in l.iter_mut().enumerate() {
            *v = 0.4 * (i as f32 * 0.1).sin();
        }
        r.copy_from_slice(&l);
        p.process(&mut l, &mut r);
        p.publish(&mut audio);
    }
    assert_eq!(p.latency(), p.engine().latency());
    let f = p.engine().latency_frame();
    assert_eq!(f[0], f[1]);
    assert_eq!(f[0], f[2]);
    assert_eq!(f[3], SR);
}
