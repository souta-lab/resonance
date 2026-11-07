//! Envelope followers and detectors.

/// Attack/release envelope follower (one-pole, per sample).
#[derive(Clone, Copy, Debug)]
pub struct Envelope {
    env: f32,
    attack: f32,
    release: f32,
}

impl Envelope {
    pub fn new() -> Self {
        Self {
            env: 0.0,
            attack: 0.5,
            release: 0.99,
        }
    }

    pub fn set_times(&mut self, sample_rate: f32, attack_ms: f32, release_ms: f32) {
        let sr = sample_rate.max(1.0);
        self.attack = (-1.0 / (sr * attack_ms.max(0.05) / 1000.0)).exp();
        self.release = (-1.0 / (sr * release_ms.max(0.05) / 1000.0)).exp();
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let a = if x > self.env {
            self.attack
        } else {
            self.release
        };
        self.env = x + a * (self.env - x);
        self.env
    }

    pub fn value(&self) -> f32 {
        self.env
    }

    pub fn reset(&mut self) {
        self.env = 0.0;
    }
}

impl Default for Envelope {
    fn default() -> Self {
        Self::new()
    }
}

/// Peak detector with hold.
#[derive(Clone, Copy, Debug)]
pub struct PeakHold {
    peak: f32,
    hold: u32,
    hold_samples: u32,
    decay: f32,
}

impl PeakHold {
    pub fn new() -> Self {
        Self {
            peak: 0.0,
            hold: 0,
            hold_samples: 0,
            decay: 0.9995,
        }
    }

    pub fn set_hold_ms(&mut self, sample_rate: f32, ms: f32) {
        self.hold_samples = (sample_rate * ms / 1000.0) as u32;
    }

    pub fn set_decay(&mut self, decay: f32) {
        self.decay = decay.clamp(0.0, 0.999_99);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let a = x.abs();
        if a >= self.peak {
            self.peak = a;
            self.hold = self.hold_samples;
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.peak *= self.decay;
        }
        self.peak
    }

    pub fn value(&self) -> f32 {
        self.peak
    }

    pub fn reset(&mut self) {
        self.peak = 0.0;
        self.hold = 0;
    }
}

impl Default for PeakHold {
    fn default() -> Self {
        Self::new()
    }
}

/// RMS detector with a fixed window (one-pole on the squared signal).
#[derive(Clone, Copy, Debug)]
pub struct Rms {
    mean_sq: f32,
    coeff: f32,
}

impl Rms {
    pub fn new() -> Self {
        Self {
            mean_sq: 0.0,
            coeff: 0.999,
        }
    }

    pub fn set_window_ms(&mut self, sample_rate: f32, ms: f32) {
        let samples = (sample_rate * ms.max(1.0) / 1000.0).max(1.0);
        self.coeff = (-1.0 / samples).exp();
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.mean_sq = x * x + self.coeff * (self.mean_sq - x * x);
        self.mean_sq.max(0.0).sqrt()
    }

    pub fn value(&self) -> f32 {
        self.mean_sq.max(0.0).sqrt()
    }

    pub fn reset(&mut self) {
        self.mean_sq = 0.0;
    }
}

impl Default for Rms {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple attack/release gain computer used by dynamics effects.
#[derive(Clone, Copy, Debug)]
pub struct GainComputer {
    pub threshold_db: f32,
    pub ratio: f32,
    pub knee_db: f32,
    pub makeup_db: f32,
}

impl GainComputer {
    pub fn new() -> Self {
        Self {
            threshold_db: -18.0,
            ratio: 4.0,
            knee_db: 6.0,
            makeup_db: 0.0,
        }
    }

    /// Returns gain reduction in dB (<= 0) for an input level in dB.
    pub fn compute(&self, input_db: f32) -> f32 {
        let ratio = self.ratio.max(1.0);
        let over = input_db - self.threshold_db;
        let knee = self.knee_db.max(0.0);
        let gr = if knee > 0.0 && over > -knee * 0.5 && over < knee * 0.5 {
            let t = over + knee * 0.5;
            -(1.0 - 1.0 / ratio) * t * t / (2.0 * knee)
        } else if over > 0.0 {
            -(1.0 - 1.0 / ratio) * over
        } else {
            0.0
        };
        gr + self.makeup_db
    }
}

impl Default for GainComputer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_reaches_target() {
        let mut e = Envelope::new();
        e.set_times(48_000.0, 1.0, 50.0);
        for _ in 0..4800 {
            e.process(1.0);
        }
        assert!(e.value() > 0.99);
    }

    #[test]
    fn gain_computer_ratio() {
        let mut g = GainComputer::new();
        g.threshold_db = -20.0;
        g.ratio = 4.0;
        g.knee_db = 0.0;
        let gr = g.compute(-10.0);
        assert!((gr - (-7.5)).abs() < 1e-4, "got {gr}");
    }
}
