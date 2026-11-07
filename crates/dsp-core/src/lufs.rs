//! ITU-R BS.1770-4 loudness measurement (K-weighting + gated integration).

use crate::biquad::BiquadCoeffs;

#[derive(Clone, Copy, Debug)]
pub struct KWeighting {
    shelf: BiquadCoeffs,
    highpass: BiquadCoeffs,
    s1: [f32; 2],
    s2: [f32; 2],
    h1: [f32; 2],
    h2: [f32; 2],
}

impl KWeighting {
    pub fn new(sample_rate: f32) -> Self {
        let fs = sample_rate.max(8000.0);
        // Stage 1: high shelf (+4 dB at 1681.97 Hz).
        let f0 = 1_681.974_450_955_533_f32;
        let g = 3.999_843_853_973_347_f32;
        let q = 0.707_175_236_955_419_6_f32;
        let k = (std::f32::consts::PI * f0 / fs).tan();
        let vh = 10f32.powf(g / 20.0);
        let vb = vh.powf(0.499_666_774_154_541_6);
        let a0 = 1.0 + k / q + k * k;
        let shelf = BiquadCoeffs {
            b0: (vh + vb * k / q + k * k) / a0,
            b1: 2.0 * (k * k - vh) / a0,
            b2: (vh - vb * k / q + k * k) / a0,
            a1: 2.0 * (k * k - 1.0) / a0,
            a2: (1.0 - k / q + k * k) / a0,
        };
        // Stage 2: high pass at 38.14 Hz.
        let f0 = 38.135_470_876_024_44_f32;
        let q = 0.500_327_037_323_877_3_f32;
        let k = (std::f32::consts::PI * f0 / fs).tan();
        let a0 = 1.0 + k / q + k * k;
        let highpass = BiquadCoeffs {
            b0: 1.0 / a0,
            b1: -2.0 / a0,
            b2: 1.0 / a0,
            a1: 2.0 * (k * k - 1.0) / a0,
            a2: (1.0 - k / q + k * k) / a0,
        };
        Self {
            shelf,
            highpass,
            s1: [0.0; 2],
            s2: [0.0; 2],
            h1: [0.0; 2],
            h2: [0.0; 2],
        }
    }

    #[inline]
    pub fn process(&mut self, ch: usize, x: f32) -> f32 {
        let ch = ch & 1;
        let c = &self.shelf;
        let y = c.b0 * x + self.s1[ch];
        self.s1[ch] = c.b1 * x - c.a1 * y + self.s2[ch];
        self.s2[ch] = c.b2 * x - c.a2 * y;

        let c = &self.highpass;
        let z = c.b0 * y + self.h1[ch];
        self.h1[ch] = c.b1 * y - c.a1 * z + self.h2[ch];
        self.h2[ch] = c.b2 * y - c.a2 * z;
        z
    }

    pub fn reset(&mut self) {
        self.s1 = [0.0; 2];
        self.s2 = [0.0; 2];
        self.h1 = [0.0; 2];
        self.h2 = [0.0; 2];
    }
}

/// Momentary (400 ms) and short-term (3 s) loudness plus a gated integrated
/// value. Energies are accumulated in 100 ms sub-blocks.
pub struct LoudnessMeter {
    k: KWeighting,
    sample_rate: f32,
    sub_len: usize,
    sub_pos: usize,
    sub_energy: f64,
    momentary: [f64; 4],
    momentary_pos: usize,
    short: [f64; 30],
    short_pos: usize,
    short_count: usize,
    integrated_blocks: Vec<f64>,
    integrated_sum: f64,
    integrated_count: usize,
}

impl LoudnessMeter {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            k: KWeighting::new(sr),
            sample_rate: sr,
            sub_len: (sr * 0.1) as usize,
            sub_pos: 0,
            sub_energy: 0.0,
            momentary: [0.0; 4],
            momentary_pos: 0,
            short: [0.0; 30],
            short_pos: 0,
            short_count: 0,
            integrated_blocks: Vec::with_capacity(4096),
            integrated_sum: 0.0,
            integrated_count: 0,
        }
    }

    pub fn reset(&mut self) {
        self.k.reset();
        self.sub_pos = 0;
        self.sub_energy = 0.0;
        self.momentary = [0.0; 4];
        self.momentary_pos = 0;
        self.short = [0.0; 30];
        self.short_pos = 0;
        self.short_count = 0;
        self.integrated_blocks.clear();
        self.integrated_sum = 0.0;
        self.integrated_count = 0;
    }

    #[inline]
    pub fn process(&mut self, ch: usize, x: f32) {
        let y = self.k.process(ch, x);
        self.sub_energy += (y as f64) * (y as f64);
        self.sub_pos += 1;
        if self.sub_pos >= self.sub_len {
            let e = self.sub_energy / self.sub_len as f64;
            self.sub_energy = 0.0;
            self.sub_pos = 0;
            self.momentary[self.momentary_pos] = e;
            self.momentary_pos = (self.momentary_pos + 1) % 4;
            self.short[self.short_pos] = e;
            self.short_pos = (self.short_pos + 1) % 30;
            self.short_count = (self.short_count + 1).min(30);
            // Integrated: 400 ms blocks with a -70 LUFS absolute gate.
            if self.momentary_pos == 0 {
                let block: f64 = self.momentary.iter().sum::<f64>() / 4.0;
                let lufs = -0.691 + 10.0 * (block.max(1e-12)).log10();
                if lufs > -70.0 {
                    self.integrated_blocks.push(block);
                    self.integrated_sum += block;
                    self.integrated_count += 1;
                    if self.integrated_blocks.len() > 36_000 {
                        let old = self.integrated_blocks.remove(0);
                        self.integrated_sum -= old;
                        self.integrated_count -= 1;
                    }
                }
            }
        }
    }

    pub fn momentary_lufs(&self) -> f32 {
        let e: f64 = self.momentary.iter().sum::<f64>() / 4.0;
        (-0.691 + 10.0 * (e.max(1e-12)).log10()) as f32
    }

    pub fn short_term_lufs(&self) -> f32 {
        if self.short_count == 0 {
            return -120.0;
        }
        let e: f64 =
            self.short.iter().take(self.short_count).sum::<f64>() / self.short_count as f64;
        (-0.691 + 10.0 * (e.max(1e-12)).log10()) as f32
    }

    pub fn integrated_lufs(&self) -> f32 {
        if self.integrated_count == 0 {
            return -120.0;
        }
        let e = self.integrated_sum / self.integrated_count as f64;
        (-0.691 + 10.0 * (e.max(1e-12)).log10()) as f32
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_very_quiet() {
        let mut m = LoudnessMeter::new(48_000.0);
        for _ in 0..48_000 {
            m.process(0, 0.0);
            m.process(1, 0.0);
        }
        assert!(m.momentary_lufs() < -100.0);
    }

    #[test]
    fn sine_1k_minus_20_dbfs_is_about_minus_23_lufs() {
        let mut m = LoudnessMeter::new(48_000.0);
        let amp = 10f32.powf(-20.0 / 20.0);
        for i in 0..48_000 * 2 {
            let x = amp * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48_000.0).sin();
            m.process(0, x);
            m.process(1, x);
        }
        let l = m.momentary_lufs();
        assert!((l + 23.0).abs() < 1.5, "got {l}");
    }
}
