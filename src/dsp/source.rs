//! Demo signals for the standalone. None of this is reachable from the
//! plug-in, which is fed by its host.
//!
//! The set is chosen for the one thing this device exists to demonstrate.
//! A **sine** is what the aliasing readout needs, because with a sine in,
//! everything that is not at a harmonic of it is aliasing. A **two-tone**
//! pair shows intermodulation, which a single tone cannot. A **sweep**
//! makes folded partials visible as descending lines against ascending
//! ones. A **saw** is a signal that already contains everything, so it
//! shows what the device does to programme rather than to a probe. And
//! **noise** is the null case: with no periodic content the readout's
//! confidence collapses, which is the meter telling the truth about itself.

/// Names of the sources, in parameter order.
pub const SOURCE_NAMES: [&str; 5] = ["Sine", "Two Tone", "Sweep", "Saw", "Noise"];

/// A phase accumulator and a noise generator.
pub struct Source {
    phase: f32,
    phase2: f32,
    sweep: f32,
    rng: u32,
}

impl Source {
    pub fn new(seed: u32) -> Self {
        Source {
            phase: 0.0,
            phase2: 0.0,
            sweep: 0.0,
            rng: seed | 1,
        }
    }

    #[inline]
    fn noise(&mut self) -> f32 {
        // Xorshift32: cheap, deterministic, and good enough for a test tone.
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// One sample of source `kind` at `hz`, at unit amplitude.
    pub fn next(&mut self, kind: usize, hz: f32, sr: f32) -> f32 {
        let step = (hz / sr).clamp(0.0, 0.49);
        self.phase += step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let tau = 2.0 * std::f32::consts::PI;
        match kind {
            // Two tones a major third apart, so the intermodulation
            // products land where they can be heard rather than on a
            // harmonic.
            1 => {
                self.phase2 += (step * 1.26).min(0.49);
                if self.phase2 >= 1.0 {
                    self.phase2 -= 1.0;
                }
                0.5 * ((self.phase * tau).sin() + (self.phase2 * tau).sin())
            }
            // Six seconds from the stated frequency to just under Nyquist
            // and back, logarithmically.
            2 => {
                self.sweep += 1.0 / (6.0 * sr);
                if self.sweep >= 1.0 {
                    self.sweep -= 1.0;
                }
                let t = if self.sweep < 0.5 {
                    self.sweep * 2.0
                } else {
                    (1.0 - self.sweep) * 2.0
                };
                let top = (sr * 0.45).max(hz * 1.001);
                let f = hz * (top / hz).powf(t);
                self.phase2 += (f / sr).clamp(0.0, 0.49);
                if self.phase2 >= 1.0 {
                    self.phase2 -= 1.0;
                }
                (self.phase2 * tau).sin()
            }
            // A trivial saw: it aliases on its own, which is the honest way
            // to show that nothing here removes aliasing already in the
            // input.
            3 => 2.0 * self.phase - 1.0,
            4 => self.noise(),
            _ => (self.phase * tau).sin(),
        }
    }
}

impl Default for Source {
    fn default() -> Self {
        Source::new(0x9E37_79B9)
    }
}
