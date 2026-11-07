//! Noise & echo effects: STFT spectral subtraction, a model-free spectral gate
//! and an NLMS adaptive echo canceller.

use std::f32::consts::PI;
use std::sync::Arc;

use dsp_core::delay::DelayLine;
use dsp_core::param::{ParamStore, ParamValue};
use dsp_core::smoothing::db_to_lin;
use dsp_core::{AudioEffect, EffectError};
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

fn id(p: &ParamStore, key: &str) -> u16 {
    p.spec_by_key(key).map(|(i, _)| i).unwrap_or(u16::MAX)
}

/// Flush denormals and replace non-finite samples with silence.
#[inline]
fn sanitize(x: f32) -> f32 {
    if x.is_finite() {
        if x.abs() < 1e-20 {
            0.0
        } else {
            x
        }
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Shared STFT engine
// ---------------------------------------------------------------------------

const FFT_SIZE: usize = 1024;
const HOP: usize = FFT_SIZE / 2;
const BINS: usize = FFT_SIZE / 2 + 1;
const MASK: usize = FFT_SIZE - 1;

#[derive(Clone, Copy)]
struct SpectralConfig {
    /// 0..1 wet amount; 0 is a bypass.
    amount: f32,
    /// Linear gain floor (maximum attenuation).
    floor: f32,
    /// One-pole coefficients per STFT frame.
    attack: f32,
    release: f32,
    /// 0 = spectral subtraction, 1 = Wiener.
    model: u16,
    /// Power over-subtraction factor.
    oversub: f32,
    /// Bias applied to the minimum-statistics noise estimate.
    bias: f32,
    /// Frames between noise-profile updates.
    min_frames: usize,
    freq_smooth: bool,
    tonal_protect: bool,
}

/// One channel of a 1024-point, 50%-overlap Hann STFT with overlap-add
/// resynthesis and a minimum-statistics noise tracker.
///
/// All buffers are allocated in [`StftChannel::alloc`]; `push` only touches
/// preallocated memory. The reported latency is one full window: the first
/// output sample is produced after `FFT_SIZE` input samples.
struct StftChannel {
    fwd: Option<Arc<dyn Fft<f32>>>,
    inv: Option<Arc<dyn Fft<f32>>>,
    in_buf: Vec<f32>,
    in_pos: usize,
    fill: usize,
    first: bool,
    ola: Vec<f32>,
    out_buf: Vec<f32>,
    out_pos: usize,
    out_len: usize,
    freq: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    window: Vec<f32>,
    noise: Vec<f32>,
    min_track: Vec<f32>,
    p_smooth: Vec<f32>,
    gain: Vec<f32>,
    g_raw: Vec<f32>,
    g_target: Vec<f32>,
    min_counter: usize,
    initialized: bool,
    gr_db: f32,
}

impl StftChannel {
    fn new() -> Self {
        Self {
            fwd: None,
            inv: None,
            in_buf: Vec::new(),
            in_pos: 0,
            fill: 0,
            first: true,
            ola: Vec::new(),
            out_buf: Vec::new(),
            out_pos: 0,
            out_len: 0,
            freq: Vec::new(),
            scratch: Vec::new(),
            window: Vec::new(),
            noise: Vec::new(),
            min_track: Vec::new(),
            p_smooth: Vec::new(),
            gain: Vec::new(),
            g_raw: Vec::new(),
            g_target: Vec::new(),
            min_counter: 0,
            initialized: false,
            gr_db: 0.0,
        }
    }

    fn alloc(&mut self, fwd: Arc<dyn Fft<f32>>, inv: Arc<dyn Fft<f32>>, scratch_len: usize) {
        self.fwd = Some(fwd);
        self.inv = Some(inv);
        self.in_buf = vec![0.0; FFT_SIZE];
        self.ola = vec![0.0; FFT_SIZE];
        self.out_buf = vec![0.0; HOP];
        self.freq = vec![Complex::new(0.0, 0.0); FFT_SIZE];
        self.scratch = vec![Complex::new(0.0, 0.0); scratch_len.max(1)];
        self.window = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FFT_SIZE as f32).cos())
            .collect();
        self.noise = vec![0.0; BINS];
        self.min_track = vec![f32::MAX; BINS];
        self.p_smooth = vec![0.0; BINS];
        self.gain = vec![1.0; BINS];
        self.g_raw = vec![1.0; BINS];
        self.g_target = vec![1.0; BINS];
        self.reset();
    }

    fn reset(&mut self) {
        self.in_buf.fill(0.0);
        self.ola.fill(0.0);
        self.out_buf.fill(0.0);
        self.freq.fill(Complex::new(0.0, 0.0));
        self.noise.fill(0.0);
        self.min_track.fill(f32::MAX);
        self.p_smooth.fill(0.0);
        self.gain.fill(1.0);
        self.g_raw.fill(1.0);
        self.g_target.fill(1.0);
        self.in_pos = 0;
        self.fill = 0;
        self.first = true;
        self.out_pos = 0;
        self.out_len = 0;
        self.min_counter = 0;
        self.initialized = false;
        self.gr_db = 0.0;
    }

    /// Push one input sample and return one output sample. The output is
    /// delayed by one window because the first frame is only complete after
    /// `FFT_SIZE` samples.
    fn push(&mut self, x: f32, cfg: &SpectralConfig) -> f32 {
        let y = if self.out_pos < self.out_len {
            let v = self.out_buf[self.out_pos];
            self.out_pos += 1;
            v
        } else {
            0.0
        };
        let x = sanitize(x);
        self.in_buf[self.in_pos] = x;
        self.in_pos = (self.in_pos + 1) & MASK;
        self.fill += 1;
        let threshold = if self.first { FFT_SIZE } else { HOP };
        if self.fill >= threshold {
            self.fill = 0;
            self.first = false;
            self.process_frame(cfg);
            self.out_pos = 0;
            self.out_len = HOP;
        }
        y
    }

    fn process_frame(&mut self, cfg: &SpectralConfig) {
        let (Some(fwd), Some(inv)) = (self.fwd.as_ref(), self.inv.as_ref()) else {
            return;
        };
        for i in 0..FFT_SIZE {
            let idx = (self.in_pos + i) & MASK;
            self.freq[i] = Complex::new(self.in_buf[idx] * self.window[i], 0.0);
        }
        fwd.process_with_scratch(&mut self.freq, &mut self.scratch);

        if !self.initialized {
            // Assume the first frame is noise so that steady noise is reduced
            // immediately; the minimum tracker refines the estimate afterwards.
            for b in 0..BINS {
                let p = self.freq[b].norm_sqr().max(1e-20);
                self.p_smooth[b] = p;
                self.min_track[b] = p;
                self.noise[b] = p;
                self.gain[b] = 1.0;
            }
            self.initialized = true;
        } else {
            for b in 0..BINS {
                let p = self.freq[b].norm_sqr();
                let p = if p.is_finite() { p.max(1e-20) } else { 1e-20 };
                // Fast attack, slow release on the power estimate.
                let a = if p > self.p_smooth[b] { 0.5 } else { 0.9 };
                self.p_smooth[b] = a * p + (1.0 - a) * self.p_smooth[b];
                if self.p_smooth[b] < self.min_track[b] {
                    self.min_track[b] = self.p_smooth[b];
                }
            }
            self.min_counter += 1;
            if self.min_counter >= cfg.min_frames {
                self.min_counter = 0;
                for b in 0..BINS {
                    self.noise[b] = (self.min_track[b] * cfg.bias).max(1e-20);
                    self.min_track[b] = self.p_smooth[b];
                }
            }
        }

        for b in 0..BINS {
            let p = self.p_smooth[b].max(1e-20);
            let n = self.noise[b] * cfg.oversub;
            let ratio = (n / p).clamp(0.0, 1.0);
            let mut g = match cfg.model {
                0 => (1.0 - ratio).max(0.0).sqrt(),
                _ => (1.0 - ratio).max(0.0),
            };
            g = g.max(cfg.floor);
            if cfg.tonal_protect {
                // Strong spectral peaks are usually tonal (voice harmonics,
                // beeps). Compare against a neighbourhood outside the main
                // lobe so a tone between two bins is still recognised.
                let mut local = 0.0f32;
                let mut count = 0.0f32;
                for k in 3..=8 {
                    if b >= k {
                        local += self.p_smooth[b - k];
                        count += 1.0;
                    }
                    if b + k < BINS {
                        local += self.p_smooth[b + k];
                        count += 1.0;
                    }
                }
                if count > 0.0 && p > 4.0 * (local / count) {
                    g = g.max(0.7);
                }
            }
            self.g_raw[b] = g;
        }

        if cfg.freq_smooth {
            for b in 0..BINS {
                let l = self.g_raw[b.saturating_sub(1)];
                let r = self.g_raw[(b + 1).min(BINS - 1)];
                self.g_target[b] = 0.25 * l + 0.5 * self.g_raw[b] + 0.25 * r;
            }
        } else {
            self.g_target.copy_from_slice(&self.g_raw);
        }

        let mut sum_db = 0.0f32;
        for b in 0..BINS {
            let target = self.g_target[b];
            let coeff = if target < self.gain[b] {
                cfg.attack
            } else {
                cfg.release
            };
            let g = (coeff * self.gain[b] + (1.0 - coeff) * target).clamp(0.0, 1.0);
            self.gain[b] = g;
            let out_g = (1.0 - cfg.amount * (1.0 - g)).clamp(0.0, 1.0);
            self.freq[b] *= out_g;
            if b > 0 && b < FFT_SIZE / 2 {
                self.freq[FFT_SIZE - b] *= out_g;
            }
            sum_db += 20.0 * out_g.max(1e-6).log10();
        }

        inv.process_with_scratch(&mut self.freq, &mut self.scratch);
        let inv_n = 1.0 / FFT_SIZE as f32;
        for i in 0..FFT_SIZE {
            let v = self.freq[i].re * inv_n;
            if v.is_finite() {
                self.ola[i] += v;
            }
        }
        self.out_buf.copy_from_slice(&self.ola[..HOP]);
        self.ola.copy_within(HOP..FFT_SIZE, 0);
        self.ola[FFT_SIZE - HOP..].fill(0.0);
        self.gr_db = sum_db / BINS as f32;
    }
}

fn make_fft() -> (Arc<dyn Fft<f32>>, Arc<dyn Fft<f32>>, usize) {
    let mut planner = FftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(FFT_SIZE);
    let inv = planner.plan_fft_inverse(FFT_SIZE);
    let scratch = fwd
        .get_inplace_scratch_len()
        .max(inv.get_inplace_scratch_len());
    (fwd, inv, scratch)
}

// ---------------------------------------------------------------------------
// Noise reduction
// ---------------------------------------------------------------------------

/// STFT spectral subtraction / Wiener noise reduction with a minimum-statistics
/// noise profile. Reports one window of latency.
pub struct NoiseReduction {
    sample_rate: f32,
    fwd: Option<Arc<dyn Fft<f32>>>,
    inv: Option<Arc<dyn Fft<f32>>>,
    channels: [StftChannel; 2],
    ids: [u16; 4],
    gr_db: f32,
}

impl NoiseReduction {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            fwd: None,
            inv: None,
            channels: [StftChannel::new(), StftChannel::new()],
            ids: [u16::MAX; 4],
            gr_db: 0.0,
        }
    }
}

impl Default for NoiseReduction {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for NoiseReduction {
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
            id(params, "noise-floor"),
            id(params, "release"),
            id(params, "model"),
        ];
        let (fwd, inv, scratch) = make_fft();
        for ch in self.channels.iter_mut() {
            ch.alloc(fwd.clone(), inv.clone(), scratch);
        }
        self.fwd = Some(fwd);
        self.inv = Some(inv);
        self.gr_db = 0.0;
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() || self.fwd.is_none() {
            return;
        }
        let amount = (p.get_f32(self.ids[0]) / 100.0).clamp(0.0, 1.0);
        let floor = db_to_lin(p.get_f32(self.ids[1]).clamp(-100.0, -20.0));
        let release_ms = p.get_f32(self.ids[2]).clamp(1.0, 1000.0);
        let model = p.get_enum(self.ids[3]);
        let frame_rate = self.sample_rate / HOP as f32;
        let attack = (-1.0 / (frame_rate * 5.0 / 1000.0)).exp();
        let release = (-1.0 / (frame_rate * release_ms / 1000.0)).exp();
        let cfg = SpectralConfig {
            amount,
            floor,
            attack,
            release,
            model,
            oversub: 1.0 + 1.5 * amount,
            bias: 1.5,
            min_frames: 16,
            freq_smooth: true,
            tonal_protect: true,
        };
        let nch = buf.len().min(2);
        for (ch, channel) in self.channels.iter_mut().enumerate().take(nch) {
            for sample in buf[ch].iter_mut().take(frames) {
                *sample = channel.push(*sample, &cfg);
            }
        }
        self.gr_db = self.channels[0].gr_db;
    }

    fn latency_frames(&self) -> usize {
        FFT_SIZE
    }

    fn reset(&mut self) {
        for ch in self.channels.iter_mut() {
            ch.reset();
        }
        self.gr_db = 0.0;
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Deep noise remover
// ---------------------------------------------------------------------------

/// Model-free spectral gate that stands in for the upstream neural model.
///
/// Every `model` value runs the same Wiener-style spectral gate; the enum only
/// selects the smoothing constants (the bundled models have different latency
/// and aggressiveness profiles). Nothing is silently bypassed.
pub struct DeepNoiseRemover {
    sample_rate: f32,
    fwd: Option<Arc<dyn Fft<f32>>>,
    inv: Option<Arc<dyn Fft<f32>>>,
    channels: [StftChannel; 2],
    ids: [u16; 2],
    gr_db: f32,
}

impl DeepNoiseRemover {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            fwd: None,
            inv: None,
            channels: [StftChannel::new(), StftChannel::new()],
            ids: [u16::MAX; 2],
            gr_db: 0.0,
        }
    }
}

impl Default for DeepNoiseRemover {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for DeepNoiseRemover {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [id(params, "model"), id(params, "amount")];
        let (fwd, inv, scratch) = make_fft();
        for ch in self.channels.iter_mut() {
            ch.alloc(fwd.clone(), inv.clone(), scratch);
        }
        self.fwd = Some(fwd);
        self.inv = Some(inv);
        self.gr_db = 0.0;
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() || self.fwd.is_none() {
            return;
        }
        let model = p.get_enum(self.ids[0]);
        let amount = (p.get_f32(self.ids[1]) / 100.0).clamp(0.0, 1.0);
        // Same algorithm for all three models; only the time constants differ.
        let (attack_ms, release_ms, oversub) = match model {
            0 => (4.0, 60.0, 2.0),
            1 => (6.0, 80.0, 1.8),
            _ => (2.0, 40.0, 2.4),
        };
        let frame_rate = self.sample_rate / HOP as f32;
        let attack = (-1.0 / (frame_rate * attack_ms / 1000.0)).exp();
        let release = (-1.0 / (frame_rate * release_ms / 1000.0)).exp();
        let cfg = SpectralConfig {
            amount,
            floor: db_to_lin(-70.0),
            attack,
            release,
            model: 1,
            oversub,
            bias: 1.5,
            min_frames: 8,
            freq_smooth: true,
            tonal_protect: true,
        };
        let nch = buf.len().min(2);
        for (ch, channel) in self.channels.iter_mut().enumerate().take(nch) {
            for sample in buf[ch].iter_mut().take(frames) {
                *sample = channel.push(*sample, &cfg);
            }
        }
        self.gr_db = self.channels[0].gr_db;
    }

    fn latency_frames(&self) -> usize {
        FFT_SIZE
    }

    fn reset(&mut self) {
        for ch in self.channels.iter_mut() {
            ch.reset();
        }
        self.gr_db = 0.0;
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Echo canceller
// ---------------------------------------------------------------------------

/// NLMS adaptive FIR echo canceller.
///
/// In stereo the far-end reference is the opposite channel; in mono it is a
/// delayed copy of the input. The filter length is allocated for 500 ms in
/// `prepare` (capped at 32768 taps) and the active length can change at runtime
/// without allocating. Reports zero latency.
pub struct EchoCanceller {
    sample_rate: f32,
    ids: [u16; 3],
    max_taps: usize,
    taps: usize,
    w: [Vec<f32>; 2],
    hist: [Vec<f32>; 2],
    pos: [usize; 2],
    norm: [f32; 2],
    ref_delay: DelayLine,
    prepared: bool,
}

impl EchoCanceller {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            ids: [u16::MAX; 3],
            max_taps: 0,
            taps: 0,
            w: [Vec::new(), Vec::new()],
            hist: [Vec::new(), Vec::new()],
            pos: [0; 2],
            norm: [0.0; 2],
            ref_delay: DelayLine::new(64),
            prepared: false,
        }
    }

    fn reset_channel(&mut self, ch: usize) {
        let taps = self.taps;
        if taps > 0 {
            self.w[ch][..taps].fill(0.0);
            self.hist[ch][..taps].fill(0.0);
        }
        self.pos[ch] = 0;
        self.norm[ch] = 0.0;
    }

    fn reset_filter(&mut self) {
        for ch in 0..2 {
            self.reset_channel(ch);
        }
    }

    /// One NLMS step. `reference` is the far-end signal, `desired` the local
    /// signal that contains the echo.
    #[allow(clippy::needless_range_loop)]
    fn filter(&mut self, ch: usize, reference: f32, desired: f32, mu: f32, amount: f32) -> f32 {
        let taps = self.taps;
        if taps == 0 {
            return desired;
        }
        let pos = self.pos[ch];
        let hist = &mut self.hist[ch];
        let old = hist[pos];
        hist[pos] = reference;
        let norm = (self.norm[ch] + reference * reference - old * old).max(0.0);
        self.norm[ch] = norm;

        let w = &mut self.w[ch];
        let mut y = 0.0f32;
        let mut idx = pos;
        for k in 0..taps {
            y += w[k] * hist[idx];
            idx = if idx == 0 { taps - 1 } else { idx - 1 };
        }
        let e = desired - y;
        if !e.is_finite() || !norm.is_finite() {
            self.reset_channel(ch);
            return desired;
        }
        let step = mu * e / (norm + 1e-6);
        if step.is_finite() {
            let mut idx = pos;
            for k in 0..taps {
                w[k] = (w[k] + step * hist[idx]).clamp(-16.0, 16.0);
                idx = if idx == 0 { taps - 1 } else { idx - 1 };
            }
        }
        self.pos[ch] = if pos + 1 >= taps { 0 } else { pos + 1 };
        sanitize(desired - amount * y)
    }
}

impl Default for EchoCanceller {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for EchoCanceller {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [
            id(params, "filter-length"),
            id(params, "adaptation"),
            id(params, "amount"),
        ];
        // 500 ms at 48 kHz is 24000 taps; cap the allocation so 192 kHz does
        // not blow up memory or CPU.
        self.max_taps = ((self.sample_rate * 0.5) as usize).clamp(64, 32_768);
        for ch in 0..2 {
            self.w[ch] = vec![0.0; self.max_taps];
            self.hist[ch] = vec![0.0; self.max_taps];
        }
        self.ref_delay = DelayLine::new(self.max_taps + 8);
        self.taps = 0;
        self.prepared = true;
        Ok(())
    }

    #[allow(clippy::needless_range_loop)]
    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() || !self.prepared {
            return;
        }
        let filter_ms = p.get_f32(self.ids[0]).clamp(10.0, 500.0);
        let adaptation = (p.get_f32(self.ids[1]) / 100.0).clamp(0.0, 1.0);
        let amount = (p.get_f32(self.ids[2]) / 100.0).clamp(0.0, 1.0);
        let taps = ((self.sample_rate * filter_ms / 1000.0) as usize).clamp(16, self.max_taps);
        if taps != self.taps {
            self.taps = taps;
            self.reset_filter();
        }

        let nch = buf.len().min(2);
        if nch >= 2 {
            for i in 0..frames {
                let l = sanitize(buf[0][i]);
                let r = sanitize(buf[1][i]);
                let yl = self.filter(0, r, l, adaptation, amount);
                let yr = self.filter(1, l, r, adaptation, amount);
                buf[0][i] = yl;
                buf[1][i] = yr;
            }
        } else {
            for i in 0..frames {
                let x = sanitize(buf[0][i]);
                self.ref_delay.write(x);
                let reference = self.ref_delay.read(self.taps as f32);
                buf[0][i] = self.filter(0, reference, x, adaptation, amount);
            }
        }
        // Small leakage keeps the filter from drifting on silent input.
        for ch in 0..nch {
            for w in self.w[ch][..self.taps].iter_mut() {
                *w *= 0.99995;
            }
        }
    }

    fn reset(&mut self) {
        self.reset_filter();
        self.ref_delay.clear();
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

    fn set_f32(store: &ParamStore, key: &str, value: f32) {
        let (id, _) = store.spec_by_key(key).expect("parameter exists");
        store.set(id, ParamValue::F32(value));
    }

    fn set_enum(store: &ParamStore, key: &str, value: u16) {
        let (id, _) = store.spec_by_key(key).expect("parameter exists");
        store.set(id, ParamValue::Enum(value));
    }

    fn rms(x: &[f32]) -> f32 {
        if x.is_empty() {
            return 0.0;
        }
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    fn sine(freq: f32, sample_rate: f32, len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / sample_rate).sin())
            .collect()
    }

    fn noise(seed: &mut u32, len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|_| {
                *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = ((*seed >> 8) as f32 / 16_777_216.0) * 2.0 - 1.0;
                v * amp
            })
            .collect()
    }

    #[test]
    fn noise_reduction_attenuates_steady_noise() {
        let sr = 48_000u32;
        let params = store(EffectKind::NoiseReduction);
        set_f32(&params, "amount", 100.0);
        set_f32(&params, "noise-floor", -60.0);
        set_f32(&params, "release", 100.0);
        set_enum(&params, "model", 0);
        let mut fx = NoiseReduction::new();
        fx.prepare(sr, 512, 2, &params).unwrap();
        assert_eq!(fx.latency_frames(), FFT_SIZE);

        let n = 48_000;
        let mut seed = 0x1234_5678u32;
        let input = noise(&mut seed, n, 0.3);
        let mut buf = vec![input.clone(), input.clone()];
        fx.process(&mut buf, n, &params);

        let in_rms = rms(&input[n / 2..]);
        let out_rms = rms(&buf[0][n / 2..]);
        assert!(out_rms < in_rms * 0.5, "in={in_rms} out={out_rms}");
        assert!(buf[0].iter().all(|x| x.is_finite()));
        assert!(fx.gain_reduction_db() < 0.0);
    }

    #[test]
    fn noise_reduction_preserves_a_tone_after_noise_training() {
        let sr = 48_000u32;
        let params = store(EffectKind::NoiseReduction);
        set_f32(&params, "amount", 100.0);
        set_f32(&params, "noise-floor", -60.0);
        set_f32(&params, "release", 100.0);
        let mut fx = NoiseReduction::new();
        fx.prepare(sr, 512, 2, &params).unwrap();

        let train = 24_000;
        let tone_len = 14_400;
        let mut seed = 0x9e37_79b9u32;
        let mut input = noise(&mut seed, train, 0.2);
        input.extend_from_slice(&sine(440.0, sr as f32, tone_len, 0.5));
        let mut buf = vec![input.clone(), input.clone()];
        fx.process(&mut buf, input.len(), &params);

        let tone_in = rms(&input[train..train + 4_800]);
        let tone_out = rms(&buf[0][train..train + 4_800]);
        assert!(
            tone_out > tone_in * 0.5,
            "tone lost: in={tone_in} out={tone_out}"
        );
    }

    #[test]
    fn noise_reduction_preserves_a_steady_tone() {
        // A steady tone is a strong spectral peak; the tonal protection keeps
        // it while broadband noise is removed.
        let sr = 48_000u32;
        let params = store(EffectKind::NoiseReduction);
        set_f32(&params, "amount", 100.0);
        set_f32(&params, "noise-floor", -60.0);
        set_f32(&params, "release", 100.0);
        let mut fx = NoiseReduction::new();
        fx.prepare(sr, 512, 2, &params).unwrap();

        let n = 24_000;
        let input = sine(440.0, sr as f32, n, 0.5);
        let mut buf = vec![input.clone(), input.clone()];
        fx.process(&mut buf, n, &params);

        let tone_in = rms(&input[n / 2..]);
        let tone_out = rms(&buf[0][n / 2..]);
        assert!(
            tone_out > tone_in * 0.5,
            "tone lost: in={tone_in} out={tone_out}"
        );
    }

    #[test]
    fn noise_reduction_both_models_attenuate_noise() {
        let sr = 48_000u32;
        let params = store(EffectKind::NoiseReduction);
        set_f32(&params, "amount", 100.0);
        set_f32(&params, "noise-floor", -60.0);
        set_f32(&params, "release", 100.0);
        for model in 0..2u16 {
            set_enum(&params, "model", model);
            let mut fx = NoiseReduction::new();
            fx.prepare(sr, 512, 2, &params).unwrap();
            let n = 24_000;
            let mut seed = 0x51ed_270bu32.wrapping_add(model as u32);
            let input = noise(&mut seed, n, 0.3);
            let mut buf = vec![input.clone(), input.clone()];
            fx.process(&mut buf, n, &params);
            let in_rms = rms(&input[n / 2..]);
            let out_rms = rms(&buf[0][n / 2..]);
            assert!(
                out_rms < in_rms * 0.5,
                "model {model}: in={in_rms} out={out_rms}"
            );
        }
    }

    #[test]
    fn noise_reduction_survives_non_finite_input() {
        let sr = 48_000u32;
        let params = store(EffectKind::NoiseReduction);
        let mut fx = NoiseReduction::new();
        fx.prepare(sr, 512, 2, &params).unwrap();
        let mut buf = vec![vec![f32::NAN; 4_096], vec![0.0; 4_096]];
        fx.process(&mut buf, 4_096, &params);
        assert_eq!(buf[0].len(), 4_096);
        assert!(buf[0].iter().all(|x| x.is_finite()));
    }

    #[test]
    fn deep_noise_remover_all_models_attenuate() {
        let sr = 48_000u32;
        let params = store(EffectKind::DeepNoiseRemover);
        set_f32(&params, "amount", 100.0);
        for model in 0..3u16 {
            set_enum(&params, "model", model);
            let mut fx = DeepNoiseRemover::new();
            fx.prepare(sr, 512, 2, &params).unwrap();
            assert_eq!(fx.latency_frames(), FFT_SIZE);

            let n = 24_000;
            let mut seed = 0xdead_beefu32.wrapping_add(model as u32);
            let input = noise(&mut seed, n, 0.3);
            let mut buf = vec![input.clone(), input.clone()];
            fx.process(&mut buf, n, &params);

            let in_rms = rms(&input[n / 2..]);
            let out_rms = rms(&buf[0][n / 2..]);
            assert!(
                out_rms < in_rms * 0.7,
                "model {model}: in={in_rms} out={out_rms}"
            );
        }
    }

    #[test]
    fn echo_canceller_reduces_correlated_echo() {
        let sr = 48_000u32;
        let params = store(EffectKind::EchoCanceller);
        set_f32(&params, "filter-length", 10.0);
        set_f32(&params, "adaptation", 100.0);
        set_f32(&params, "amount", 100.0);
        let mut fx = EchoCanceller::new();
        fx.prepare(sr, 512, 2, &params).unwrap();
        assert_eq!(fx.latency_frames(), 0);

        let n = 12_000;
        let mut seed = 0x0bad_f00du32;
        let l = noise(&mut seed, n, 0.3);
        let delay = 64usize;
        let mut r = vec![0.0f32; n];
        for i in delay..n {
            r[i] = 0.5 * l[i - delay];
        }
        let r_in = rms(&r[n / 2..]);
        let mut buf = vec![l, r];
        fx.process(&mut buf, n, &params);
        let r_out = rms(&buf[1][n / 2..]);
        assert!(
            r_out < r_in * 0.5,
            "echo not cancelled: in={r_in} out={r_out}"
        );
        assert!(buf[1].iter().all(|x| x.is_finite()));
    }

    #[test]
    fn echo_canceller_amount_zero_is_transparent_and_mono_is_stable() {
        let sr = 48_000u32;
        let params = store(EffectKind::EchoCanceller);
        set_f32(&params, "filter-length", 10.0);
        set_f32(&params, "adaptation", 100.0);
        set_f32(&params, "amount", 0.0);
        let mut fx = EchoCanceller::new();
        fx.prepare(sr, 512, 2, &params).unwrap();

        let n = 4_096;
        let mut seed = 0x00c0_ffeeu32;
        let input = noise(&mut seed, n, 0.3);
        let mut buf = vec![input.clone(), input.clone()];
        fx.process(&mut buf, n, &params);
        for i in 0..n {
            assert!((buf[0][i] - input[i]).abs() < 1e-6);
        }

        fx.reset();
        let mut mono = vec![input];
        fx.process(&mut mono, n, &params);
        assert!(mono[0].iter().all(|x| x.is_finite()));
        assert!(rms(&mono[0]) < 10.0);
    }
}
