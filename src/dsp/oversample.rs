//! Oversampling: a cascade of half-band stages at two, four, eight or
//! sixteen times, and the matched delay that keeps the dry path aligned
//! with it.
//!
//! ## Why oversample at all, when the antialiasing is already exact
//!
//! Not for the aliasing, or not mainly. First-order antiderivative
//! antialiasing is a low-pass as well as an antialiaser — its small-signal
//! response is `|cos(ω/2)|` — and at the base rate that costs **16.7 dB at
//! 20 kHz**. That is not a subtlety, it is a tone control. Two times brings
//! it to 2.4 dB, four times to 0.56 dB, eight times to 0.14 dB. A device
//! that must hold a flat response to 20 kHz is therefore forced to at least
//! four times by the response alone, whatever the aliasing needs, and that
//! is why the lowest factor offered here is two rather than one.
//!
//! What the factor then buys on top is compounding rather than fixed:
//! measured, adding first-order antialiasing to a fixed factor is worth
//! +16.5 dB at one time, +21.9 at two, +27.8 at four and +32.0 at eight.
//! The two techniques multiply.
//!
//! ## Why 129 taps
//!
//! Two constraints, and they nearly conflict.
//!
//! **The round trip must be a whole number of base-rate samples.** A stage's
//! filter delays `(N−1)/2` samples at its own rate, so a stage running at
//! `2^k` times the base rate costs `(N−1)/2^k` base samples for the round
//! trip. With `N − 1 = 128`, a multiple of 16, every cascade up to sixteen
//! times lands on a whole number. If it did not, no integer delay could
//! match the dry path and no honest latency could be reported to the host —
//! which is the point of the whole exercise. This constraint has been paid
//! for three times in the compressor lab next door, at 63, 61 and 65 taps,
//! each chosen for a different cascade depth.
//!
//! | depth | factor | round trip, base samples | ms at 44.1 kHz |
//! |---|---|---|---|
//! | 1 | 2x | 64 | 1.45 |
//! | 2 | 4x | 96 | 2.18 |
//! | 3 | 8x | 112 | 2.54 |
//! | 4 | 16x | 120 | 2.72 |
//!
//! **The passband has to reach 20 kHz.** A measured sweep of decimator
//! lengths at four times, with the dry path matched and the mix at half,
//! converges at 129 taps: 33 taps give −2.02 dB at 20 kHz, 65 give −0.87,
//! 129 give −0.41, and 257 and 513 give −0.42. Beyond 129 the residual **is**
//! the antialiasing kernel's own droop and a longer filter buys latency for
//! nothing. So 129 is derived rather than guessed, and it is the shortest
//! filter that reaches the floor.
//!
//! ## Why β = 13
//!
//! The window is **Kaiser** rather than Blackman on published evidence: at
//! equal tap count a Blackman-windowed half-band has a transition band
//! roughly twice as wide, which drags its passband droop down to 15 kHz.
//! Same cost, materially different result.
//!
//! The shape parameter is 13 rather than the 9 the compressor lab uses, and
//! that is a finding of this build rather than a preference. At β = 9 the
//! stopband is 97 dB, and the first benchmark run showed the wavefolder
//! pinned near −105 dB by a product at 901.7 Hz that did not improve with
//! the oversampling factor. It is not a folded harmonic: it is
//! **intermodulation between the fundamental and the interpolator's own
//! residual image**. A 15 kHz tone leaves an image at 29.1 kHz whatever the
//! factor, the stopband decides how loud that image is, and the shape then
//! beats the two together at `2 × 15000.6 − 29099.4 = 901.7 Hz`. Raising the
//! stopband is the whole fix.
//!
//! What makes it nearly free is the half-band symmetry `H(ω) + H(π−ω) = 1`,
//! which ties the passband ripple to the stopband floor: a deeper stopband
//! is a flatter passband, and the only cost of a wider transition is where
//! it reaches. Evaluated at 129 taps, 20 kHz through the whole round trip:
//!
//! | β | stopband | image at 29.1 kHz | 20 kHz, 4x round trip |
//! |---|---|---|---|
//! | 9 | −97.5 dB | −102.6 dB | −0.001 dB |
//! | 11 | −114.2 dB | −128.1 dB | −0.013 dB |
//! | **13** | **−129.4 dB** | **−139.7 dB** | **−0.047 dB** |
//! | 15 | −147.0 dB | −150.1 dB | −0.098 dB |
//!
//! Thirty-two decibels of stopband for five hundredths of a decibel at
//! 20 kHz, against an antialiasing kernel that costs 0.56 dB there anyway.
//! Measured through the engine, that moved the wavefolder from −81.4 dB to
//! −113.2 dB at four times and cost nothing anywhere else.
//!
//! ## Why the filters are written in polyphase form
//!
//! Because sixteen times has to be affordable, and on the hardest curves the
//! target is only met there. In a half-band filter every tap at an even
//! offset from the centre is zero, which means three quarters of the
//! arithmetic in the obvious implementation is spent either on samples that
//! are zero or on outputs that are thrown away:
//!
//! * The interpolator zero-stuffs and filters. Of its two outputs per input
//!   sample, one is reached only through even taps, so it reduces to the
//!   **centre tap times a delayed input sample** — one multiply instead of
//!   129. The other is reached only through odd taps, so it is a dot product
//!   over the input history rather than over the stuffed sequence.
//! * The decimator filters and then throws away every other output. Only the
//!   surviving one is computed, and it splits into the same two branches.
//!
//! Both rearrangements are exact rather than approximate. Written this way a
//! stage costs 65 multiplies per output instead of 258, and the dot product
//! runs over a **contiguous** slice so the compiler can vectorise it, which
//! a wrapping ring index prevents. Measured end to end, sixteen times went
//! from 17.7 % of a core to 4.5 % for a stereo instance at 48 kHz.
//!
//! One property is load-bearing and `tests.rs` asserts it: a windowed sinc is
//! **symmetric**, `h[k] = h[N−1−k]`, which is what lets the dot products run
//! forwards over a history stored oldest first without reversing anything.

/// Filter length. Odd, and `TAPS − 1` a multiple of 16, so that a cascade
/// four deep still delays a whole number of base-rate samples.
pub const TAPS: usize = 129;
/// Taps in one polyphase branch: the odd-offset coefficients.
pub const HALF: usize = (TAPS - 1) / 2;
/// The centre tap's index.
const CENTRE: usize = HALF;
/// Where the centre branch reads in a branch history of [`HALF`] samples.
/// The centre tap sits `HALF` samples back at the stage's fast rate, which
/// is `HALF/2` samples of a branch, and the history runs oldest first.
const CENTRE_READ: usize = HALF - 1 - HALF / 2;
/// Deepest cascade, i.e. sixteen times.
pub const MAX_DEPTH: usize = 4;
/// Largest factor, for stack buffers.
pub const MAX_FACTOR: usize = 1 << MAX_DEPTH;
/// The longest round trip any factor produces, in base-rate samples.
pub const MAX_LATENCY: usize = TAPS - 1 - ((TAPS - 1) >> MAX_DEPTH);

/// The oversampling factors the plug-in offers, in parameter order.
pub const FACTOR_NAMES: [&str; MAX_DEPTH] = ["2x", "4x", "8x", "16x"];

/// Kaiser window shape. See the module documentation: 13 puts the stopband
/// near 129 dB, which is what keeps the interpolator's residual image from
/// intermodulating with the signal inside the shape, and it costs five
/// hundredths of a decibel at 20 kHz to do it.
const KAISER_BETA: f32 = 13.0;

/// Cascade depth for a parameter index, i.e. 2x, 4x, 8x, 16x.
pub fn depth_for_index(i: usize) -> usize {
    (i + 1).clamp(1, MAX_DEPTH)
}

/// Round-trip latency in base-rate samples for a cascade `depth` deep.
pub fn latency_for_depth(depth: usize) -> usize {
    (1..=depth.clamp(1, MAX_DEPTH))
        .map(|k| (TAPS - 1) >> k)
        .sum()
}

/// Modified Bessel function of the first kind, order zero, by its series.
/// Only ever called while the coefficients are designed.
fn bessel_i0(x: f32) -> f32 {
    let mut term = 1.0f64;
    let mut sum = 1.0f64;
    let half = x as f64 / 2.0;
    for k in 1..80 {
        term *= (half / k as f64) * (half / k as f64);
        sum += term;
        if term < 1e-16 * sum {
            break;
        }
    }
    sum as f32
}

/// `0.5·sinc((k − c)/2)` under a Kaiser window, normalised to unity
/// direct-current gain.
///
/// The even-offset taps are forced to exact zeros rather than left at the
/// `10⁻⁸` that `sin(kπ)` evaluates to in single precision. That makes the
/// polyphase rearrangement above exact instead of nearly exact, and it
/// improves the filter rather than approximating it: those taps are zero in
/// the design and only the arithmetic disagreed.
pub fn coefficients() -> [f32; TAPS] {
    let mut h = [0.0f32; TAPS];
    let mut sum = 0.0;
    let denom = bessel_i0(KAISER_BETA);
    let c = CENTRE as i32;
    for (k, hk) in h.iter_mut().enumerate() {
        let n = k as i32 - c;
        if n != 0 && n % 2 == 0 {
            continue;
        }
        let nf = n as f32;
        let sinc = if n == 0 {
            1.0
        } else {
            (std::f32::consts::PI * nf / 2.0).sin() / (std::f32::consts::PI * nf / 2.0)
        };
        let t = nf / CENTRE as f32;
        let w = bessel_i0(KAISER_BETA * (1.0 - t * t).max(0.0).sqrt()) / denom;
        *hk = 0.5 * sinc * w;
        sum += *hk;
    }
    for hk in h.iter_mut() {
        *hk /= sum;
    }
    h
}

/// The two polyphase branches of the half-band, in the form the hot loops
/// want them.
#[derive(Clone, Copy)]
struct Kernel {
    /// `h[2j + 1]`. By the filter's symmetry this array is its own reversal,
    /// which is what lets the dot product run forwards over a history that
    /// is stored oldest first.
    odd: [f32; HALF],
    centre: f32,
}

impl Kernel {
    fn new() -> Self {
        let h = coefficients();
        let mut odd = [0.0f32; HALF];
        for (j, o) in odd.iter_mut().enumerate() {
            *o = h[2 * j + 1];
        }
        Kernel {
            odd,
            centre: h[CENTRE],
        }
    }

    /// `Σ h[2j+1]·x[m−j]` over a history running oldest first.
    #[inline]
    fn dot(&self, w: &[f32]) -> f32 {
        let mut acc = 0.0f32;
        for j in 0..HALF {
            acc += self.odd[j] * w[j];
        }
        acc
    }
}

/// One branch's history, doubled so the window is always contiguous.
#[derive(Clone)]
struct Line {
    buf: [f32; 2 * HALF],
    /// Where the next sample goes, in `0..HALF`.
    pos: usize,
}

impl Line {
    fn new() -> Self {
        Line {
            buf: [0.0; 2 * HALF],
            pos: 0,
        }
    }

    fn reset(&mut self) {
        self.buf = [0.0; 2 * HALF];
        self.pos = 0;
    }

    /// Store `x` and hand back the window ending on it, oldest first.
    #[inline]
    fn push(&mut self, x: f32) -> &[f32] {
        self.buf[self.pos] = x;
        self.buf[self.pos + HALF] = x;
        let start = self.pos + 1;
        self.pos = if self.pos + 1 == HALF {
            0
        } else {
            self.pos + 1
        };
        &self.buf[start..start + HALF]
    }

    /// The window without storing anything, oldest first.
    #[inline]
    fn window(&self) -> &[f32] {
        &self.buf[self.pos..self.pos + HALF]
    }
}

/// One interpolating stage: one sample in, two out.
#[derive(Clone)]
struct Up {
    k: Kernel,
    line: Line,
}

impl Up {
    fn new() -> Self {
        Up {
            k: Kernel::new(),
            line: Line::new(),
        }
    }

    fn reset(&mut self) {
        self.line.reset();
    }

    /// The gain of two is the interpolation gain: zero-stuffing halves the
    /// signal and the filter has unity direct-current gain.
    #[inline]
    fn pair(&mut self, x: f32) -> (f32, f32) {
        let w = self.line.push(x);
        let even = 2.0 * self.k.centre * w[CENTRE_READ];
        let odd = 2.0 * self.k.dot(w);
        (even, odd)
    }
}

/// One decimating stage: two samples in, one out.
#[derive(Clone)]
struct Down {
    k: Kernel,
    /// The even-indexed inputs, which reach the output through the centre
    /// tap alone.
    even: Line,
    /// The odd-indexed inputs, which reach it through the odd taps.
    odd: Line,
}

impl Down {
    fn new() -> Self {
        Down {
            k: Kernel::new(),
            even: Line::new(),
            odd: Line::new(),
        }
    }

    fn reset(&mut self) {
        self.even.reset();
        self.odd.reset();
    }

    /// The odd branch is pushed **after** the output is taken, because the
    /// output at `m` reads the odd sample before it rather than the one
    /// beside it.
    #[inline]
    fn pair(&mut self, v0: f32, v1: f32) -> f32 {
        let centre = self.k.centre * self.even.push(v0)[CENTRE_READ];
        let y = centre + self.k.dot(self.odd.window());
        self.odd.push(v1);
        y
    }
}

/// A cascade of half-band stages, up on one side and down on the other.
#[derive(Clone)]
pub struct Resampler {
    up: Vec<Up>,
    down: Vec<Down>,
    depth: usize,
}

impl Resampler {
    /// A resampler `depth` stages deep, i.e. `2^depth` times oversampled.
    pub fn new(depth: usize) -> Self {
        let depth = depth.clamp(1, MAX_DEPTH);
        Resampler {
            up: (0..MAX_DEPTH).map(|_| Up::new()).collect(),
            down: (0..MAX_DEPTH).map(|_| Down::new()).collect(),
            depth,
        }
    }

    /// Change the factor. Every filter is cleared, because a cascade of a
    /// different depth is a different filter and its history means nothing.
    pub fn set_depth(&mut self, depth: usize) {
        let depth = depth.clamp(1, MAX_DEPTH);
        if depth != self.depth {
            self.depth = depth;
            self.reset();
        }
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn factor(&self) -> usize {
        1 << self.depth
    }

    /// Round-trip latency in base-rate samples, which the tap count is
    /// chosen to keep whole.
    pub fn latency(&self) -> usize {
        latency_for_depth(self.depth)
    }

    pub fn reset(&mut self) {
        for f in self.up.iter_mut() {
            f.reset();
        }
        for f in self.down.iter_mut() {
            f.reset();
        }
    }

    /// One base-rate sample in, `factor()` oversampled samples into `out`.
    ///
    /// Each stage reads from a copy, because the filters are stateful and
    /// have to be fed **forward in time**. Expanding in place and walking
    /// backwards to avoid overwriting the source pushes the samples through
    /// in reverse order, which is not the same filter at all: it cost the
    /// compressor lab about 2 dB at 15 kHz before it was measured.
    pub fn up(&mut self, x: f32, out: &mut [f32; MAX_FACTOR]) {
        let mut src = [0.0f32; MAX_FACTOR];
        src[0] = x;
        out[0] = x;
        let mut n = 1;
        for stage in 0..self.depth {
            for i in 0..n {
                let (a, b) = self.up[stage].pair(src[i]);
                out[2 * i] = a;
                out[2 * i + 1] = b;
            }
            n *= 2;
            src[..n].copy_from_slice(&out[..n]);
        }
    }

    /// `factor()` oversampled samples in, one base-rate sample out.
    pub fn down(&mut self, xs: &[f32; MAX_FACTOR]) -> f32 {
        let mut buf = *xs;
        let mut n = self.factor();
        for stage in (0..self.depth).rev() {
            for i in 0..n / 2 {
                buf[i] = self.down[stage].pair(buf[2 * i], buf[2 * i + 1]);
            }
            n /= 2;
        }
        buf[0]
    }
}

/// A whole-sample delay for the dry path, as long as the resampler's round
/// trip.
///
/// This is the component that makes the dry/wet control honest, and against
/// what it removes it is the cheapest thing in the design: one store, one
/// load and a wrapping index, with no arithmetic in the signal path at all,
/// so it cannot colour the dry signal and 0 % wet is bit-exact bypass.
///
/// Measured on the same 4x cascade this device uses, mixing an undelayed dry
/// path with the wet one produces **18.17 dB of comb** across the audio
/// band; matching the delay leaves 0.87 dB, all of it the wet path's own
/// droop rather than cancellation. The elegant-looking alternative — split
/// the dry and wet branches *inside* the oversampled region so both traverse
/// identical filters — measures no better and is worse where it matters,
/// because a dry signal that has been through a resampler round trip is no
/// longer bypass at 0 % wet.
#[derive(Clone)]
pub struct DryDelay {
    buf: Vec<f32>,
    pos: usize,
    len: usize,
}

impl DryDelay {
    /// A delay of `len` samples, which may be zero.
    pub fn new(len: usize) -> Self {
        let len = len.min(MAX_LATENCY);
        DryDelay {
            buf: vec![0.0; MAX_LATENCY + 1],
            pos: 0,
            len,
        }
    }

    /// Change the delay. The buffer is cleared, because samples held at one
    /// length come out at the wrong time at another.
    pub fn set_len(&mut self, len: usize) {
        let len = len.min(MAX_LATENCY);
        if len != self.len {
            self.len = len;
            self.reset();
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn reset(&mut self) {
        self.buf.fill(0.0);
        self.pos = 0;
    }

    /// Push `x` and return what went in `len` samples ago.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if self.len == 0 {
            return x;
        }
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos > self.len {
            self.pos = 0;
        }
        self.buf[self.pos]
    }
}

impl Default for DryDelay {
    fn default() -> Self {
        DryDelay::new(0)
    }
}
