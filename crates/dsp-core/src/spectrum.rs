//! Spectrum analyzer tap (log-spaced bins, dBFS).

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

use crate::meter::SPECTRUM_BINS;

pub struct SpectrumAnalyzer {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    time: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    ranges: Vec<(usize, usize)>,
    smoothed: [f32; SPECTRUM_BINS],
    pos: usize,
    sample_rate: f32,
    counter: usize,
}

impl SpectrumAnalyzer {
    pub fn new(sample_rate: f32) -> Self {
        let n = 2048;
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        let window: Vec<f32> = (0..n)
            .map(|i| {
                let x = i as f32 / (n - 1) as f32;
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * x).cos()
            })
            .collect();
        let mut ranges = Vec::with_capacity(SPECTRUM_BINS);
        let f_min = 20.0f32;
        let f_max = (sample_rate * 0.5).min(20_000.0);
        for i in 0..SPECTRUM_BINS {
            let lo = f_min * (f_max / f_min).powf(i as f32 / SPECTRUM_BINS as f32);
            let hi = f_min * (f_max / f_min).powf((i + 1) as f32 / SPECTRUM_BINS as f32);
            let b0 = ((lo / sample_rate) * n as f32).floor() as usize;
            let b1 = ((hi / sample_rate) * n as f32).ceil() as usize;
            let b0 = b0.min(n / 2 - 1);
            let b1 = b1.clamp(b0 + 1, n / 2);
            ranges.push((b0, b1));
        }
        Self {
            fft,
            window,
            time: vec![Complex::new(0.0, 0.0); n],
            scratch: vec![Complex::new(0.0, 0.0); n],
            ranges,
            smoothed: [-120.0; SPECTRUM_BINS],
            pos: 0,
            sample_rate,
            counter: 0,
        }
    }

    pub fn reset(&mut self) {
        self.time.fill(Complex::new(0.0, 0.0));
        self.pos = 0;
        self.smoothed = [-120.0; SPECTRUM_BINS];
    }

    /// Feed a mono mix. Returns true when a new frame was produced.
    pub fn push(&mut self, mono: &[f32]) -> bool {
        let n = self.time.len();
        for &x in mono {
            self.time[self.pos] = Complex::new(x, 0.0);
            self.pos += 1;
            if self.pos >= n {
                self.pos = 0;
                self.counter += 1;
                if self.counter % 2 == 0 {
                    self.compute();
                    return true;
                }
            }
        }
        false
    }

    fn compute(&mut self) {
        let n = self.time.len();
        for i in 0..n {
            self.time[i] *= self.window[i];
        }
        self.scratch.copy_from_slice(&self.time);
        self.fft.process(&mut self.scratch);
        let norm = 2.0 / n as f32;
        for (i, (b0, b1)) in self.ranges.iter().enumerate() {
            let mut peak = 0.0f32;
            for b in *b0..*b1 {
                let m = self.scratch[b].norm() * norm;
                if m > peak {
                    peak = m;
                }
            }
            let db = 20.0 * peak.max(1e-9).log10();
            let prev = self.smoothed[i];
            // Fast attack, slow release for a readable display.
            let next = if db > prev {
                prev + (db - prev) * 0.6
            } else {
                prev + (db - prev) * 0.12
            };
            self.smoothed[i] = next;
        }
    }

    pub fn bins(&self) -> &[f32; SPECTRUM_BINS] {
        &self.smoothed
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        if (sr - self.sample_rate).abs() > 1.0 {
            *self = Self::new(sr);
        }
    }
}
