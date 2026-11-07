//! Biquad filters (RBJ cookbook) and Butterworth cascades.

use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BiquadKind {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    AllPass,
    Peak,
    LowShelf,
    HighShelf,
}

impl BiquadKind {
    pub fn parse(s: &str) -> Option<BiquadKind> {
        match s.to_ascii_lowercase().replace([' ', '_'], "-").as_str() {
            "low-pass" | "lowpass" | "lp" => Some(BiquadKind::LowPass),
            "high-pass" | "highpass" | "hp" => Some(BiquadKind::HighPass),
            "band-pass" | "bandpass" | "bp" => Some(BiquadKind::BandPass),
            "notch" | "band-reject" => Some(BiquadKind::Notch),
            "all-pass" | "allpass" | "ap" => Some(BiquadKind::AllPass),
            "bell" | "peak" | "peaking" | "pk" => Some(BiquadKind::Peak),
            "lo-shelf" | "lowshelf" | "low-shelf" | "lsc" => Some(BiquadKind::LowShelf),
            "hi-shelf" | "highshelf" | "high-shelf" | "hsc" => Some(BiquadKind::HighShelf),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        Self::identity()
    }
}

impl BiquadCoeffs {
    pub const fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    pub fn new(kind: BiquadKind, sample_rate: f32, freq: f32, q: f32, gain_db: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let f = freq.clamp(5.0, sr * 0.49);
        let w0 = 2.0 * PI * f / sr;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let q = q.clamp(0.05, 40.0);
        let alpha = sin_w0 / (2.0 * q);
        let a = 10f32.powf(gain_db / 40.0);

        let (b0, b1, b2, a0, a1, a2) = match kind {
            BiquadKind::LowPass => (
                (1.0 - cos_w0) * 0.5,
                1.0 - cos_w0,
                (1.0 - cos_w0) * 0.5,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            BiquadKind::HighPass => (
                (1.0 + cos_w0) * 0.5,
                -(1.0 + cos_w0),
                (1.0 + cos_w0) * 0.5,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            BiquadKind::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha),
            BiquadKind::Notch => (
                1.0,
                -2.0 * cos_w0,
                1.0,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            BiquadKind::AllPass => (
                1.0 - alpha,
                -2.0 * cos_w0,
                1.0 + alpha,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            BiquadKind::Peak => (
                1.0 + alpha * a,
                -2.0 * cos_w0,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos_w0,
                1.0 - alpha / a,
            ),
            BiquadKind::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 - s),
                    (a + 1.0) + (a - 1.0) * cos_w0 + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                    (a + 1.0) + (a - 1.0) * cos_w0 - s,
                )
            }
            BiquadKind::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 - s),
                    (a + 1.0) - (a - 1.0) * cos_w0 + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                    (a + 1.0) - (a - 1.0) * cos_w0 - s,
                )
            }
        };

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Magnitude response at `freq` (linear gain).
    pub fn magnitude(&self, sample_rate: f32, freq: f32) -> f32 {
        let w = 2.0 * PI * freq / sample_rate.max(1.0);
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);
        let num = (num_re * num_re + num_im * num_im).sqrt();
        let den = (den_re * den_re + den_im * den_im).sqrt();
        if den <= f32::EPSILON {
            0.0
        } else {
            num / den
        }
    }

    pub fn magnitude_db(&self, sample_rate: f32, freq: f32) -> f32 {
        20.0 * self.magnitude(sample_rate, freq).max(1e-9).log10()
    }
}

/// A single biquad section with per-channel state (max 2 channels).
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    pub c: BiquadCoeffs,
    z1: [f32; 2],
    z2: [f32; 2],
}

impl Biquad {
    pub fn new() -> Self {
        Self {
            c: BiquadCoeffs::identity(),
            z1: [0.0; 2],
            z2: [0.0; 2],
        }
    }

    pub fn set(&mut self, kind: BiquadKind, sample_rate: f32, freq: f32, q: f32, gain_db: f32) {
        self.c = BiquadCoeffs::new(kind, sample_rate, freq, q, gain_db);
    }

    pub fn set_coeffs(&mut self, c: BiquadCoeffs) {
        self.c = c;
    }

    #[inline]
    pub fn process(&mut self, ch: usize, x: f32) -> f32 {
        let ch = ch & 1;
        let y = self.c.b0 * x + self.z1[ch];
        self.z1[ch] = self.c.b1 * x - self.c.a1 * y + self.z2[ch];
        self.z2[ch] = self.c.b2 * x - self.c.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }
}

/// Butterworth cascade of up to four biquads (orders 2, 4, 6, 8).
#[derive(Clone, Debug)]
pub struct Cascade {
    sections: Vec<Biquad>,
    order: usize,
}

const Q_ORDER_2: [f32; 1] = [0.707_106_77];
const Q_ORDER_4: [f32; 2] = [0.541_196_1, 1.306_563];
const Q_ORDER_6: [f32; 3] = [0.517_638_1, 0.707_106_77, 1.931_851_7];
const Q_ORDER_8: [f32; 4] = [0.509_795_6, 0.601_344_9, 0.899_976_2, 2.562_915_5];

impl Cascade {
    pub fn new(order: usize) -> Self {
        let sections = order / 2;
        Self {
            sections: vec![Biquad::new(); sections.max(1)],
            order,
        }
    }

    pub fn set(&mut self, kind: BiquadKind, sample_rate: f32, freq: f32, gain_db: f32) {
        let qs: &[f32] = match self.order {
            2 => &Q_ORDER_2,
            4 => &Q_ORDER_4,
            6 => &Q_ORDER_6,
            _ => &Q_ORDER_8,
        };
        for (i, section) in self.sections.iter_mut().enumerate() {
            let q = qs.get(i).copied().unwrap_or(0.707);
            section.set(kind, sample_rate, freq, q, gain_db);
        }
    }

    #[inline]
    pub fn process(&mut self, ch: usize, mut x: f32) -> f32 {
        for section in self.sections.iter_mut() {
            x = section.process(ch, x);
        }
        x
    }

    pub fn reset(&mut self) {
        for section in self.sections.iter_mut() {
            section.reset();
        }
    }

    pub fn magnitude(&self, sample_rate: f32, freq: f32) -> f32 {
        self.sections
            .iter()
            .map(|s| s.c.magnitude(sample_rate, freq))
            .product()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_filter_has_expected_gain() {
        let c = BiquadCoeffs::new(BiquadKind::Peak, 48_000.0, 1_000.0, 1.0, 6.0);
        let at_center = c.magnitude_db(48_000.0, 1_000.0);
        assert!((at_center - 6.0).abs() < 0.05, "got {at_center}");
        let far = c.magnitude_db(48_000.0, 20.0);
        assert!(far.abs() < 0.1, "got {far}");
    }

    #[test]
    fn lowpass_is_minus_3db_at_cutoff() {
        let c = BiquadCoeffs::new(BiquadKind::LowPass, 48_000.0, 1_000.0, 0.707_106_77, 0.0);
        let at = c.magnitude_db(48_000.0, 1_000.0);
        assert!((at + 3.01).abs() < 0.1, "got {at}");
    }

    #[test]
    fn cascade_order_4_rolloff() {
        let mut c = Cascade::new(4);
        c.set(BiquadKind::LowPass, 48_000.0, 1_000.0, 0.0);
        let at_2k = 20.0 * c.magnitude(48_000.0, 2_000.0).log10();
        assert!(at_2k < -20.0, "got {at_2k}");
    }
}
