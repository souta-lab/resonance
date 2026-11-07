//! Resampling helpers.

use crate::biquad::{Biquad, BiquadKind};

/// Streaming linear-interpolation resampler.
pub struct LinearResampler {
    pos: f64,
    step: f64,
    last: f32,
    primed: bool,
}

impl LinearResampler {
    pub fn new() -> Self {
        Self {
            pos: 0.0,
            step: 1.0,
            last: 0.0,
            primed: false,
        }
    }

    /// `ratio` = output samples per input sample.
    pub fn set_ratio(&mut self, ratio: f64) {
        self.step = ratio.clamp(0.01, 16.0);
    }

    pub fn reset(&mut self) {
        self.pos = 0.0;
        self.primed = false;
        self.last = 0.0;
    }

    /// Produce `out.len()` samples from `input`. Returns the number of input
    /// samples consumed.
    pub fn process(&mut self, input: &[f32], out: &mut [f32]) -> usize {
        if input.is_empty() || out.is_empty() {
            return 0;
        }
        let mut consumed = 0usize;
        for o in out.iter_mut() {
            while self.pos >= 1.0 {
                self.pos -= 1.0;
                self.last = input[consumed.min(input.len() - 1)];
                consumed += 1;
                if consumed >= input.len() {
                    consumed = input.len() - 1;
                    self.pos = self.pos.min(0.999);
                    break;
                }
            }
            let i = consumed.min(input.len() - 1);
            let next = input[(i + 1).min(input.len() - 1)];
            let t = self.pos as f32;
            *o = self.last + (next - self.last) * t;
            self.pos += self.step;
        }
        consumed
    }
}

impl Default for LinearResampler {
    fn default() -> Self {
        Self::new()
    }
}

/// 2x/4x oversampling wrapper built from linear interpolation and a low-pass
/// decimation filter. Good enough for limiter sidechain / true-peak work; not a
/// mastering-grade oversampler.
pub struct Oversampler {
    factor: usize,
    lp: Biquad,
    phase: usize,
}

impl Oversampler {
    pub fn new(factor: usize, sample_rate: f32) -> Self {
        let factor = factor.clamp(1, 4);
        let mut lp = Biquad::new();
        lp.set(
            BiquadKind::LowPass,
            sample_rate * factor as f32,
            sample_rate * 0.45,
            0.707,
            0.0,
        );
        Self {
            factor,
            lp,
            phase: 0,
        }
    }

    pub fn factor(&self) -> usize {
        self.factor
    }

    /// Upsample `input` into `out` (must be at least `input.len() * factor`).
    pub fn upsample(&mut self, input: &[f32], out: &mut [f32]) -> usize {
        let need = input.len() * self.factor;
        if out.len() < need {
            return 0;
        }
        for i in 0..input.len() {
            let prev = if i == 0 { input[0] } else { input[i - 1] };
            let cur = input[i];
            for k in 0..self.factor {
                let t = k as f32 / self.factor as f32;
                out[i * self.factor + k] = prev + (cur - prev) * t;
            }
        }
        need
    }

    /// Downsample by picking every `factor`-th sample after low-pass filtering.
    pub fn downsample(&mut self, input: &[f32], out: &mut [f32]) -> usize {
        let mut n = 0;
        for (i, &x) in input.iter().enumerate() {
            let y = self.lp.process(0, x);
            if i % self.factor == self.phase {
                if n < out.len() {
                    out[n] = y;
                    n += 1;
                }
            }
        }
        n
    }

    pub fn reset(&mut self) {
        self.lp.reset();
        self.phase = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_preserves_length() {
        let mut r = LinearResampler::new();
        r.set_ratio(1.0);
        let input: Vec<f32> = (0..64).map(|i| i as f32).collect();
        let mut out = vec![0.0; 64];
        r.process(&input, &mut out);
        assert!((out[10] - 10.0).abs() < 1.5);
    }
}
