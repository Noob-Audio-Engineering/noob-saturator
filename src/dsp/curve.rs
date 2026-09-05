//! The six shapes, and the antiderivative of each one in closed form.
//!
//! These are ours. They are not Ableton's, they are not reverse-engineered
//! from Ableton's, and they are not modelled on a circuit. What they are is
//! a set chosen so that every one of them can be antialiased exactly — each
//! is built from terms that antidifferentiate in elementary functions, so
//! the antialiasing in [`super::adaa`] never needs a table, never needs a
//! rebuild when a knob moves, and never carries a table's error into the
//! quotient it divides.
//!
//! Every curve here is **odd**, and every one has **unit slope at the
//! origin** except `Gate`, whose slope there is zero on purpose. Every one
//! reaches a ceiling of exactly 1 except `Fold`, which folds back through it.
//! The unit slope is what makes the drive control mean the same thing on all
//! six, and it is what lets the device null against its own input when the
//! drive is turned down.
//!
//! ## The set
//!
//! Listed in order of hardness, which is the order they appear in the menu.
//!
//! | name | `f(x)` | `F₀(x)`, with `F₀(0) = 0` |
//! |---|---|---|
//! | **Warm** | `(2/π)·atan(πx/2)` | `(2/π)·x·atan(πx/2) − (2/π²)·ln(1 + π²x²/4)` |
//! | **Round** | `x/√(1+x²)` | `√(1+x²) − 1` |
//! | **Soft** | `tanh x` | `ln cosh x` |
//! | **Clip** | piecewise, below | piecewise, below |
//! | **Fold** | `sin x` | `1 − cos x` |
//! | **Gate** | `2·tanh x − tanh 2x` | `2·ln cosh x − ½·ln cosh 2x` |
//!
//! **Clip** is a quadratic-knee soft clipper with a knee width `w` in
//! `0..1`. Writing `k = 1 − w` for where the knee starts and `a = |x|`:
//!
//! ```text
//!            a ≤ k    f = x                            F₀ = a²/2
//!    k < a < 2 − k    f = sgn(x)·(a − (a−k)²/(4(1−k))) F₀ = a²/2 − (a−k)³/(12(1−k))
//!        a ≥ 2 − k    f = sgn(x)                        F₀ = C + (a − (2−k)),
//!                                                       C = (2−k)²/2 − ⅔(1−k)²
//! ```
//!
//! At `w = 0` the middle branch is empty and this is a hard clipper, which
//! is the hardest thing the device can do and the standard benchmark for
//! aliasing. At `w = 1` the parabola runs from the origin all the way to the
//! ceiling. The knee is continuous in value and in slope at both joints, so
//! the curve has no corner anywhere except at `w = 0`, where the corner is
//! the point.
//!
//! **Gate** is the one shape here that flattens near the origin — our answer
//! to the "like an ultra-fast noise gate" term in Ableton's Waveshaper
//! mode. Near zero it behaves as `2x³`, so the flattening is a **rounded
//! corner and not a discontinuity**, which matters: a discontinuity in the
//! function itself is the one case first-order antialiasing handles worst,
//! and rounding the corner is the standard remedy (`ANTIALIASING.md` §5.3).
//! It is assembled as a difference of two hyperbolic tangents at different
//! scales, `(tanh x − a·tanh(x/a))/(1 − a)` with `a = 1/2`, which is
//! monotone, bounded by 1, and antidifferentiates term by term because
//! antidifferentiation is linear.
//!
//! **Fold** is a wavefolder, and it is here because the field benchmarks
//! heavy aliasing on wavefolders. It is the curve that decides whether this
//! plug-in meets its target; it is also the curve with the cheapest
//! antiderivative on the list.
//!
//! ## Why the antiderivative is what matters
//!
//! First-order antiderivative antialiasing evaluates
//! `(F₀(u) − F₀(u₋₁)) / (u − u₋₁)` — the average of `f` over the segment the
//! signal actually crossed, rather than `f` at one endpoint. So a curve is
//! only as antialiasable as its antiderivative is available, and a curve set
//! chosen for its antiderivatives costs nothing in flexibility at first
//! order. Each closed form below was checked against a numerical
//! differentiation of itself in `tests.rs`, which is the only honest way to
//! check a closed form: differentiating `F₀` must give back `f`, and `f` is
//! written here independently of it.
//!
//! ## Precision
//!
//! Everything is `f64`. That is not caution for its own sake: the
//! antialiasing quotient divides a difference of two nearly equal
//! antiderivatives by a small step, which amplifies whatever error `F₀`
//! carries by roughly `2r·|F₀|/|Δu|`. At the drive this device reaches,
//! `|F₀|` runs to about 60, and the smallest step the quotient is used on is
//! `1/100`, so an `f32` evaluation's few-part-in-ten-million error would
//! surface at about −50 dB — well above the −80 dB the plug-in exists to
//! reach. In `f64` the same arithmetic lands near −230 dB. See
//! [`super::adaa::MIN_STEP`], where the two decisions are made together.

use std::f64::consts::PI;

/// Names of the curves, in menu order. The order is the parameter's value,
/// so it is stable API: appending is safe, reordering is not.
pub const CURVE_NAMES: [&str; 6] = ["Warm", "Round", "Soft", "Clip", "Fold", "Gate"];

/// The `Gate` curve's inner scale. The curve is
/// `(tanh x − a·tanh(x/a))/(1 − a)`; at `a = 1/2` that collapses to
/// `2·tanh x − tanh 2x`, which is why this is not a parameter.
const GATE_A: f64 = 0.5;

/// Which shape the waveshaper is using.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Curve {
    /// `(2/π)·atan(πx/2)`. The softest: it approaches its ceiling as `1/x`,
    /// so it never quite stops growing.
    Warm,
    /// `x/√(1+x²)`. Algebraic, approaches the ceiling as `1/x²`.
    Round,
    /// `tanh x`. Approaches the ceiling exponentially, which is why it is
    /// the default and why it is what most people mean by saturation.
    #[default]
    Soft,
    /// A quadratic-knee soft clipper, hard at knee zero.
    Clip,
    /// `sin x`. A wavefolder.
    Fold,
    /// `2·tanh x − tanh 2x`. Flattens near the origin with a rounded corner.
    Gate,
}

impl Curve {
    /// From the parameter's value; out of range falls back to the default.
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Curve::Warm,
            1 => Curve::Round,
            2 => Curve::Soft,
            3 => Curve::Clip,
            4 => Curve::Fold,
            5 => Curve::Gate,
            _ => Curve::Soft,
        }
    }

    /// The menu index, which is the parameter's value.
    pub fn index(self) -> usize {
        match self {
            Curve::Warm => 0,
            Curve::Round => 1,
            Curve::Soft => 2,
            Curve::Clip => 3,
            Curve::Fold => 4,
            Curve::Gate => 5,
        }
    }

    /// Every curve, in menu order.
    pub const ALL: [Curve; 6] = [
        Curve::Warm,
        Curve::Round,
        Curve::Soft,
        Curve::Clip,
        Curve::Fold,
        Curve::Gate,
    ];

    /// The panel name.
    pub fn name(self) -> &'static str {
        CURVE_NAMES[self.index()]
    }
}

/// A curve together with whatever shapes it. Only `Clip` has anything to
/// shape, which is the point: a parameter that genuinely reshapes a curve
/// would force a table rebuild if the antiderivative were tabulated, and
/// the whole curve set is designed so that none of them is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shape {
    pub curve: Curve,
    /// `Clip`'s knee width, 0 (a hard corner) to 1 (all knee). Ignored by
    /// every other curve.
    pub knee: f64,
}

impl Default for Shape {
    fn default() -> Self {
        Shape {
            curve: Curve::default(),
            knee: 0.5,
        }
    }
}

/// `ln cosh x`, without overflowing.
///
/// The direct form overflows at `|x| > 710`, and this curve is driven to
/// 63 times a full-scale signal before the bias is added, so the direct
/// form is not merely inelegant. `cosh x = (e^|x|/2)(1 + e^−2|x|)`, so
/// `ln cosh x = |x| + ln(1 + e^−2|x|) − ln 2`, which is exact for every
/// argument and loses nothing to cancellation because `ln_1p` is accurate
/// near zero.
#[inline]
fn ln_cosh(x: f64) -> f64 {
    let a = x.abs();
    a + (-2.0 * a).exp().ln_1p() - std::f64::consts::LN_2
}

impl Shape {
    pub fn new(curve: Curve, knee: f64) -> Self {
        Shape {
            curve,
            knee: knee.clamp(0.0, 1.0),
        }
    }

    /// A hard clipper: the `Clip` curve with no knee at all.
    pub fn hard_clip() -> Self {
        Shape {
            curve: Curve::Clip,
            knee: 0.0,
        }
    }

    /// The transfer function itself, `f(x)`.
    #[inline]
    pub fn f(&self, x: f64) -> f64 {
        match self.curve {
            Curve::Warm => (2.0 / PI) * (PI * x / 2.0).atan(),
            Curve::Round => x / (1.0 + x * x).sqrt(),
            Curve::Soft => x.tanh(),
            Curve::Clip => self.clip_f(x),
            Curve::Fold => x.sin(),
            Curve::Gate => (x.tanh() - GATE_A * (x / GATE_A).tanh()) / (1.0 - GATE_A),
        }
    }

    /// The antiderivative `F₀`, with the constant chosen so `F₀(0) = 0`.
    ///
    /// Parker, Zavalishin and Le Bivic recommend that choice for free: it
    /// is the one that minimises the precision lost in the difference the
    /// quotient takes, because it puts zero where the signal spends most of
    /// its time.
    #[inline]
    pub fn f0(&self, x: f64) -> f64 {
        match self.curve {
            Curve::Warm => {
                let c = PI / 2.0;
                (2.0 / PI) * x * (c * x).atan() - (2.0 / (PI * PI)) * (1.0 + c * c * x * x).ln()
            }
            Curve::Round => (1.0 + x * x).sqrt() - 1.0,
            Curve::Soft => ln_cosh(x),
            Curve::Clip => self.clip_f0(x),
            Curve::Fold => 1.0 - x.cos(),
            Curve::Gate => (ln_cosh(x) - GATE_A * GATE_A * ln_cosh(x / GATE_A)) / (1.0 - GATE_A),
        }
    }

    /// Where the `Clip` curve's knee starts. `w = 0` puts it at 1, which is
    /// a hard clipper; `w = 1` puts it at the origin.
    #[inline]
    fn knee_start(&self) -> f64 {
        1.0 - self.knee
    }

    #[inline]
    fn clip_f(&self, x: f64) -> f64 {
        let k = self.knee_start();
        let a = x.abs();
        let s = if x < 0.0 { -1.0 } else { 1.0 };
        if k >= 1.0 - 1e-9 {
            // No knee: a hard clipper, and the branch below would divide by
            // zero rather than degenerate to one.
            return x.clamp(-1.0, 1.0);
        }
        if a <= k {
            x
        } else if a < 2.0 - k {
            let d = a - k;
            s * (a - d * d / (4.0 * (1.0 - k)))
        } else {
            s
        }
    }

    #[inline]
    fn clip_f0(&self, x: f64) -> f64 {
        let k = self.knee_start();
        let a = x.abs();
        if k >= 1.0 - 1e-9 {
            return if a <= 1.0 { 0.5 * a * a } else { a - 0.5 };
        }
        if a <= k {
            0.5 * a * a
        } else if a < 2.0 - k {
            let d = a - k;
            0.5 * a * a - d * d * d / (12.0 * (1.0 - k))
        } else {
            let edge = 2.0 - k;
            let w = 1.0 - k;
            let c = 0.5 * edge * edge - (2.0 / 3.0) * w * w;
            c + (a - edge)
        }
    }
}
