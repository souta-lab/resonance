//! Tone shaping: harmonic exciter, transient-aware crystalizer and bit crusher.

// DSP loops index several buffers with the same index; iterator rewrites would
// obscure the sample/partition arithmetic.
#![allow(clippy::needless_range_loop)]

use dsp_core::biquad::{Biquad, BiquadKind};
use dsp_core::envelope::Envelope;
use dsp_core::param::{ParamStore, ParamValue};
use dsp_core::smoothing::{fast_tanh, Smoothed};
use dsp_core::{AudioEffect, EffectError};

fn id(p: &ParamStore, key: &str) -> u16 {
    p.spec_by_key(key).map(|(i, _)| i).unwrap_or(u16::MAX)
}

/// Replace non-finite samples so a single NaN cannot poison the state.
#[inline]
fn sanitize(x: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Exciter
// ---------------------------------------------------------------------------

/// Band-limited harmonic exciter.
///
/// The selected band is isolated with a low-pass/high-pass pair, driven through
/// a soft saturator, high-passed again to drop the fundamental and mixed back
/// with the dry signal.
pub struct Exciter {
    sample_rate: f32,
    lp: [Biquad; 2],
    hp: [Biquad; 2],
    harm_hp: [Biquad; 2],
    ceil_lp: [Biquad; 2],
    ids: [u16; 6],
    amount: Smoothed,
    blend: Smoothed,
    last: (u16, f32, bool),
}

impl Exciter {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            lp: [Biquad::new(), Biquad::new()],
            hp: [Biquad::new(), Biquad::new()],
            harm_hp: [Biquad::new(), Biquad::new()],
            ceil_lp: [Biquad::new(), Biquad::new()],
            ids: [u16::MAX; 6],
            amount: Smoothed::new(0.0),
            blend: Smoothed::new(0.0),
            last: (u16::MAX, 0.0, false),
        }
    }
}

impl Default for Exciter {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for Exciter {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate.max(1) as f32;
        self.ids = [
            id(params, "amount"),
            id(params, "harmonics"),
            id(params, "scope"),
            id(params, "blend"),
            id(params, "ceil"),
            id(params, "ceil-active"),
        ];
        self.amount.set_time_ms(self.sample_rate, 20.0);
        self.blend.set_time_ms(self.sample_rate, 20.0);
        self.amount
            .snap((params.get_f32(self.ids[0]) / 100.0).clamp(0.0, 1.0));
        self.blend
            .snap((params.get_f32(self.ids[3]) / 100.0).clamp(0.0, 1.0));
        self.last = (u16::MAX, 0.0, false);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let frames = frames.min(buf[0].len());
        if frames == 0 {
            return;
        }
        let amount = (p.get_f32(self.ids[0]) / 100.0).clamp(0.0, 1.0);
        let harmonics = (p.get_f32(self.ids[1]) / 100.0).clamp(0.0, 1.0);
        let scope = p.get_enum(self.ids[2]);
        let blend = (p.get_f32(self.ids[3]) / 100.0).clamp(0.0, 1.0);
        let ceil = p.get_f32(self.ids[4]).clamp(1000.0, 20_000.0);
        let ceil_active = p.get_bool(self.ids[5]);
        if amount <= 1e-4 || blend <= 1e-4 {
            return;
        }

        let sr = self.sample_rate;
        let (lo, hi): (f32, f32) = match scope {
            0 => (20.0, 200.0),
            1 => (200.0, 2_000.0),
            _ => (2_000.0, 8_000.0),
        };
        let hi = hi.min(sr * 0.45);
        let lo = lo.min(hi * 0.5);
        if self.last != (scope, ceil, ceil_active) {
            for ch in 0..2 {
                self.lp[ch].set(BiquadKind::LowPass, sr, hi, 0.707, 0.0);
                self.hp[ch].set(BiquadKind::HighPass, sr, lo, 0.707, 0.0);
                self.harm_hp[ch].set(BiquadKind::HighPass, sr, hi, 0.707, 0.0);
                self.ceil_lp[ch].set(BiquadKind::LowPass, sr, ceil, 0.707, 0.0);
            }
            self.last = (scope, ceil, ceil_active);
        }

        let drive = 1.0 + 6.0 * harmonics;
        self.amount.set_target(amount);
        self.blend.set_target(blend);
        let chans = buf.len().min(2);
        for i in 0..frames {
            let gain = self.amount.next() * self.blend.next() * 2.0;
            for ch in 0..chans {
                let x = sanitize(buf[ch][i]);
                let low = self.lp[ch].process(ch, x);
                let band = self.hp[ch].process(ch, low);
                let driven = fast_tanh(band * drive);
                let mut harm = driven - band;
                harm = self.harm_hp[ch].process(ch, harm);
                if ceil_active {
                    harm = self.ceil_lp[ch].process(ch, harm);
                }
                buf[ch][i] = x + harm * gain;
            }
        }
    }

    fn reset(&mut self) {
        for b in self
            .lp
            .iter_mut()
            .chain(self.hp.iter_mut())
            .chain(self.harm_hp.iter_mut())
            .chain(self.ceil_lp.iter_mut())
        {
            b.reset();
        }
        self.amount.snap(0.0);
        self.blend.snap(0.0);
        self.last = (u16::MAX, 0.0, false);
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Crystalizer
// ---------------------------------------------------------------------------

/// Transient-aware high-frequency enhancement.
///
/// A fast and a slow envelope follower are compared; the difference drives a
/// high-band boost so attacks get more "crystal" than steady material.
pub struct Crystalizer {
    sample_rate: f32,
    lp: [Biquad; 2],
    fast: [Envelope; 2],
    slow: [Envelope; 2],
    ids: [u16; 2],
    amount: Smoothed,
    intensity: Smoothed,
}

impl Crystalizer {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            lp: [Biquad::new(), Biquad::new()],
            fast: [Envelope::new(), Envelope::new()],
            slow: [Envelope::new(), Envelope::new()],
            ids: [u16::MAX; 2],
            amount: Smoothed::new(0.0),
            intensity: Smoothed::new(0.0),
        }
    }
}

impl Default for Crystalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for Crystalizer {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate.max(1) as f32;
        self.ids = [id(params, "amount"), id(params, "intensity")];
        let sr = self.sample_rate;
        for ch in 0..2 {
            self.lp[ch].set(BiquadKind::LowPass, sr, 4_000.0, 0.707, 0.0);
            self.fast[ch].set_times(sr, 0.5, 15.0);
            self.slow[ch].set_times(sr, 30.0, 150.0);
        }
        self.amount.set_time_ms(sr, 20.0);
        self.intensity.set_time_ms(sr, 20.0);
        self.amount
            .snap((params.get_f32(self.ids[0]) / 100.0).clamp(0.0, 1.0));
        self.intensity
            .snap((params.get_f32(self.ids[1]) / 10.0).clamp(-1.0, 1.0));
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let frames = frames.min(buf[0].len());
        if frames == 0 {
            return;
        }
        let amount = (p.get_f32(self.ids[0]) / 100.0).clamp(0.0, 1.0);
        let intensity = (p.get_f32(self.ids[1]) / 10.0).clamp(-1.0, 1.0);
        if amount <= 1e-4 || intensity.abs() <= 1e-4 {
            return;
        }
        self.amount.set_target(amount);
        self.intensity.set_target(intensity);
        let chans = buf.len().min(2);
        for i in 0..frames {
            let a = self.amount.next();
            let inten = self.intensity.next();
            for ch in 0..chans {
                let x = sanitize(buf[ch][i]);
                let low = self.lp[ch].process(ch, x);
                let high = x - low;
                let f = self.fast[ch].process(x.abs());
                let s = self.slow[ch].process(x.abs());
                let transient = ((f - s) / (s + 1e-3)).clamp(0.0, 1.0);
                let boost = a * inten * (0.35 + 1.65 * transient);
                buf[ch][i] = x + high * boost;
            }
        }
    }

    fn reset(&mut self) {
        for b in self.lp.iter_mut() {
            b.reset();
        }
        for e in self.fast.iter_mut().chain(self.slow.iter_mut()) {
            e.reset();
        }
        self.amount.snap(0.0);
        self.intensity.snap(0.0);
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Crusher
// ---------------------------------------------------------------------------

/// Bit-depth and sample-rate reduction.
pub struct Crusher {
    sample_rate: f32,
    ids: [u16; 3],
    hold: [f32; 2],
    counter: [u32; 2],
    mix: Smoothed,
}

impl Crusher {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            ids: [u16::MAX; 3],
            hold: [0.0; 2],
            counter: [0; 2],
            mix: Smoothed::new(1.0),
        }
    }
}

impl Default for Crusher {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for Crusher {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate.max(1) as f32;
        self.ids = [
            id(params, "bit-depth"),
            id(params, "sample-rate"),
            id(params, "mix"),
        ];
        self.mix.set_time_ms(self.sample_rate, 10.0);
        self.mix
            .snap((params.get_f32(self.ids[2]) / 100.0).clamp(0.0, 1.0));
        self.hold = [0.0; 2];
        self.counter = [0; 2];
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let frames = frames.min(buf[0].len());
        if frames == 0 {
            return;
        }
        let bits = p.get_f32(self.ids[0]).clamp(2.0, 32.0);
        let half = 2f32.powf(bits) * 0.5;
        let target_rate = p.get_f32(self.ids[1]).clamp(1_000.0, 192_000.0);
        let step = (self.sample_rate / target_rate).max(1.0).round() as u32;
        let mix = (p.get_f32(self.ids[2]) / 100.0).clamp(0.0, 1.0);
        self.mix.set_target(mix);
        let chans = buf.len().min(2);
        for i in 0..frames {
            let m = self.mix.next();
            for ch in 0..chans {
                let x = sanitize(buf[ch][i]);
                if self.counter[ch] == 0 {
                    let xc = x.clamp(-1.0, 1.0);
                    self.hold[ch] = (xc * half).round() / half;
                    self.counter[ch] = step;
                }
                self.counter[ch] -= 1;
                let wet = self.hold[ch];
                buf[ch][i] = x + (wet - x) * m;
            }
        }
    }

    fn reset(&mut self) {
        self.hold = [0.0; 2];
        self.counter = [0; 2];
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use dsp_core::EffectKind;

    fn store(kind: EffectKind) -> ParamStore {
        ParamStore::new(catalog::descriptor(kind).params, None)
    }

    fn set(p: &ParamStore, key: &str, v: f32) {
        let id = p.spec_by_key(key).expect("param key").0;
        p.set_f32(id, v);
    }

    #[test]
    fn crusher_quantizes_to_bit_depth() {
        let params = store(EffectKind::Crusher);
        set(&params, "bit-depth", 2.0);
        set(&params, "sample-rate", 48_000.0);
        set(&params, "mix", 100.0);
        let mut c = Crusher::new();
        c.prepare(48_000, 128, 1, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 256]];
        for i in 0..256 {
            buf[0][i] = (i as f32 * 0.1).sin() * 0.9;
        }
        c.process(&mut buf, 256, &params);
        for &v in &buf[0] {
            let q = (v * 2.0).round() / 2.0;
            assert!((v - q).abs() < 1e-5, "value {v} is not quantized");
        }
    }

    #[test]
    fn crusher_holds_samples_at_target_rate() {
        let params = store(EffectKind::Crusher);
        set(&params, "bit-depth", 32.0);
        set(&params, "sample-rate", 12_000.0);
        set(&params, "mix", 100.0);
        let mut c = Crusher::new();
        c.prepare(48_000, 128, 1, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 16]];
        for i in 0..16 {
            buf[0][i] = i as f32 / 16.0;
        }
        c.process(&mut buf, 16, &params);
        assert_eq!(buf[0][0], buf[0][1]);
        assert_eq!(buf[0][1], buf[0][2]);
        assert_eq!(buf[0][2], buf[0][3]);
        assert_ne!(buf[0][3], buf[0][4]);
    }

    #[test]
    fn crusher_mix_zero_is_transparent() {
        let params = store(EffectKind::Crusher);
        set(&params, "bit-depth", 2.0);
        set(&params, "sample-rate", 1_000.0);
        set(&params, "mix", 0.0);
        let mut c = Crusher::new();
        c.prepare(48_000, 128, 1, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 64]];
        for i in 0..64 {
            buf[0][i] = (i as f32 * 0.2).sin();
        }
        let orig = buf.clone();
        c.process(&mut buf, 64, &params);
        assert_eq!(buf, orig);
    }

    #[test]
    fn exciter_adds_harmonics() {
        let params = store(EffectKind::Exciter);
        set(&params, "amount", 100.0);
        set(&params, "harmonics", 100.0);
        set(&params, "blend", 100.0);
        set(&params, "scope", 0.0);
        let mut e = Exciter::new();
        e.prepare(48_000, 128, 1, &params).unwrap();
        let n = 4800;
        let mut buf = vec![vec![0.0f32; n]];
        for i in 0..n {
            buf[0][i] = (2.0 * std::f32::consts::PI * 100.0 * i as f32 / 48_000.0).sin() * 0.5;
        }
        e.process(&mut buf, n, &params);
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for i in 0..n {
            let phase = 2.0 * std::f32::consts::PI * 300.0 * i as f32 / 48_000.0;
            re += buf[0][i] * phase.cos();
            im -= buf[0][i] * phase.sin();
        }
        let mag = (re * re + im * im).sqrt() / n as f32;
        assert!(mag > 1e-3, "third harmonic magnitude {mag}");
    }

    #[test]
    fn exciter_is_transparent_at_zero_amount() {
        let params = store(EffectKind::Exciter);
        set(&params, "amount", 0.0);
        set(&params, "blend", 100.0);
        let mut e = Exciter::new();
        e.prepare(48_000, 128, 1, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 128]];
        for i in 0..128 {
            buf[0][i] = (i as f32 * 0.3).sin();
        }
        let orig = buf.clone();
        e.process(&mut buf, 128, &params);
        assert_eq!(buf, orig);
    }

    #[test]
    fn crystalizer_boosts_transients_more_than_steady_state() {
        let params = store(EffectKind::Crystalizer);
        set(&params, "amount", 100.0);
        set(&params, "intensity", 10.0);
        let mut c = Crystalizer::new();
        c.prepare(48_000, 128, 1, &params).unwrap();
        let n = 4800;
        let mut buf = vec![vec![0.0f32; n]];
        for i in 2400..n {
            buf[0][i] = if i % 2 == 0 { 0.5 } else { -0.5 };
        }
        let dry = buf.clone();
        c.process(&mut buf, n, &params);
        let sum =
            |b: &[f32], a: usize, z: usize| -> f32 { b[a..z].iter().map(|x| x.abs()).sum::<f32>() };
        let onset = sum(&buf[0], 2400, 2450) / sum(&dry[0], 2400, 2450);
        let steady = sum(&buf[0], 4000, 4050) / sum(&dry[0], 4000, 4050);
        assert!(onset > steady, "onset {onset} steady {steady}");
    }

    #[test]
    fn exciter_survives_non_finite_input() {
        let params = store(EffectKind::Exciter);
        set(&params, "amount", 100.0);
        set(&params, "blend", 100.0);
        let mut e = Exciter::new();
        e.prepare(48_000, 128, 1, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 512]];
        for i in 0..512 {
            buf[0][i] = if i % 7 == 0 {
                f32::NAN
            } else if i % 11 == 0 {
                f32::INFINITY
            } else {
                (i as f32 * 0.1).sin()
            };
        }
        e.process(&mut buf, 512, &params);
        assert!(buf[0].iter().all(|x| x.is_finite()));
    }

    #[test]
    fn crystalizer_is_transparent_at_zero_intensity() {
        let params = store(EffectKind::Crystalizer);
        set(&params, "amount", 100.0);
        set(&params, "intensity", 0.0);
        let mut c = Crystalizer::new();
        c.prepare(48_000, 128, 1, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 128]];
        for i in 0..128 {
            buf[0][i] = (i as f32 * 0.3).sin();
        }
        let orig = buf.clone();
        c.process(&mut buf, 128, &params);
        assert_eq!(buf, orig);
    }
}
