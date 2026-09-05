//! The aliasing readout: how much of what is coming out is not a harmonic
//! of what went in.
//!
//! With a sine at the input, **everything that is not at a harmonic of that
//! sine is aliasing**. Showing that number live is the cheapest possible
//! demonstration of the whole argument this plug-in is built on: turn the
//! drive up and watch it stay where a naive waveshaper's would climb. No
//! version of the device this one answers has ever shown it, although the
//! latest one has a spectrum analyser inside it and its manual admits the
//! defect in writing.
//!
//! ## Method
//!
//! Four thousand and ninety-six samples of input and of output are
//! accumulated, windowed with a four-term Blackman-Harris window (sidelobes
//! at −92 dB, which is what sets the floor), and transformed. The
//! fundamental is the input's peak bin, refined by fitting a parabola to it
//! and its neighbours. Then every bin within [`MASK_BINS`] of a harmonic of
//! that fundamental is masked out, direct current is masked out, and what is
//! left is summed as aliasing and reported against the fundamental's own
//! amplitude.
//!
//! ## What it is and is not
//!
//! It is a **meter**, not the benchmark. The benchmark in `src/bin` snaps
//! its test tones to exact transform bins so that the window contributes no
//! leakage at all, trims the resampler's ringing from both ends, and sweeps
//! the band in fine steps looking for a single loud product — none of which
//! a live meter can do, because the input is whatever the user is playing.
//! The meter's own floor with a clean tone through a linear path is a
//! measured figure and it appears in `docs/BENCHMARK.md`; a reading at that
//! floor means "nothing found", not "nothing there".
//!
//! It is also only meaningful for a periodic input, so it publishes a
//! confidence alongside the number: the share of the input's energy that
//! sits in the peak's own mainlobe. A sine gives nearly one. A mix gives
//! nearly nothing, and the reading beside it should be disregarded rather
//! than believed.
//!
//! Real-time: the transform runs on pre-planned state with pre-allocated
//! scratch, so nothing here allocates, locks or blocks. It fires once per
//! window, which at 48 kHz is about twelve times a second.

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

/// Transform length. Long enough that the bin spacing (11.7 Hz at 48 kHz)
/// resolves a low fundamental's harmonics, short enough to update visibly.
pub const WINDOW: usize = 4096;

/// Half-width of the mask placed over the fundamental and each harmonic, in
/// bins. The window's mainlobe is four bins wide, so three either side
/// covers it with a margin.
pub const MASK_BINS: usize = 3;

/// Below this input level the readout is meaningless and says so.
const SILENCE: f32 = 1e-5;

/// The floor the readout reports when it has found nothing.
pub const FLOOR_DB: f32 = -140.0;

/// One frame of the readout, in the order the stream publishes it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Reading {
    /// Non-harmonic energy against the fundamental, in dB.
    pub alias_db: f32,
    /// The fundamental the mask was built around, in Hz.
    pub f0_hz: f32,
    /// How periodic the input is, 0 to 1. Below about a half the number
    /// beside it means nothing.
    pub confidence: f32,
    /// How many harmonics of the fundamental fell below Nyquist and were
    /// masked.
    pub harmonics: f32,
}

impl Reading {
    pub fn to_frame(self) -> [f32; 4] {
        [self.alias_db, self.f0_hz, self.confidence, self.harmonics]
    }
}

/// Accumulates a window of input and output and measures the one against
/// the other.
pub struct AliasMeter {
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex<f32>>,
    window: Vec<f32>,
    inp: Vec<f32>,
    out: Vec<f32>,
    spec_in: Vec<Complex<f32>>,
    spec_out: Vec<Complex<f32>>,
    /// Which bins the harmonic mask covers. Held here rather than built per
    /// measurement, because this runs in the audio thread and nothing in the
    /// audio thread allocates.
    masked: Vec<bool>,
    fill: usize,
    sr: f32,
    reading: Reading,
}

impl AliasMeter {
    pub fn new(sr: f32) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(WINDOW);
        let scratch = vec![Complex::new(0.0, 0.0); fft.get_inplace_scratch_len()];
        // Four-term Blackman-Harris. The sidelobe level is what the readout
        // can see past, so it is the meter's floor and not a detail.
        let mut window = vec![0.0f32; WINDOW];
        let n = WINDOW as f32;
        for (i, w) in window.iter_mut().enumerate() {
            let t = 2.0 * std::f32::consts::PI * i as f32 / n;
            *w =
                0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos();
        }
        AliasMeter {
            fft,
            scratch,
            window,
            inp: vec![0.0; WINDOW],
            out: vec![0.0; WINDOW],
            spec_in: vec![Complex::new(0.0, 0.0); WINDOW],
            spec_out: vec![Complex::new(0.0, 0.0); WINDOW],
            masked: vec![false; WINDOW / 2],
            fill: 0,
            sr,
            reading: Reading {
                alias_db: FLOOR_DB,
                ..Default::default()
            },
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.fill = 0;
        self.reading = Reading {
            alias_db: FLOOR_DB,
            ..Default::default()
        };
    }

    /// The most recent measurement. Held between windows, so the page sees a
    /// steady number rather than a flicker.
    pub fn reading(&self) -> Reading {
        self.reading
    }

    /// Feed one block. When a window fills, it is measured and the reading
    /// is replaced.
    pub fn push(&mut self, input: &[f32], output: &[f32]) {
        let n = input.len().min(output.len());
        let mut i = 0;
        while i < n {
            let room = WINDOW - self.fill;
            let take = room.min(n - i);
            self.inp[self.fill..self.fill + take].copy_from_slice(&input[i..i + take]);
            self.out[self.fill..self.fill + take].copy_from_slice(&output[i..i + take]);
            self.fill += take;
            i += take;
            if self.fill == WINDOW {
                self.measure();
                self.fill = 0;
            }
        }
    }

    fn measure(&mut self) {
        for k in 0..WINDOW {
            self.spec_in[k] = Complex::new(self.inp[k] * self.window[k], 0.0);
            self.spec_out[k] = Complex::new(self.out[k] * self.window[k], 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.spec_in, &mut self.scratch);
        self.fft
            .process_with_scratch(&mut self.spec_out, &mut self.scratch);

        let half = WINDOW / 2;
        let bin_hz = self.sr / WINDOW as f32;

        // The fundamental: the input's loudest bin above direct current,
        // refined by a parabola through it and its neighbours.
        let mut peak = 0usize;
        let mut peak_p = 0.0f32;
        let mut total_in = 0.0f64;
        for k in 1..half {
            let p = self.spec_in[k].norm_sqr();
            total_in += p as f64;
            if p > peak_p {
                peak_p = p;
                peak = k;
            }
        }
        let input_peak = self.inp.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        if peak == 0 || input_peak < SILENCE {
            self.reading = Reading {
                alias_db: FLOOR_DB,
                ..Default::default()
            };
            return;
        }
        let refined = {
            let lo = self.spec_in[peak - 1].norm().max(1e-30);
            let mid = self.spec_in[peak].norm().max(1e-30);
            let hi = self.spec_in[(peak + 1).min(half - 1)].norm().max(1e-30);
            // Parabolic interpolation on the log magnitudes, the standard
            // refinement for a windowed peak.
            let (a, b, c) = (lo.ln(), mid.ln(), hi.ln());
            let denom = a - 2.0 * b + c;
            let delta = if denom.abs() < 1e-12 {
                0.0
            } else {
                (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
            };
            peak as f32 + delta
        };
        let f0 = refined * bin_hz;

        // Confidence: the share of the input's energy inside the peak's own
        // mainlobe. A tone puts nearly all of it there; anything else does
        // not, and the reading beside it should be disregarded.
        let mut peak_energy = 0.0f64;
        for k in peak.saturating_sub(MASK_BINS)..=(peak + MASK_BINS).min(half - 1) {
            peak_energy += self.spec_in[k].norm_sqr() as f64;
        }
        let confidence = if total_in > 0.0 {
            (peak_energy / total_in) as f32
        } else {
            0.0
        };

        // Mask direct current and every harmonic of the fundamental, then
        // sum what is left.
        self.masked.fill(false);
        for m in self.masked.iter_mut().take(MASK_BINS + 1) {
            *m = true;
        }
        let mut harmonics = 0usize;
        let mut k = 1usize;
        loop {
            let centre = refined * k as f32;
            if centre >= (half - 1) as f32 {
                break;
            }
            let c = centre.round() as usize;
            let lo = c.saturating_sub(MASK_BINS);
            let hi = (c + MASK_BINS).min(half - 1);
            for m in self.masked.iter_mut().take(hi + 1).skip(lo) {
                *m = true;
            }
            harmonics += 1;
            k += 1;
        }

        let mut fundamental = 0.0f64;
        for j in peak.saturating_sub(MASK_BINS)..=(peak + MASK_BINS).min(half - 1) {
            fundamental += self.spec_out[j].norm_sqr() as f64;
        }
        let mut alias = 0.0f64;
        for (j, m) in self.masked.iter().enumerate() {
            if !*m {
                alias += self.spec_out[j].norm_sqr() as f64;
            }
        }

        let ratio = if fundamental > 0.0 {
            (alias / fundamental).sqrt()
        } else {
            0.0
        };
        let alias_db = if ratio > 0.0 {
            (20.0 * ratio.log10()) as f32
        } else {
            FLOOR_DB
        };
        self.reading = Reading {
            alias_db: alias_db.max(FLOOR_DB),
            f0_hz: f0,
            confidence: confidence.clamp(0.0, 1.0),
            harmonics: harmonics as f32,
        };
    }
}
