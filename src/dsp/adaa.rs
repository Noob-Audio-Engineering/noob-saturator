//! First-order antiderivative antialiasing, and the one constant that
//! decides whether it works at all.
//!
//! A memoryless waveshaper computes `y[n] = f(x[n])`, which is not
//! bandlimited: a sine through it makes harmonics without end, everything
//! above Nyquist folds back inharmonically, and the folded partials move
//! *downward* as the input moves upward. That is the defect this whole
//! plug-in exists to avoid.
//!
//! Parker, Zavalishin and Le Bivic's answer is to compute what the shaper
//! would have produced in continuous time, filtered before sampling.
//! Approximate the signal as linear between the samples you actually have,
//! integrate the shaper across that segment, and the integral has a closed
//! form — their equation (9):
//!
//! ```text
//!            F₀(u[n]) − F₀(u[n−1])
//!   y[n]  =  ─────────────────────
//!               u[n] − u[n−1]
//! ```
//!
//! Read it as physics rather than algebra and it is obvious: the quotient is
//! the **average value of `f` over the segment the signal actually
//! traversed**, instead of `f` sampled at one end of it. A corner gets
//! smeared across the segment it was crossed in rather than sampled through.
//! It costs one extra antiderivative evaluation, one subtraction and one
//! divide — six floating-point operations against the several hundred that
//! eight-times oversampling costs, and it buys more than eight-times
//! oversampling does.
//!
//! ## Where the drive and the bias went
//!
//! The stage this module implements is `y(x) = f(g·x + b) − f(b)`: a drive
//! gain and an operating-point offset around the shape, with the offset's
//! own output subtracted so the stage rests at zero. Asymmetry is what makes
//! a saturator sound like a valve rather than a clipper, and it is free
//! here — `f(g·x)` has antiderivative `F₀(g·x)/g`, and `f(x + b)` has
//! `F₀(x + b)`, so neither reshapes anything the antiderivative depends on.
//!
//! Working that through, the whole stage collapses to the plain quotient in
//! the shaper's own domain. Writing `u = g·x + b`:
//!
//! ```text
//!   Φ(x) = [F₀(g·x + b) − F₀(b)] / g − f(b)·x
//!
//!   Φ(x) − Φ(x₋₁)     F₀(u) − F₀(u₋₁)
//!   ───────────────  =  ───────────────  −  f(b)
//!     x − x₋₁            u − u₋₁
//! ```
//!
//! The gain cancels exactly. So drive, bias and trim never touch the
//! antiderivative, the threshold below always means the same thing, and the
//! knobs can move between samples: this stores the previous **input**, not
//! the previous shaper argument, and rebuilds both ends of the segment at
//! the gain and bias now in force.
//!
//! ## Group delay
//!
//! At small signals many shapers are transparent, `f(x) ≈ x`, and the whole
//! method reduces to `y[n] = (x[n] + x[n−1])/2` — a linear interpolator, so
//! **half a sample of group delay at the rate the shaper runs at**. Inside
//! four-times oversampling that is an eighth of a base-rate sample, which is
//! fractional and so cannot be matched by an integer delay line. It is
//! deliberately left out of the latency the plug-in reports, and
//! [`super::oversample`] says why that is the right call rather than a
//! rounding error: a sub-sample delay puts its first comb null above
//! Nyquist at any oversampling factor, so it cannot produce the in-band
//! cancellation the reported figure exists to prevent.
//!
//! The same filter is a **droop** as well as a delay, `|cos(ω/2)|`, and that
//! one is not negligible: 16.7 dB at 20 kHz un-oversampled, 2.4 dB at two
//! times, 0.56 dB at four. It is the reason this device oversamples at all,
//! and the reason the lowest factor it offers is two rather than one.

use super::curve::Shape;

/// Steps closer together than this take the midpoint instead of the
/// quotient.
///
/// **This constant and the antiderivative's precision are one decision, not
/// two**, and getting the pairing wrong destroys the antialiasing silently:
/// the transfer curve stays right, the sound stays saturated, nothing
/// crashes, and the alias floor collapses by forty decibels.
///
/// The mechanism is in [`Self`]'s parent module. Near a turning point the
/// quotient divides a difference of two nearly equal antiderivatives by a
/// nearly zero step, so an error `r` in `F₀` reaches the output amplified by
/// roughly `2·r·|F₀|/|Δu|`. Two numbers therefore have to be chosen
/// together:
///
/// | quantity | value here | why |
/// |---|---|---|
/// | `F₀`'s relative error | [`F0_RELATIVE_ERROR`], about `1e−15` | every curve has a closed-form antiderivative evaluated in `f64` (`super::curve`), so there is no table and no interpolation error — only a few units in the last place |
/// | the threshold | `0.01` | the floor the amplification imposes is `√r ≈ 3e−8`, so this clears it by three hundred thousand times; the ceiling is that the threshold must stay small against the signal, and a measured sweep puts `0.01` safely below any signal above −40 dBFS |
///
/// The published practice that would be wrong here is `ε = 1e−8`, which is
/// right for a polynomial antiderivative exact to machine precision and
/// catastrophic for a tabulated one: measured, with `F₀` carrying a
/// part-per-thousand table error, `ε = 1e−6` gives **−4.6 dB** of alias
/// rejection where `ε = 1e−2` gives **−44.3 dB** (`ANTIALIASING.md` §4.1).
/// This build sits at the safe end of both constraints because it pays for
/// `f64` closed forms instead of a table, and `tests.rs` asserts the pairing
/// two ways: arithmetically, that `MIN_STEP² ≥ F0_RELATIVE_ERROR`, and
/// behaviourally, that deliberately spoiling `F₀` to a table's precision
/// while holding the threshold at the textbook value reproduces the
/// collapse.
pub const MIN_STEP: f64 = 0.01;

/// The relative error the antiderivative is claimed to carry.
///
/// `super::curve` evaluates every `F₀` in closed form in `f64`, from
/// compositions of `ln_1p`, `exp`, `sqrt`, `atan` and `cos`, so a few units
/// in the last place is the honest figure and `1e−15` is generous. Raise
/// this if the antiderivative ever becomes a table, and [`MIN_STEP`] has to
/// rise with it — the test that pins them together will say so.
pub const F0_RELATIVE_ERROR: f64 = 1e-15;

/// One shaping stage, antialiased.
///
/// Feed it samples in order; it holds the previous one so it can integrate
/// across the segment between them.
///
/// ## What is cached, and why it is safe
///
/// Written out plainly the quotient needs three evaluations per sample:
/// `F₀` at each end of the segment and `f` at the bias. Two of the three are
/// avoidable, and at sixteen times oversampling — where the shape runs
/// 705,600 times a second per channel and `ln cosh` is two transcendental
/// calls — that decides whether the factor is affordable.
///
/// `F₀` at the near end of one segment is `F₀` at the far end of the next,
/// **provided the gain, the bias and the shape have not moved between
/// them**. So the cache is keyed on all three and falls back to a fresh
/// evaluation when any of them changes. Within one base-rate sample they
/// cannot change, because the parameters are smoothed once per base sample
/// and held across the oversampled ones, so even a knob in motion misses the
/// cache once in every `factor` calls. `f(bias)` is cached on the bias for
/// the same reason.
#[derive(Clone, Copy, Debug)]
pub struct Adaa {
    /// The previous **input**, before the gain and the bias, so that both
    /// ends of the segment can be rebuilt at the settings now in force.
    x1: f64,
    /// The settings the cache below was filled at.
    key: Option<(Shape, f64, f64)>,
    /// `F₀` at the previous sample's shaper argument.
    f0_x1: f64,
    /// `f(bias)`, which the stage subtracts so it rests at zero.
    rest: f64,
    /// The ill-conditioning threshold actually used. [`MIN_STEP`] in every
    /// shipped path; a field only so the pairing above can be tested by
    /// moving one of the two numbers without the other.
    threshold: f64,
    /// A relative rounding deliberately applied to `F₀`, emulating a
    /// tabulated antiderivative. Zero in every shipped path, and the branch
    /// that reads it is the same one every sample. It exists for the same
    /// reason as `threshold`: a claim that two constants must move together
    /// is worth nothing without a test that moves one of them.
    f0_error: f64,
}

impl Default for Adaa {
    fn default() -> Self {
        Adaa::new()
    }
}

impl Adaa {
    pub fn new() -> Self {
        Adaa::with_precision(MIN_STEP, 0.0)
    }

    /// A stage with the threshold and the antiderivative's precision set by
    /// hand. Only the pairing test uses this.
    pub fn with_precision(threshold: f64, f0_error: f64) -> Self {
        Adaa {
            x1: 0.0,
            key: None,
            f0_x1: 0.0,
            rest: 0.0,
            threshold,
            f0_error,
        }
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.key = None;
    }

    /// `F₀`, spoiled to `f0_error` if the pairing is being tested.
    ///
    /// The perturbation is **deterministic in the argument**, because that is
    /// what a lookup table is: the same `u` always reads back the same wrong
    /// value, and it is wrong by up to `f0_error` relatively. An error that
    /// varied between two reads of the same point would be dither, and dither
    /// is a different and much kinder failure than a table's.
    #[inline]
    fn f0(&self, shape: &Shape, u: f64) -> f64 {
        let v = shape.f0(u);
        if self.f0_error <= 0.0 {
            return v;
        }
        let h = v.to_bits().wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 11;
        let d = (h as f64 / (1u64 << 53) as f64) * 2.0 - 1.0;
        v * (1.0 + self.f0_error * d)
    }

    /// One sample through `y(x) = f(g·x + b) − f(b)`, antialiased.
    ///
    /// `gain` and `bias` may move between calls; both ends of the segment
    /// are rebuilt at the values passed in, so a knob turning mid-block
    /// changes the stage rather than corrupting its history.
    #[inline]
    pub fn process(&mut self, shape: &Shape, x: f32, gain: f64, bias: f64) -> f32 {
        let x = x as f64;
        let u = gain * x + bias;
        let u1 = gain * self.x1 + bias;
        let key = (*shape, gain, bias);
        if self.key != Some(key) {
            self.key = Some(key);
            self.f0_x1 = self.f0(shape, u1);
            self.rest = shape.f(bias);
        }
        let f0_u = self.f0(shape, u);
        let d = u - u1;
        let y = if d.abs() > self.threshold {
            (f0_u - self.f0_x1) / d - self.rest
        } else {
            // Below the threshold the midpoint is both more accurate and
            // just as correct: its error is second order in the step, and a
            // signal moving this slowly has nothing near Nyquist to alias,
            // which is the only thing the quotient is here to fix.
            shape.f(0.5 * (u + u1)) - self.rest
        };
        self.x1 = x;
        self.f0_x1 = f0_u;
        y as f32
    }

    /// One sample through a plain `f(x)`, antialiased. The ceiling stage
    /// after the shaper uses this: it has no drive and no offset of its own.
    #[inline]
    pub fn process_plain(&mut self, shape: &Shape, x: f32) -> f32 {
        self.process(shape, x, 1.0, 0.0)
    }
}
