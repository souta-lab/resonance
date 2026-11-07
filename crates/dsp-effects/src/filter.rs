//! Filter, bass enhancement and loudness compensation.

use dsp_core::biquad::{Biquad, BiquadKind, Cascade};
use dsp_core::envelope::Envelope;
use dsp_core::param::{ParamStore, ParamValue};
use dsp_core::smoothing::Smoothed;
use dsp_core::{AudioEffect, EffectError};

fn id(p: &ParamStore, key: &str) -> u16 {
    p.spec_by_key(key).map(|(i, _)| i).unwrap_or(u16::MAX)
}

pub struct Filter {
    sample_rate: f32,
    cascade: [Cascade; 2],
    ids: [u16; 4],
    last: [(u16, f32, f32, u16); 2],
}

impl Filter {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            cascade: [Cascade::new(2), Cascade::new(2)],
            ids: [u16::MAX; 4],
            last: [(u16::MAX, 0.0, 0.0, u16::MAX); 2],
        }
    }
}

impl AudioEffect for Filter {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [
            id(params, "type"),
            id(params, "frequency"),
            id(params, "resonance"),
            id(params, "mode"),
        ];
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        let ty = p.get_enum(self.ids[0]);
        let freq = p.get_f32(self.ids[1]);
        let res = p.get_f32(self.ids[2]);
        let mode = p.get_enum(self.ids[3]);
        let order = match mode {
            0 => 2,
            1 => 4,
            _ => 8,
        };
        let kind = match ty {
            0 => BiquadKind::LowPass,
            1 => BiquadKind::HighPass,
            2 => BiquadKind::BandPass,
            3 => BiquadKind::Notch,
            4 => BiquadKind::LowShelf,
            _ => BiquadKind::HighShelf,
        };
        let shelf = matches!(kind, BiquadKind::LowShelf | BiquadKind::HighShelf);
        for ch in 0..buf.len().min(2) {
            let key = (ty, freq, res, mode);
            if self.last[ch] != key {
                if shelf {
                    self.cascade[ch] = Cascade::new(2);
                    self.cascade[ch].set(kind, self.sample_rate, freq, 0.0);
                } else {
                    self.cascade[ch] = Cascade::new(order);
                    self.cascade[ch].set(kind, self.sample_rate, freq, 0.0);
                }
                self.last[ch] = key;
            }
            let cascade = &mut self.cascade[ch];
            for x in buf[ch][..frames].iter_mut() {
                *x = cascade.process(ch, *x);
            }
        }
    }

    fn reset(&mut self) {
        for c in self.cascade.iter_mut() {
            c.reset();
        }
        self.last = [(u16::MAX, 0.0, 0.0, u16::MAX); 2];
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

/// Harmonic bass enhancement: low band is rectified/soft-clipped and mixed back.
pub struct BassEnhancer {
    sample_rate: f32,
    lowpass: [Biquad; 2],
    highpass: [Biquad; 2],
    ids: [u16; 6],
    amount: Smoothed,
    blend: Smoothed,
}

impl BassEnhancer {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            lowpass: [Biquad::new(), Biquad::new()],
            highpass: [Biquad::new(), Biquad::new()],
            ids: [u16::MAX; 6],
            amount: Smoothed::new(0.0),
            blend: Smoothed::new(0.0),
        }
    }
}

impl AudioEffect for BassEnhancer {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [
            id(params, "amount"),
            id(params, "harmonics"),
            id(params, "scope"),
            id(params, "blend"),
            id(params, "floor"),
            id(params, "floor-active"),
        ];
        self.amount.set_time_ms(self.sample_rate, 20.0);
        self.blend.set_time_ms(self.sample_rate, 20.0);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        let amount = (p.get_f32(self.ids[0]) / 100.0).clamp(0.0, 1.0);
        let harmonics = (p.get_f32(self.ids[1]) / 100.0).clamp(0.0, 1.0);
        let scope = p.get_enum(self.ids[2]);
        let blend = (p.get_f32(self.ids[3]) / 100.0).clamp(0.0, 1.0);
        let floor = p.get_f32(self.ids[4]);
        let floor_active = p.get_bool(self.ids[5]);
        if amount <= 1e-4 && blend <= 1e-4 {
            return;
        }
        let cutoff = match scope {
            0 => 120.0,
            1 => 400.0,
            _ => 1200.0,
        };
        let cutoff = if floor_active {
            floor.max(cutoff)
        } else {
            cutoff
        };
        for ch in 0..buf.len().min(2) {
            self.lowpass[ch].set(BiquadKind::LowPass, self.sample_rate, cutoff, 0.707, 0.0);
            self.highpass[ch].set(BiquadKind::HighPass, self.sample_rate, cutoff, 0.707, 0.0);
        }
        self.amount.set_target(amount);
        self.blend.set_target(blend);
        for i in 0..frames {
            let a = self.amount.next();
            let b = self.blend.next();
            for ch in 0..buf.len().min(2) {
                let x = buf[ch][i];
                let low = self.lowpass[ch].process(ch, x);
                let high = self.highpass[ch].process(ch, x);
                // Generate harmonics from the low band.
                let driven = (low * (1.0 + 6.0 * a)).tanh();
                let harm = driven - low;
                let enhanced = low + harm * (0.5 + harmonics);
                buf[ch][i] = high + low * (1.0 - b) + enhanced * b;
            }
        }
    }

    fn reset(&mut self) {
        for b in self.lowpass.iter_mut().chain(self.highpass.iter_mut()) {
            b.reset();
        }
        self.amount.snap(0.0);
        self.blend.snap(0.0);
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

/// Level-dependent bass boost following an equal-loudness style curve.
pub struct BassLoudness {
    sample_rate: f32,
    shelf: [Biquad; 2],
    env: Envelope,
    ids: [u16; 2],
    gain: Smoothed,
    last_gain_db: f32,
}

impl BassLoudness {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            shelf: [Biquad::new(), Biquad::new()],
            env: Envelope::new(),
            ids: [u16::MAX; 2],
            gain: Smoothed::new(0.0),
            last_gain_db: f32::NAN,
        }
    }
}

impl AudioEffect for BassLoudness {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [id(params, "amount"), id(params, "threshold")];
        self.env.set_times(self.sample_rate, 30.0, 300.0);
        self.gain.set_time_ms(self.sample_rate, 50.0);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        let amount = (p.get_f32(self.ids[0]) / 100.0).clamp(0.0, 1.0);
        let threshold = p.get_f32(self.ids[1]);
        if amount <= 1e-4 {
            return;
        }
        let mut level = 0.0f32;
        for ch in 0..buf.len().min(2) {
            for &x in &buf[ch][..frames] {
                level = level.max(x.abs());
            }
        }
        let env = self.env.process(level);
        let env_db = 20.0 * env.max(1e-6).log10();
        // More boost when the signal is quiet relative to the threshold.
        let quiet = ((threshold - env_db) / 30.0).clamp(0.0, 1.0);
        let target_db = amount * quiet * 12.0;
        self.gain.set_target(target_db);
        let g = self.gain.next();
        if (g - self.last_gain_db).abs() > 0.01 {
            for ch in 0..buf.len().min(2) {
                self.shelf[ch].set(BiquadKind::LowShelf, self.sample_rate, 120.0, 0.707, g);
            }
            self.last_gain_db = g;
        }
        for ch in 0..buf.len().min(2) {
            let shelf = &mut self.shelf[ch];
            for x in buf[ch][..frames].iter_mut() {
                *x = shelf.process(ch, *x);
            }
        }
    }

    fn reset(&mut self) {
        for b in self.shelf.iter_mut() {
            b.reset();
        }
        self.env.reset();
        self.gain.snap(0.0);
        self.last_gain_db = f32::NAN;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

/// Equal-loudness compensation: bass and treble shelves scaled by the control.
pub struct Loudness {
    sample_rate: f32,
    low: [Biquad; 2],
    high: [Biquad; 2],
    ids: [u16; 2],
    low_gain: Smoothed,
    high_gain: Smoothed,
    last: [f32; 2],
}

impl Loudness {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            low: [Biquad::new(), Biquad::new()],
            high: [Biquad::new(), Biquad::new()],
            ids: [u16::MAX; 2],
            low_gain: Smoothed::new(0.0),
            high_gain: Smoothed::new(0.0),
            last: [f32::NAN; 2],
        }
    }
}

impl AudioEffect for Loudness {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [id(params, "loudness"), id(params, "reference")];
        self.low_gain.set_time_ms(self.sample_rate, 30.0);
        self.high_gain.set_time_ms(self.sample_rate, 30.0);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        let amount = (p.get_f32(self.ids[0]) / 100.0).clamp(0.0, 1.0);
        let reference = p.get_f32(self.ids[1]);
        if amount <= 1e-4 {
            return;
        }
        // Reference 75 dB is neutral; quieter playback gets more compensation.
        let deviation = ((75.0 - reference) / 25.0).clamp(-1.0, 1.0);
        let low_db = amount * (6.0 + 6.0 * deviation);
        let high_db = amount * (2.0 + 3.0 * deviation);
        self.low_gain.set_target(low_db);
        self.high_gain.set_target(high_db);
        let lg = self.low_gain.next();
        let hg = self.high_gain.next();
        if (lg - self.last[0]).abs() > 0.01 || (hg - self.last[1]).abs() > 0.01 {
            for ch in 0..buf.len().min(2) {
                self.low[ch].set(BiquadKind::LowShelf, self.sample_rate, 120.0, 0.707, lg);
                self.high[ch].set(BiquadKind::HighShelf, self.sample_rate, 8_000.0, 0.707, hg);
            }
            self.last = [lg, hg];
        }
        for ch in 0..buf.len().min(2) {
            for i in 0..frames {
                let x = buf[ch][i];
                let y = self.low[ch].process(ch, x);
                buf[ch][i] = self.high[ch].process(ch, y);
            }
        }
    }

    fn reset(&mut self) {
        for b in self.low.iter_mut().chain(self.high.iter_mut()) {
            b.reset();
        }
        self.low_gain.snap(0.0);
        self.high_gain.snap(0.0);
        self.last = [f32::NAN; 2];
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

/// Utility used by the API to render a filter response preview.
pub fn filter_response_db(
    kind: BiquadKind,
    sample_rate: f32,
    freq: f32,
    _q: f32,
    order: usize,
    freqs: &[f32],
) -> Vec<f32> {
    let mut cascade = Cascade::new(order);
    cascade.set(kind, sample_rate, freq, 0.0);
    freqs
        .iter()
        .map(|f| 20.0 * cascade.magnitude(sample_rate, *f).max(1e-9).log10())
        .collect()
}
