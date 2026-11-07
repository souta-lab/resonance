//! Parametric equalizers (left/right and mid/side).

use dsp_core::biquad::{Biquad, BiquadCoeffs, BiquadKind};
use dsp_core::param::{ParamStore, ParamValue};
use dsp_core::{AudioEffect, EffectError};

const MAX_BANDS: usize = 32;

#[derive(Clone, Copy, Default)]
struct BandCache {
    kind: u16,
    freq: f32,
    gain: f32,
    q: f32,
    valid: bool,
}

#[derive(Clone, Copy, Default)]
struct Ids {
    num_bands: u16,
    split: u16,
    balance: u16,
    pitch_l: u16,
    pitch_r: u16,
}

fn id(p: &ParamStore, key: &str) -> u16 {
    p.spec_by_key(key).map(|(i, _)| i).unwrap_or(u16::MAX)
}

fn kind_from_index(i: u16) -> BiquadKind {
    match i {
        1 => BiquadKind::LowShelf,
        2 => BiquadKind::HighShelf,
        3 => BiquadKind::LowPass,
        4 => BiquadKind::HighPass,
        5 => BiquadKind::Notch,
        6 => BiquadKind::BandPass,
        7 => BiquadKind::AllPass,
        _ => BiquadKind::Peak,
    }
}

pub struct Equalizer {
    sample_rate: f32,
    mid_side: bool,
    bands: [[Biquad; MAX_BANDS]; 2],
    cache: [[BandCache; MAX_BANDS]; 2],
    ids: Ids,
    scratch: [Vec<f32>; 2],
    balance_gain: [f32; 2],
}

impl Equalizer {
    pub fn new(mid_side: bool) -> Self {
        Self {
            sample_rate: 48_000.0,
            mid_side,
            bands: std::array::from_fn(|_| std::array::from_fn(|_| Biquad::new())),
            cache: [[BandCache::default(); MAX_BANDS]; 2],
            ids: Ids::default(),
            scratch: [Vec::new(), Vec::new()],
            balance_gain: [1.0, 1.0],
        }
    }

    fn update_coeffs(&mut self, p: &ParamStore, num_bands: usize, split: bool, pitch: [f32; 2]) {
        for ch in 0..2 {
            let src = if split { ch } else { 0 };
            let scale = 2f32.powf(pitch[ch] / 12.0);
            for b in 0..num_bands {
                let kind = p.band_value(src, b, 0) as u16;
                let freq = (p.band_value(src, b, 1) * scale).clamp(10.0, self.sample_rate * 0.49);
                let gain = p.band_value(src, b, 2);
                let q = p.band_value(src, b, 3).max(0.05);
                let c = &mut self.cache[ch][b];
                if !c.valid
                    || c.kind != kind
                    || (c.freq - freq).abs() > 1e-3
                    || (c.gain - gain).abs() > 1e-4
                    || (c.q - q).abs() > 1e-4
                {
                    self.bands[ch][b].set(kind_from_index(kind), self.sample_rate, freq, q, gain);
                    *c = BandCache {
                        kind,
                        freq,
                        gain,
                        q,
                        valid: true,
                    };
                }
            }
        }
    }
}

impl AudioEffect for Equalizer {
    fn prepare(
        &mut self,
        sample_rate: u32,
        max_frames: usize,
        channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = Ids {
            num_bands: id(params, "num-bands"),
            split: id(params, "split-channels"),
            balance: id(params, "balance"),
            pitch_l: id(params, "pitch-left"),
            pitch_r: id(params, "pitch-right"),
        };
        let _ = channels;
        self.scratch = [vec![0.0; max_frames.max(16)], vec![0.0; max_frames.max(16)]];
        self.cache = [[BandCache::default(); MAX_BANDS]; 2];
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let frames = frames.min(buf[0].len());
        let num_bands = (p.get_f32(self.ids.num_bands) as usize).clamp(1, MAX_BANDS);
        let split = p.get_bool(self.ids.split);
        let balance = p.get_f32(self.ids.balance).clamp(-1.0, 1.0);
        let pitch = [p.get_f32(self.ids.pitch_l), p.get_f32(self.ids.pitch_r)];
        self.update_coeffs(p, num_bands, split, pitch);

        // Equal-power balance.
        let angle = (balance + 1.0) * std::f32::consts::FRAC_PI_4;
        self.balance_gain = [angle.cos(), angle.sin()];

        let mut solo = [false; 2];
        for ch in 0..2 {
            let src = if split { ch } else { 0 };
            for b in 0..num_bands {
                if p.band_value(src, b, 6) != 0.0 {
                    solo[ch] = true;
                }
            }
        }

        if self.mid_side {
            let (l, r) = buf.split_at_mut(1);
            let (left, right) = (&mut l[0], &mut r[0]);
            for i in 0..frames {
                let m = (left[i] + right[i]) * 0.5;
                let s = (left[i] - right[i]) * 0.5;
                self.scratch[0][i] = m;
                self.scratch[1][i] = s;
            }
            for ch in 0..2 {
                let src = if split { ch } else { 0 };
                for b in 0..num_bands {
                    if p.band_value(src, b, 7) != 0.0 {
                        continue;
                    }
                    if solo[ch] && p.band_value(src, b, 6) == 0.0 {
                        continue;
                    }
                    let band = &mut self.bands[ch][b];
                    for x in self.scratch[ch][..frames].iter_mut() {
                        *x = band.process(ch, *x);
                    }
                }
            }
            for i in 0..frames {
                let m = self.scratch[0][i] * self.balance_gain[0];
                let s = self.scratch[1][i] * self.balance_gain[1];
                left[i] = m + s;
                right[i] = m - s;
            }
        } else {
            for ch in 0..buf.len().min(2) {
                let src = if split { ch } else { 0 };
                for b in 0..num_bands {
                    if p.band_value(src, b, 7) != 0.0 {
                        continue;
                    }
                    if solo[ch] && p.band_value(src, b, 6) == 0.0 {
                        continue;
                    }
                    let band = &mut self.bands[ch][b];
                    for x in buf[ch][..frames].iter_mut() {
                        *x = band.process(ch, *x);
                    }
                }
                let g = self.balance_gain[ch];
                if (g - 1.0).abs() > 1e-6 {
                    for x in buf[ch][..frames].iter_mut() {
                        *x *= g;
                    }
                }
            }
        }
    }

    fn reset(&mut self) {
        for ch in 0..2 {
            for b in 0..MAX_BANDS {
                self.bands[ch][b].reset();
            }
        }
        self.cache = [[BandCache::default(); MAX_BANDS]; 2];
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

/// Magnitude response of the equalizer at the given frequencies, for the UI
/// curve. Uses the same coefficient code as the audio path.
pub fn response_db(params: &ParamStore, sample_rate: f32, freqs: &[f32]) -> Vec<f32> {
    let num_bands = (params.get_f32(id(params, "num-bands")) as usize).clamp(1, MAX_BANDS);
    let split = params.get_bool(id(params, "split-channels"));
    let pitch = [
        params.get_f32(id(params, "pitch-left")),
        params.get_f32(id(params, "pitch-right")),
    ];
    let mut out = vec![0.0f32; freqs.len()];
    for ch in 0..2 {
        let src = if split { ch } else { 0 };
        let scale = 2f32.powf(pitch[ch] / 12.0);
        let mut coeffs = Vec::with_capacity(num_bands);
        for b in 0..num_bands {
            if params.band_value(src, b, 7) != 0.0 {
                continue;
            }
            let kind = kind_from_index(params.band_value(src, b, 0) as u16);
            let freq = (params.band_value(src, b, 1) * scale).clamp(10.0, sample_rate * 0.49);
            let gain = params.band_value(src, b, 2);
            let q = params.band_value(src, b, 3).max(0.05);
            coeffs.push(BiquadCoeffs::new(kind, sample_rate, freq, q, gain));
        }
        for (i, f) in freqs.iter().enumerate() {
            let mut mag = 1.0f32;
            for c in &coeffs {
                mag *= c.magnitude(sample_rate, *f);
            }
            let db = 20.0 * mag.max(1e-9).log10();
            out[i] += db * 0.5;
        }
    }
    out
}
