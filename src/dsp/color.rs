//! The Colour section: an equaliser applied before the shaper and its exact
//! algebraic inverse applied after it.
//!
//! This is the one piece of the target device's design worth adopting
//! wholesale, and it is worth saying so plainly. Read as a tone control it
//! looks unremarkable. It is not a tone control. The forward filter decides
//! **which bands reach the nonlinear part of the curve**; the inverse puts
//! the spectrum back. What survives is the distortion the forward emphasis
//! created, spectrally shaped by the inverse on the way out — which is
//! pre-emphasis and de-emphasis, the idea behind tape and FM, applied to a
//! waveshaper. It is what makes a saturator usable on a full mix in a way a
//! bare clipper is not.
//!
//! ```text
//!   … ──▶ H(z) ──▶ [ curve ] ──▶ H⁻¹(z) ──▶ …
//! ```
//!
//! `H` is a low shelf (Base, ±36 dB, corner fixed at [`SHELF_HZ`]) in series
//! with a peaking bell (Depth, ±24 dB, with its own frequency and Q).
//!
//! ## What we add: the inverse is exact, and provably
//!
//! Whether the target device's inverse is a true inverse is not established
//! anywhere, and it is a strong, falsifiable prediction either way: if the
//! pair is exact then with the drive low enough that the curve is in its
//! linear region the whole Colour section is **perfectly transparent at
//! every setting**. Ours is, and the reason is structural rather than
//! careful.
//!
//! Take the Robert Bristow-Johnson peaking equaliser. Its coefficients are
//!
//! ```text
//!   b = [ 1 + αA,  −2cos ω₀,  1 − αA ]        a = [ 1 + α/A,  −2cos ω₀,  1 − α/A ]
//! ```
//!
//! with `α = sin ω₀ / 2Q`, which does not depend on the gain. Substituting
//! `A → 1/A` swaps `b` and `a` exactly, so the reciprocal-gain filter **is**
//! `1/H(z)`, pole for zero. The same substitution does the same thing to the
//! low shelf, up to a factor of `A²` that appears in the numerator and the
//! denominator alike and cancels; with the shelf slope at `S = 1` the `α`
//! there does not depend on the gain either. So the inverse is not an
//! approximation fitted to the forward filter, it is the same design
//! evaluated at the reciprocal gain, and the cascade nulls to the precision
//! of the arithmetic. `tests.rs` asserts it as an identity rather than as a
//! bound, which is the honest form for a claim of this kind.
//!
//! Two consequences worth knowing. Both filters are minimum-phase IIR, so
//! the section adds **no latency** and nothing here enters the figure the
//! plug-in reports. And a negative Base means: cut lows before the shaper,
//! boost lows after it — so unsaturated low frequencies get the full inverse
//! boost and the output level rises. That is what the ceiling stage in
//! [`super::engine`] is for, and it is the target device's own stated reason
//! for having one.
//!
//! ## Where they sit
//!
//! Inside the oversampled region, both of them, which is a question the
//! target device leaves open. Two things follow from putting them there and
//! neither is available otherwise: the ceiling stage can be the **last**
//! thing in the wet path, so its guarantee holds whatever the Colour
//! controls are doing, and the forward and inverse filters run at the same
//! rate as each other with nothing between them but the shaper, so the null
//! is exact rather than exact-to-the-resampler.
//!
//! ## State is `f64`
//!
//! At ±36 dB the forward filter and its inverse are a long way from unity,
//! and a null is a difference of large nearly equal numbers. Single-precision
//! state would put the residual around −100 dB and make the guarantee a
//! measurement rather than an identity. The states are `f64`; the cost is
//! four biquads.

use std::f64::consts::PI;

/// The low shelf's corner. Ableton's Base control has no frequency of its
/// own — the manual says only "very low frequencies" — so ours is fixed, and
/// stating the number is the improvement.
pub const SHELF_HZ: f32 = 150.0;

/// Lowest and highest the bell's frequency reaches, from the target
/// device's own serialised range.
pub const FREQ_MIN_HZ: f32 = 30.0;
pub const FREQ_MAX_HZ: f32 = 18_500.0;

/// One biquad in transposed direct form II, with `f64` state.
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// A pass-through.
    pub fn unity() -> Self {
        Biquad {
            b0: 1.0,
            ..Default::default()
        }
    }

    /// Set the coefficients, normalising by `a0` and keeping the state.
    fn set(&mut self, b: [f64; 3], a: [f64; 3]) {
        let inv = 1.0 / a[0];
        self.b0 = b[0] * inv;
        self.b1 = b[1] * inv;
        self.b2 = b[2] * inv;
        self.a1 = a[1] * inv;
        self.a2 = a[2] * inv;
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y as f32
    }

    /// Magnitude at `hz`, for the curve the page draws. Evaluated from the
    /// coefficients rather than from a second copy of the design, so the
    /// drawn curve is the filter that is running.
    pub fn magnitude(&self, hz: f32, sr: f32) -> f32 {
        let w = 2.0 * PI * hz as f64 / sr as f64;
        let (c1, s1) = (w.cos(), w.sin());
        let (c2, s2) = ((2.0 * w).cos(), (2.0 * w).sin());
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -(self.b1 * s1 + self.b2 * s2);
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -(self.a1 * s1 + self.a2 * s2);
        let num = (nr * nr + ni * ni).sqrt();
        let den = (dr * dr + di * di).sqrt();
        (num / den.max(1e-30)) as f32
    }
}

/// A low shelf at `hz` with gain `db`, Robert Bristow-Johnson's design at
/// shelf slope `S = 1`.
///
/// Passing `-db` gives the exact algebraic inverse: at `S = 1` the `α` term
/// carries no dependence on the gain, and substituting `A → 1/A` exchanges
/// the numerator and the denominator up to a common factor.
fn low_shelf(sr: f32, hz: f32, db: f32) -> ([f64; 3], [f64; 3]) {
    let a_amp = 10f64.powf(db as f64 / 40.0);
    let w0 = 2.0 * PI * (hz as f64 / sr as f64).clamp(1e-6, 0.49);
    let (cw, sw) = (w0.cos(), w0.sin());
    // S = 1: alpha = sin(w0)/2 * sqrt(2), independent of the gain.
    let alpha = sw / 2.0 * std::f64::consts::SQRT_2;
    let ap1 = a_amp + 1.0;
    let am1 = a_amp - 1.0;
    let tsa = 2.0 * a_amp.sqrt() * alpha;
    let b = [
        a_amp * (ap1 - am1 * cw + tsa),
        2.0 * a_amp * (am1 - ap1 * cw),
        a_amp * (ap1 - am1 * cw - tsa),
    ];
    let a = [
        ap1 + am1 * cw + tsa,
        -2.0 * (am1 + ap1 * cw),
        ap1 + am1 * cw - tsa,
    ];
    (b, a)
}

/// A peaking bell at `hz` with gain `db` and quality `q`. Passing `-db`
/// gives the exact algebraic inverse.
fn peaking(sr: f32, hz: f32, db: f32, q: f32) -> ([f64; 3], [f64; 3]) {
    let a_amp = 10f64.powf(db as f64 / 40.0);
    let w0 = 2.0 * PI * (hz as f64 / sr as f64).clamp(1e-6, 0.49);
    let (cw, sw) = (w0.cos(), w0.sin());
    let alpha = sw / (2.0 * (q as f64).max(1e-3));
    let b = [1.0 + alpha * a_amp, -2.0 * cw, 1.0 - alpha * a_amp];
    let a = [1.0 + alpha / a_amp, -2.0 * cw, 1.0 - alpha / a_amp];
    (b, a)
}

/// What the Colour section is set to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub on: bool,
    /// Low-shelf gain, dB. Ableton call it Base, then Amt Lo.
    pub base_db: f32,
    /// Bell frequency, Hz.
    pub freq_hz: f32,
    /// Bell quality. Ableton's Width is a unit-free 0..1 whose meaning they
    /// have never published; ours is a Q and says so.
    pub q: f32,
    /// Bell gain, dB. Ableton call it Depth, then Amt Hi.
    pub depth_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            on: true,
            base_db: 0.0,
            freq_hz: 1000.0,
            q: 0.7,
            depth_db: 0.0,
        }
    }
}

/// The forward pair and the inverse pair, kept in step.
#[derive(Clone, Debug)]
pub struct Color {
    shelf: Biquad,
    bell: Biquad,
    shelf_inv: Biquad,
    bell_inv: Biquad,
    sr: f32,
    settings: Settings,
}

impl Color {
    pub fn new(sr: f32) -> Self {
        let mut c = Color {
            shelf: Biquad::unity(),
            bell: Biquad::unity(),
            shelf_inv: Biquad::unity(),
            bell_inv: Biquad::unity(),
            sr,
            settings: Settings::default(),
        };
        c.design();
        c
    }

    /// The rate the filters run at, which is the oversampled rate.
    pub fn set_sample_rate(&mut self, sr: f32) {
        if (sr - self.sr).abs() > 1e-3 {
            self.sr = sr;
            self.design();
            self.reset();
        }
    }

    pub fn configure(&mut self, s: Settings) {
        if s != self.settings {
            self.settings = s;
            self.design();
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings
    }

    fn design(&mut self) {
        let s = self.settings;
        let hz = s.freq_hz.clamp(FREQ_MIN_HZ, FREQ_MAX_HZ);
        let (b, a) = low_shelf(self.sr, SHELF_HZ, s.base_db);
        self.shelf.set(b, a);
        let (b, a) = low_shelf(self.sr, SHELF_HZ, -s.base_db);
        self.shelf_inv.set(b, a);
        let (b, a) = peaking(self.sr, hz, s.depth_db, s.q);
        self.bell.set(b, a);
        let (b, a) = peaking(self.sr, hz, -s.depth_db, s.q);
        self.bell_inv.set(b, a);
    }

    pub fn reset(&mut self) {
        self.shelf.reset();
        self.bell.reset();
        self.shelf_inv.reset();
        self.bell_inv.reset();
    }

    /// The pre-shaper half.
    #[inline]
    pub fn forward(&mut self, x: f32) -> f32 {
        if !self.settings.on {
            return x;
        }
        self.bell.process(self.shelf.process(x))
    }

    /// The post-shaper half, run in the opposite order for symmetry.
    #[inline]
    pub fn inverse(&mut self, x: f32) -> f32 {
        if !self.settings.on {
            return x;
        }
        self.shelf_inv.process(self.bell_inv.process(x))
    }

    /// The forward curve's magnitude at `hz`, in dB. This is the curve the
    /// page draws: the emphasis the shaper actually sees.
    pub fn forward_db(&self, hz: f32) -> f32 {
        if !self.settings.on {
            return 0.0;
        }
        let m = self.shelf.magnitude(hz, self.sr) * self.bell.magnitude(hz, self.sr);
        20.0 * m.max(1e-9).log10()
    }
}
