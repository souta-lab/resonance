//! Fractional delay line.

/// Power-of-two circular buffer with linear-interpolated reads.
pub struct DelayLine {
    buf: Vec<f32>,
    mask: usize,
    write: usize,
}

impl DelayLine {
    pub fn new(max_samples: usize) -> Self {
        let cap = (max_samples + 4).next_power_of_two();
        Self {
            buf: vec![0.0; cap],
            mask: cap - 1,
            write: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.write = 0;
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        self.buf[self.write] = x;
        self.write = (self.write + 1) & self.mask;
    }

    /// Read `delay` samples back. `delay` is clamped to `[1, capacity-2]`.
    #[inline]
    pub fn read(&self, delay: f32) -> f32 {
        let cap = self.buf.len();
        let d = delay.clamp(1.0, (cap - 2) as f32);
        let i = d.floor();
        let frac = d - i;
        let i = i as usize;
        let a = self.buf[(self.write + cap - i) & self.mask];
        let b = self.buf[(self.write + cap - i - 1) & self.mask];
        a + (b - a) * frac
    }

    #[inline]
    pub fn read_int(&self, delay: usize) -> f32 {
        let cap = self.buf.len();
        let d = delay.clamp(1, cap - 2);
        self.buf[(self.write + cap - d) & self.mask]
    }

    #[inline]
    pub fn last(&self) -> f32 {
        self.buf[(self.write + self.buf.len() - 1) & self.mask]
    }
}

/// Stereo pair of delay lines.
pub struct StereoDelay {
    pub l: DelayLine,
    pub r: DelayLine,
}

impl StereoDelay {
    pub fn new(max_samples: usize) -> Self {
        Self {
            l: DelayLine::new(max_samples),
            r: DelayLine::new(max_samples),
        }
    }

    pub fn clear(&mut self) {
        self.l.clear();
        self.r.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_delay() {
        let mut d = DelayLine::new(16);
        for i in 0..8 {
            d.write(i as f32);
        }
        assert_eq!(d.read_int(1), 7.0);
        assert_eq!(d.read_int(4), 4.0);
    }

    #[test]
    fn fractional_delay_interpolates() {
        let mut d = DelayLine::new(16);
        for i in 0..8 {
            d.write(i as f32);
        }
        let v = d.read(2.5);
        assert!((v - 5.5).abs() < 1e-5, "got {v}");
    }
}
