//! The whole device: the wet path, the dry path, and the mix that has to be
//! flat wherever it is set.
//!
//! ## The chain
//!
//! ```text
//!   in ─┬─ dry delay (exactly the resampler's round trip) ───────────────┐
//!       │                                                                │
//!       └─ DC block ─ up ─┬─ colour H ─ shaper ─ colour H⁻¹ ─ ceiling ─ trim ─┬─ down ─┤
//!                          └────── all of this at the oversampled rate ──────┘         │
//!                                                                                  mix ─▶ out
//! ```
//!
//! Four decisions in that picture are worth stating, because each of them
//! is the point of something the device this one answers gets wrong.
//!
//! **The dry path is delayed to match.** Not band-limited, not resampled,
//! not approximated: held in a whole-sample delay line exactly as long as
//! the resampler's round trip, which is why the tap count in
//! [`super::oversample`] is what it is. Mixing an undelayed dry path with a
//! delayed wet one is a comb filter, and it measures eighteen decibels deep
//! across the audio band. At 0 % wet the output is the input, bit for bit,
//! delayed by the figure the host was told.
//!
//! **There is no quality mode.** The device is always antialiased. The
//! oversampling factor is a visible, automatable control with a published
//! aliasing figure for each of its settings, which is a trade-off with a
//! number on it rather than a hidden switch. Ableton made exactly this
//! decision for Operator and for EQ Eight and never applied it here.
//!
//! **The latency is declared unconditionally**, at the host rate, at every
//! sample rate, and it is compensated internally so the dry path always
//! matches. The antialiasing kernel's own half-sample is deliberately not
//! folded into the figure: it is fractional, so no integer delay can carry
//! it, and rounding it in would make the reported number wrong by more than
//! the error it fixed. A sub-sample delay puts its first comb null above
//! Nyquist at any factor, which is why it can be left alone.
//!
//! **The ceiling is last in the wet path.** So whatever it guarantees holds
//! whatever the Colour controls are doing, including the case Ableton
//! themselves flag, where a negative Colour Base sends unsaturated low
//! frequencies through the full inverse boost and lifts the level.
//!
//! What it guarantees is worth stating exactly, because Ableton's wording —
//! "will never exceed the level set by the Output control" — is stronger
//! than any band-limited system can deliver. **The shape never exceeds the
//! setting**, and that part is exact. **The output stays within a decibel of
//! it**, and the difference is the decimator ringing past a hard corner,
//! which is what band-limiting a discontinuity does and which no filter of
//! finite length avoids. The benchmark publishes the overshoot rather than
//! the documentation rounding it away.
//!
//! ## What is not claimed
//!
//! That the dry/wet sum is *perfectly* flat, which is not achievable and
//! would be a lie. The wet path has a droop of its own — the antialiasing
//! kernel's `|cos(ω/2)|` and the decimator's passband — so a half-and-half
//! mix necessarily averages a flat path with a slightly drooped one. The
//! promise is narrower and it is the one that matters: **the dry/wet control
//! introduces no cancellation.** Residual droop belongs to the wet path and
//! is attacked there, by the oversampling factor and the filter length.

use super::adaa::Adaa;
use super::alias::AliasMeter;
use super::color::{self, Color};
use super::curve::{Curve, Shape};
use super::oversample::{self, DryDelay, MAX_FACTOR, Resampler};

/// Corner of the direct-current filters, both of them.
///
/// **The one before the shape** is not a preference: an asymmetric shaper
/// eats headroom asymmetrically when it is fed an offset, and a clipper
/// wastes it entirely. Ableton ship theirs off and, in the current version,
/// hidden in a context menu; here it is on the panel, on by default, and the
/// corner is printed rather than left to be guessed.
///
/// **The one after it is ours, and Bias is why.** A biased shape rectifies:
/// measured, the Soft curve at the maximum bias and a moderate drive puts
/// **39 % of full scale** of steady offset on its output, which is what
/// asymmetry *is* and not a fault in the shape. Ableton have no bias control
/// and so no reason for a second filter; we added the control, so we own the
/// consequence rather than leaving it in the signal for somebody's next
/// device to trip over.
///
/// Both sit in the wet path, so the dry signal is untouched and a fully dry
/// setting stays bit-exact. Both together cost 0.5 dB at 20 Hz at a
/// half-and-half mix, which the benchmark measures with the filters on and
/// off so the two effects can be told apart.
pub const DC_HZ: f32 = 5.0;

/// Longest block the input scratch covers. A longer one is processed
/// normally; only the aliasing readout, which is a meter, skips it.
pub const MAX_BLOCK: usize = 8192;

/// Points in the transfer curve the page draws.
pub const TRANSFER_POINTS: usize = 257;
/// Points in the colour curve the page draws.
pub const COLOR_POINTS: usize = 129;
/// The colour curve's span.
pub const COLOR_MIN_HZ: f32 = 20.0;
pub const COLOR_MAX_HZ: f32 = 20_000.0;

/// How the dry/wet control crossfades.
pub const MIX_LAW_NAMES: [&str; 2] = ["Linear", "Equal Power"];
/// The ceiling stage's modes.
pub const CLIP_MODE_NAMES: [&str; 3] = ["Off", "Soft", "Hard"];

/// A one-pole smoother for a continuous control, so a knob does not step.
#[derive(Clone, Copy, Debug, Default)]
struct Smooth {
    cur: f32,
    target: f32,
    coef: f32,
}

impl Smooth {
    fn new(v: f32) -> Self {
        Smooth {
            cur: v,
            target: v,
            coef: 0.0,
        }
    }

    fn set_rate(&mut self, ms: f32, sr: f32) {
        self.coef = (-1.0 / (ms * 1e-3 * sr)).exp();
    }

    fn set(&mut self, v: f32) {
        self.target = v;
    }

    fn snap(&mut self, v: f32) {
        self.target = v;
        self.cur = v;
    }

    #[inline]
    fn next(&mut self) -> f32 {
        self.cur = self.target + (self.cur - self.target) * self.coef;
        if (self.cur - self.target).abs() < 1e-9 {
            self.cur = self.target;
        }
        self.cur
    }
}

/// A one-pole direct-current blocker.
#[derive(Clone, Copy, Debug, Default)]
struct DcBlock {
    x1: f32,
    y1: f32,
    a: f32,
}

impl DcBlock {
    fn set_sample_rate(&mut self, sr: f32) {
        self.a = (-2.0 * std::f32::consts::PI * DC_HZ / sr).exp();
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.a * self.y1;
        self.x1 = x;
        // Keep a decaying tail out of the subnormal range, where arithmetic
        // is slow on some hardware.
        self.y1 = if y.abs() < 1e-12 { 0.0 } else { y };
        self.y1
    }
}

/// Everything the engine is set to. Read once per block from the
/// parameters; the continuous controls are smoothed inside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Pre-gain into the shape, dB.
    pub drive_db: f32,
    /// Operating-point offset, which is what makes a saturator sound like a
    /// valve rather than a clipper.
    pub bias: f32,
    /// Which curve, as a menu index.
    pub curve: usize,
    /// Trim after the ceiling, dB.
    pub output_db: f32,
    /// Wet share, 0 to 1.
    pub mix: f32,
    /// 0 linear, 1 equal power.
    pub mix_law: usize,
    pub color: color::Settings,
    pub dc_block: bool,
    /// 0 off, 1 soft, 2 hard.
    pub clip_mode: usize,
    /// Knee width of the soft clipper, shared by the `Clip` curve and the
    /// ceiling stage, because they are the same shape.
    pub clip_knee: f32,
    /// Oversampling factor as a menu index: 2x, 4x, 8x, 16x.
    ///
    /// The default is **sixteen times**, which is higher than the design
    /// document recommended and is a measurement rather than a preference.
    /// Measured over a ten-tone sweep at a hot drive, four times reaches
    /// −57 to −65 dB and eight times −62 to −73, against a target of −80;
    /// sixteen reaches −85 to −90 on every curve. A single 15 kHz tone
    /// flatters the lower factors badly — it puts four times at −75 — which
    /// is exactly the narrow measurement this project has been caught by
    /// before. The cost is 8.5 % of one core for a stereo instance at
    /// 48 kHz with everything engaged, against 2.1 % at four times, and the
    /// control is on the panel with its figure printed so that trade is the
    /// user's to make rather than ours to hide.
    pub oversample: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            drive_db: 0.0,
            bias: 0.0,
            curve: Curve::default().index(),
            output_db: 0.0,
            mix: 1.0,
            mix_law: 0,
            color: color::Settings::default(),
            dc_block: true,
            clip_mode: 0,
            clip_knee: 0.5,
            oversample: 3,
        }
    }
}

/// The engine. One instance carries both channels.
pub struct Saturator {
    sr: f32,
    settings: Settings,
    configured: bool,
    resampler: [Resampler; 2],
    shaper: [Adaa; 2],
    ceiling: [Adaa; 2],
    color: [Color; 2],
    dc_pre: [DcBlock; 2],
    dc_post: [DcBlock; 2],
    dry: [DryDelay; 2],
    drive: Smooth,
    bias: Smooth,
    trim: Smooth,
    dry_c: Smooth,
    wet_c: Smooth,
    shape: Shape,
    clip_shape: Shape,
    clip_on: bool,
    meter: [f32; 4],
    alias: AliasMeter,
    scratch_in: Vec<f32>,
    scratch_out: Vec<f32>,
}

impl Saturator {
    pub fn new(sr: f32) -> Self {
        let s = Settings::default();
        let depth = oversample::depth_for_index(s.oversample);
        let mut e = Saturator {
            sr,
            settings: s,
            configured: false,
            resampler: [Resampler::new(depth), Resampler::new(depth)],
            shaper: [Adaa::new(); 2],
            ceiling: [Adaa::new(); 2],
            color: [Color::new(sr), Color::new(sr)],
            dc_pre: [DcBlock::default(); 2],
            dc_post: [DcBlock::default(); 2],
            dry: [
                DryDelay::new(oversample::latency_for_depth(depth)),
                DryDelay::new(oversample::latency_for_depth(depth)),
            ],
            drive: Smooth::new(1.0),
            bias: Smooth::new(0.0),
            trim: Smooth::new(1.0),
            dry_c: Smooth::new(0.0),
            wet_c: Smooth::new(1.0),
            shape: Shape::default(),
            clip_shape: Shape::hard_clip(),
            clip_on: false,
            meter: [0.0; 4],
            alias: AliasMeter::new(sr),
            scratch_in: vec![0.0; MAX_BLOCK],
            scratch_out: vec![0.0; MAX_BLOCK],
        };
        e.set_sample_rate(sr);
        e
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
        for d in self.dc_pre.iter_mut().chain(self.dc_post.iter_mut()) {
            d.set_sample_rate(sr);
        }
        // Twenty milliseconds is slow enough that a swept knob does not
        // step and fast enough that automation lands where it was drawn.
        for s in [
            &mut self.drive,
            &mut self.bias,
            &mut self.trim,
            &mut self.dry_c,
            &mut self.wet_c,
        ] {
            s.set_rate(20.0, sr);
        }
        self.alias.set_sample_rate(sr);
        let osr = sr * self.resampler[0].factor() as f32;
        for c in self.color.iter_mut() {
            c.set_sample_rate(osr);
        }
        self.reset();
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    pub fn reset(&mut self) {
        for i in 0..2 {
            self.resampler[i].reset();
            self.shaper[i].reset();
            self.ceiling[i].reset();
            self.color[i].reset();
            self.dc_pre[i].reset();
            self.dc_post[i].reset();
            self.dry[i].reset();
        }
        self.meter = [0.0; 4];
        self.alias.reset();
    }

    /// The latency the host is told, in base-rate samples.
    ///
    /// This is the resampler's round trip and nothing else. The
    /// antialiasing kernel's half-sample at the oversampled rate is
    /// deliberately excluded — see the module documentation.
    pub fn latency(&self) -> usize {
        self.resampler[0].latency()
    }

    /// The oversampling factor now in force.
    pub fn factor(&self) -> usize {
        self.resampler[0].factor()
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Take a fresh snapshot. Cheap enough to call every block.
    pub fn configure(&mut self, s: &Settings) {
        let first = !self.configured;
        let depth = oversample::depth_for_index(s.oversample);
        if !first && depth != self.resampler[0].depth() {
            // A cascade of a different depth is a different filter; the dry
            // delay changes with it and both are cleared together, so the
            // two can never drift apart.
            for i in 0..2 {
                self.resampler[i].set_depth(depth);
                self.dry[i].set_len(oversample::latency_for_depth(depth));
                self.shaper[i].reset();
                self.ceiling[i].reset();
            }
            let osr = self.sr * self.resampler[0].factor() as f32;
            for c in self.color.iter_mut() {
                c.set_sample_rate(osr);
            }
        } else if first {
            for i in 0..2 {
                self.resampler[i].set_depth(depth);
                self.dry[i].set_len(oversample::latency_for_depth(depth));
            }
            let osr = self.sr * self.resampler[0].factor() as f32;
            for c in self.color.iter_mut() {
                c.set_sample_rate(osr);
            }
        }

        self.shape = Shape::new(Curve::from_index(s.curve), s.clip_knee as f64);
        self.clip_on = s.clip_mode != 0;
        self.clip_shape = Shape::new(
            Curve::Clip,
            if s.clip_mode == 2 {
                0.0
            } else {
                s.clip_knee as f64
            },
        );
        for c in self.color.iter_mut() {
            c.configure(s.color);
        }

        let drive = 10f32.powf(s.drive_db / 20.0);
        let trim = 10f32.powf(s.output_db / 20.0);
        let m = s.mix.clamp(0.0, 1.0);
        let (dry_c, wet_c) = if s.mix_law == 1 {
            let a = m * std::f32::consts::FRAC_PI_2;
            (a.cos(), a.sin())
        } else {
            (1.0 - m, m)
        };
        if first {
            self.drive.snap(drive);
            self.bias.snap(s.bias);
            self.trim.snap(trim);
            self.dry_c.snap(dry_c);
            self.wet_c.snap(wet_c);
            self.configured = true;
        } else {
            self.drive.set(drive);
            self.bias.set(s.bias);
            self.trim.set(trim);
            self.dry_c.set(dry_c);
            self.wet_c.set(wet_c);
        }
        self.settings = *s;
    }

    /// One block, in place.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());
        if n == 0 {
            return;
        }
        let metered = n <= MAX_BLOCK;
        if metered {
            self.scratch_in[..n].copy_from_slice(&l[..n]);
        } else {
            // A block this long would splice the readout's analysis window,
            // so it drops the partial window rather than measuring nonsense.
            self.alias.reset();
        }

        let factor = self.resampler[0].factor();
        let mut peaks = [0.0f32; 4];
        let mut buf = [0.0f32; MAX_FACTOR];

        for i in 0..n {
            let g = self.drive.next() as f64;
            let b = self.bias.next() as f64;
            let trim = self.trim.next();
            let dry_c = self.dry_c.next();
            let wet_c = self.wet_c.next();
            let dc_on = self.settings.dc_block;

            let xs = [l[i], r[i]];
            peaks[0] = peaks[0].max(xs[0].abs());
            peaks[1] = peaks[1].max(xs[1].abs());

            for ch in 0..2 {
                let x = xs[ch];
                let dry = self.dry[ch].process(x);
                let mut w = x;
                if dc_on {
                    w = self.dc_pre[ch].process(w);
                }
                self.resampler[ch].up(w, &mut buf);
                for s in buf.iter_mut().take(factor) {
                    let mut v = self.color[ch].forward(*s);
                    v = self.shaper[ch].process(&self.shape, v, g, b);
                    v = self.color[ch].inverse(v);
                    if self.clip_on {
                        v = self.ceiling[ch].process_plain(&self.clip_shape, v);
                    }
                    *s = v * trim;
                }
                let mut wet = self.resampler[ch].down(&buf);
                if dc_on {
                    wet = self.dc_post[ch].process(wet);
                }
                let y = dry * dry_c + wet * wet_c;
                if ch == 0 {
                    l[i] = y;
                } else {
                    r[i] = y;
                }
                peaks[2 + ch] = peaks[2 + ch].max(y.abs());
            }
        }

        self.meter = peaks;
        if metered {
            self.scratch_out[..n].copy_from_slice(&l[..n]);
            self.alias
                .push(&self.scratch_in[..n], &self.scratch_out[..n]);
        }
    }

    /// `[in_l, in_r, out_l, out_r]`, linear peaks, one frame per block.
    pub fn meter(&self) -> [f32; 4] {
        self.meter
    }

    /// The aliasing readout's most recent frame.
    pub fn alias_frame(&self) -> [f32; 4] {
        self.alias.reading().to_frame()
    }

    /// The static transfer curve: the wet path's shaper, ceiling and trim,
    /// with no colour, no resampler and no mix, for input swept −1 to +1.
    ///
    /// This is the plain shaper rather than the antialiased one, because a
    /// transfer curve is a statement about amplitude and the antialiased
    /// form is a statement about a *segment*. Drawing the segment average
    /// would draw the wrong picture.
    pub fn transfer(&self, out: &mut [f32]) {
        let n = out.len();
        if n == 0 {
            return;
        }
        let s = &self.settings;
        let g = 10f64.powf(s.drive_db as f64 / 20.0);
        let b = s.bias as f64;
        let trim = 10f64.powf(s.output_db as f64 / 20.0);
        let rest = self.shape.f(b);
        for (i, o) in out.iter_mut().enumerate() {
            let x = -1.0 + 2.0 * i as f64 / (n - 1).max(1) as f64;
            let mut y = self.shape.f(g * x + b) - rest;
            if self.clip_on {
                y = self.clip_shape.f(y);
            }
            *o = (y * trim) as f32;
        }
    }

    /// The colour section's forward magnitude in dB, log-spaced from
    /// [`COLOR_MIN_HZ`] to [`COLOR_MAX_HZ`]. This is the pre-shaper curve:
    /// the emphasis the shape actually sees.
    pub fn color_curve(&self, out: &mut [f32]) {
        let n = out.len();
        if n == 0 {
            return;
        }
        let ratio = (COLOR_MAX_HZ / COLOR_MIN_HZ) as f64;
        for (i, o) in out.iter_mut().enumerate() {
            let t = i as f64 / (n - 1).max(1) as f64;
            let hz = COLOR_MIN_HZ as f64 * ratio.powf(t);
            *o = self.color[0].forward_db(hz as f32);
        }
    }

    /// `[wet_delay, dry_delay, factor, latency_ms]` — the alignment
    /// indicator. The first two are always equal; that is the point of
    /// showing them.
    pub fn align_frame(&self) -> [f32; 4] {
        let lat = self.latency();
        [
            lat as f32,
            self.dry[0].len() as f32,
            self.factor() as f32,
            lat as f32 * 1000.0 / self.sr,
        ]
    }
}
