//! Small DSP utilities shared by the effects.

/// Convert dB to linear gain.
#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Convert linear gain to dB, with a floor so that silence maps to `-120`.
#[inline]
pub fn lin_to_db(lin: f32) -> f32 {
    if lin <= 1e-6 {
        -120.0
    } else {
        20.0 * lin.log10()
    }
}

/// One-pole smoother with a time constant in milliseconds.
#[derive(Clone, Copy, Debug)]
pub struct Smoothed {
    current: f32,
    target: f32,
    coeff: f32,
    initialized: bool,
}

impl Smoothed {
    pub fn new(value: f32) -> Self {
        Self {
            current: value,
            target: value,
            coeff: 0.0,
            initialized: true,
        }
    }

    pub fn set_time_ms(&mut self, sample_rate: f32, ms: f32) {
        let samples = (sample_rate * ms / 1000.0).max(1.0);
        self.coeff = (-1.0 / samples).exp();
    }

    pub fn set_target(&mut self, v: f32) {
        self.target = v;
    }

    pub fn snap(&mut self, v: f32) {
        self.current = v;
        self.target = v;
        self.initialized = true;
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        if !self.initialized {
            self.current = self.target;
            self.initialized = true;
        }
        self.current = self.target + self.coeff * (self.current - self.target);
        self.current
    }

    pub fn current(&self) -> f32 {
        self.current
    }

    pub fn is_settled(&self) -> bool {
        (self.current - self.target).abs() < 1e-5
    }
}

/// Equal-power crossfade weights for a 0..1 position.
#[inline]
pub fn equal_power(p: f32) -> (f32, f32) {
    let p = p.clamp(0.0, 1.0);
    let a = (p * std::f32::consts::FRAC_PI_2).sin();
    let b = (p * std::f32::consts::FRAC_PI_2).cos();
    (b, a)
}

/// Soft clipper with a smooth knee, unity slope at 0.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    if x.abs() <= 0.5 {
        x
    } else {
        let s = x.signum();
        let a = x.abs();
        s * (1.0 - 0.25 / a)
    }
}

/// Fast tanh approximation (Padé), accurate to ~1e-4 for |x| < 3.
#[inline]
pub fn fast_tanh(x: f32) -> f32 {
    let x = x.clamp(-4.0, 4.0);
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

/// Linear interpolation.
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Convert a MIDI note number to Hz.
#[inline]
pub fn midi_to_hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// Convert Hz to a fractional MIDI note number.
#[inline]
pub fn hz_to_midi(hz: f32) -> f32 {
    69.0 + 12.0 * (hz.max(1e-6) / 440.0).log2()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_roundtrip() {
        assert!((db_to_lin(-6.0) - 0.501_187).abs() < 1e-4);
        assert!((lin_to_db(db_to_lin(-12.0)) + 12.0).abs() < 1e-4);
    }

    #[test]
    fn midi_roundtrip() {
        assert!((midi_to_hz(69.0) - 440.0).abs() < 1e-3);
        assert!((hz_to_midi(440.0) - 69.0).abs() < 1e-3);
    }
}
