//! Space & time effects: partitioned convolution, FDN reverb, stereo delay,
//! Bauer crossfeed, crosstalk cancellation and mid/side stereo tools.

// DSP loops index several buffers with the same index; iterator rewrites would
// obscure the sample/partition arithmetic.
#![allow(clippy::needless_range_loop)]

use std::sync::Arc;

use dsp_core::biquad::{Biquad, BiquadKind};
use dsp_core::delay::{DelayLine, StereoDelay};
use dsp_core::param::{ParamStore, ParamValue};
use dsp_core::smoothing::{db_to_lin, Smoothed};
use dsp_core::{AudioEffect, EffectError};
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

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

/// Flush denormals in feedback paths (they are a real CPU hazard on x86).
#[inline]
fn flush(x: f32) -> f32 {
    if x.abs() < 1e-15 {
        0.0
    } else {
        x
    }
}

// ---------------------------------------------------------------------------
// Convolver
// ---------------------------------------------------------------------------

const CONV_BLOCKS: [usize; 5] = [256, 512, 1024, 2048, 4096];

/// Uniform partitioned FFT convolution with a frequency-domain delay line.
///
/// The IR is split into `K` partitions of `P` samples; each block of `P` input
/// samples is transformed once and multiplied with all `K` IR partitions. The
/// block buffering introduces exactly `P` samples of latency.
pub struct Convolver {
    sample_rate: f32,
    channels: usize,
    partition: usize,
    fft: Option<Arc<dyn Fft<f32>>>,
    ifft: Option<Arc<dyn Fft<f32>>>,
    fft_scratch: Vec<Complex<f32>>,
    ifft_scratch: Vec<Complex<f32>>,
    time: Vec<Complex<f32>>,
    acc: Vec<Complex<f32>>,
    ir_raw: Vec<Vec<f32>>,
    ir_rate: u32,
    ir_l: Vec<Complex<f32>>,
    ir_r: Vec<Complex<f32>>,
    k_full: usize,
    k_active: usize,
    fdl: Vec<Vec<Complex<f32>>>,
    fdl_pos: usize,
    in_hist: Vec<Vec<f32>>,
    out_block: Vec<Vec<f32>>,
    fill: usize,
    out_pos: usize,
    width: f32,
    autogain_gain: f32,
    autogain_on: bool,
    autogain: Smoothed,
    ids: [u16; 4],
    has_ir: bool,
}

impl Convolver {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            channels: 2,
            partition: 1024,
            fft: None,
            ifft: None,
            fft_scratch: Vec::new(),
            ifft_scratch: Vec::new(),
            time: Vec::new(),
            acc: Vec::new(),
            ir_raw: Vec::new(),
            ir_rate: 48_000,
            ir_l: Vec::new(),
            ir_r: Vec::new(),
            k_full: 0,
            k_active: 0,
            fdl: Vec::new(),
            fdl_pos: 0,
            in_hist: Vec::new(),
            out_block: Vec::new(),
            fill: 0,
            out_pos: 0,
            width: 1.0,
            autogain_gain: 1.0,
            autogain_on: true,
            autogain: Smoothed::new(1.0),
            ids: [u16::MAX; 4],
            has_ir: false,
        }
    }

    /// Load an impulse response. `sample_rate` is the rate of `ir`; it is
    /// resampled to the engine rate when they differ. Control thread only.
    pub fn load_ir(&mut self, ir: &[Vec<f32>], sample_rate: u32) -> Result<(), EffectError> {
        if ir.is_empty() || ir.iter().any(|c| c.is_empty()) {
            return Err(EffectError::InvalidConfig(
                "impulse response has no samples".into(),
            ));
        }
        if sample_rate == 0 {
            return Err(EffectError::InvalidConfig(
                "impulse response sample rate is zero".into(),
            ));
        }
        let chans = ir.len().min(2);
        self.ir_raw = ir[..chans].iter().map(|c| c.to_vec()).collect();
        self.ir_rate = sample_rate;
        self.build()?;
        self.autogain.snap(if self.autogain_on {
            self.autogain_gain
        } else {
            1.0
        });
        Ok(())
    }

    pub fn has_ir(&self) -> bool {
        self.has_ir
    }

    fn make_plan(&mut self) {
        let n = 2 * self.partition;
        if self.fft.is_some() && self.time.len() == n {
            return;
        }
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        self.fft_scratch = vec![Complex::new(0.0, 0.0); fft.get_inplace_scratch_len()];
        self.ifft_scratch = vec![Complex::new(0.0, 0.0); ifft.get_inplace_scratch_len()];
        self.time = vec![Complex::new(0.0, 0.0); n];
        self.acc = vec![Complex::new(0.0, 0.0); n];
        self.fft = Some(fft);
        self.ifft = Some(ifft);
    }

    fn clear_ir(&mut self) {
        self.ir_l.clear();
        self.ir_r.clear();
        self.fdl.clear();
        self.in_hist.clear();
        self.out_block.clear();
        self.k_full = 0;
        self.k_active = 0;
        self.fdl_pos = 0;
        self.fill = 0;
        self.out_pos = 0;
        self.has_ir = false;
    }

    /// Re-partition the stored IR. Allocates, so it is only called from
    /// `prepare` / `load_ir` (control thread).
    fn build(&mut self) -> Result<(), EffectError> {
        self.clear_ir();
        if self.ir_raw.is_empty() {
            return Ok(());
        }
        self.make_plan();
        let p = self.partition;
        let n = 2 * p;
        let sr = self.sample_rate.max(1.0);
        let ratio = sr / self.ir_rate.max(1) as f32;

        let mut ir: Vec<Vec<f32>> = Vec::with_capacity(self.ir_raw.len());
        for ch in &self.ir_raw {
            let out_len = ((ch.len() as f32) * ratio).round().max(1.0) as usize;
            let mut out = vec![0.0f32; out_len];
            for (i, o) in out.iter_mut().enumerate() {
                let pos = i as f32 / ratio;
                let i0 = pos.floor() as usize;
                let frac = pos - i0 as f32;
                let a = ch[i0.min(ch.len() - 1)];
                let b = ch[(i0 + 1).min(ch.len() - 1)];
                *o = sanitize(a + (b - a) * frac);
            }
            ir.push(out);
        }
        let ir_len = ir[0].len();
        if ir_len == 0 {
            return Ok(());
        }
        self.autogain_gain = compute_autogain(&ir);

        let k = ir_len.div_ceil(p);
        let mut ir_l = vec![Complex::new(0.0, 0.0); k * n];
        let mut ir_r = if ir.len() > 1 {
            Some(vec![Complex::new(0.0, 0.0); k * n])
        } else {
            None
        };
        let fft = self.fft.as_ref().expect("fft plan");
        for kk in 0..k {
            for i in 0..n {
                let idx = kk * p + i;
                let v = if i < p && idx < ir_len {
                    ir[0][idx]
                } else {
                    0.0
                };
                self.time[i] = Complex::new(v, 0.0);
            }
            fft.process_with_scratch(&mut self.time, &mut self.fft_scratch);
            ir_l[kk * n..(kk + 1) * n].copy_from_slice(&self.time);
            if let Some(r) = ir.get(1) {
                for i in 0..n {
                    let idx = kk * p + i;
                    let v = if i < p && idx < r.len() { r[idx] } else { 0.0 };
                    self.time[i] = Complex::new(v, 0.0);
                }
                fft.process_with_scratch(&mut self.time, &mut self.fft_scratch);
                if let Some(dst) = ir_r.as_mut() {
                    dst[kk * n..(kk + 1) * n].copy_from_slice(&self.time);
                }
            }
        }
        self.ir_l = ir_l;
        self.ir_r = ir_r.unwrap_or_default();
        self.k_full = k;
        self.k_active = k;

        let chans = self.channels.clamp(1, 2);
        self.fdl = (0..chans)
            .map(|_| vec![Complex::new(0.0, 0.0); k * n])
            .collect();
        self.in_hist = (0..chans).map(|_| vec![0.0; n]).collect();
        self.out_block = (0..chans).map(|_| vec![0.0; p]).collect();
        self.fill = 0;
        self.out_pos = 0;
        self.fdl_pos = 0;
        self.has_ir = true;
        Ok(())
    }

    fn process_block(&mut self, ch: usize, slot: usize, mono: bool) {
        let p = self.partition;
        let n = 2 * p;
        let k = self.k_full;
        if k == 0 {
            return;
        }
        for (t, &x) in self.time.iter_mut().zip(self.in_hist[ch].iter()) {
            *t = Complex::new(x, 0.0);
        }
        let fft = self.fft.as_ref().expect("fft plan");
        fft.process_with_scratch(&mut self.time, &mut self.fft_scratch);
        let base = slot * n;
        self.fdl[ch][base..base + n].copy_from_slice(&self.time[..n]);
        self.acc[..n].fill(Complex::new(0.0, 0.0));

        let stereo_ir = !self.ir_r.is_empty();
        let w = self.width;
        let blend = stereo_ir && (w - 1.0).abs() > 1e-4;
        let a = (1.0 + w) * 0.5;
        let b = (1.0 - w) * 0.5;
        for kk in 0..self.k_active {
            let s = (slot + k - kk) % k;
            let xb = &self.fdl[ch][s * n..s * n + n];
            let lb = &self.ir_l[kk * n..kk * n + n];
            if mono {
                if stereo_ir {
                    let rb = &self.ir_r[kk * n..kk * n + n];
                    for i in 0..n {
                        let h =
                            Complex::new(0.5 * (lb[i].re + rb[i].re), 0.5 * (lb[i].im + rb[i].im));
                        self.acc[i] += xb[i] * h;
                    }
                } else {
                    for i in 0..n {
                        self.acc[i] += xb[i] * lb[i];
                    }
                }
            } else if ch == 0 {
                if blend {
                    let rb = &self.ir_r[kk * n..kk * n + n];
                    for i in 0..n {
                        let h =
                            Complex::new(a * lb[i].re + b * rb[i].re, a * lb[i].im + b * rb[i].im);
                        self.acc[i] += xb[i] * h;
                    }
                } else {
                    for i in 0..n {
                        self.acc[i] += xb[i] * lb[i];
                    }
                }
            } else if blend {
                let rb = &self.ir_r[kk * n..kk * n + n];
                for i in 0..n {
                    let h = Complex::new(b * lb[i].re + a * rb[i].re, b * lb[i].im + a * rb[i].im);
                    self.acc[i] += xb[i] * h;
                }
            } else if stereo_ir {
                let rb = &self.ir_r[kk * n..kk * n + n];
                for i in 0..n {
                    self.acc[i] += xb[i] * rb[i];
                }
            } else {
                for i in 0..n {
                    self.acc[i] += xb[i] * lb[i];
                }
            }
        }
        let ifft = self.ifft.as_ref().expect("fft plan");
        ifft.process_with_scratch(&mut self.acc, &mut self.ifft_scratch);
        let norm = 1.0 / n as f32;
        for (o, c) in self.out_block[ch].iter_mut().zip(self.acc[p..].iter()) {
            *o = c.re * norm;
        }
    }
}

impl Default for Convolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Peak magnitude of the IR spectrum, inverted so the response peaks at 0 dB.
fn compute_autogain(ir: &[Vec<f32>]) -> f32 {
    let len = ir[0].len();
    let n = len.next_power_of_two().max(2);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf = vec![Complex::new(0.0, 0.0); n];
    let mut scratch = vec![Complex::new(0.0, 0.0); fft.get_inplace_scratch_len()];
    let inv = 1.0 / ir.len() as f32;
    for (i, c) in buf.iter_mut().take(len).enumerate() {
        let mut s = 0.0;
        for ch in ir {
            s += ch.get(i).copied().unwrap_or(0.0);
        }
        *c = Complex::new(s * inv, 0.0);
    }
    fft.process_with_scratch(&mut buf, &mut scratch);
    let mut peak = 0.0f32;
    for c in &buf[..n / 2] {
        peak = peak.max(c.norm());
    }
    if peak > 1e-9 {
        (1.0 / peak).clamp(1e-3, 1e3)
    } else {
        1.0
    }
}

impl Convolver {
    /// Read a WAV/IRS impulse response and load it. Called on the control
    /// thread; the audio thread only sees the prepared partitions.
    pub fn load_ir_file(&mut self, path: &std::path::Path) -> Result<(), EffectError> {
        let mut reader = hound::WavReader::open(path)
            .map_err(|e| EffectError::Asset(format!("{}: {e}", path.display())))?;
        let spec = reader.spec();
        let channels = spec.channels as usize;
        if channels == 0 || channels > 2 {
            return Err(EffectError::Asset(format!(
                "{}: expected 1 or 2 channels, got {channels}",
                path.display()
            )));
        }
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| EffectError::Asset(e.to_string()))?,
            hound::SampleFormat::Int => {
                let max = (1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
                reader
                    .samples::<i32>()
                    .map(|s| s.map(|v| v as f32 / max))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| EffectError::Asset(e.to_string()))?
            }
        };
        let frames = samples.len() / channels;
        if frames == 0 {
            return Err(EffectError::Asset(format!("{}: empty IR", path.display())));
        }
        if frames > 4_000_000 {
            return Err(EffectError::Asset(format!(
                "{}: IR is too long ({frames} frames)",
                path.display()
            )));
        }
        let mut ir = vec![vec![0.0f32; frames]; channels];
        for (i, sample) in samples.iter().enumerate() {
            ir[i % channels][i / channels] = *sample;
        }
        self.load_ir(&ir, spec.sample_rate)
    }
}

impl AudioEffect for Convolver {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate.max(1) as f32;
        self.channels = channels.clamp(1, 2);
        self.ids = [
            id(params, "autogain"),
            id(params, "blocksize"),
            id(params, "tailsize"),
            id(params, "ir-width"),
        ];
        let bs = params.get_enum(self.ids[1]) as usize;
        self.partition = CONV_BLOCKS[bs.min(CONV_BLOCKS.len() - 1)];
        self.make_plan();
        self.autogain.set_time_ms(self.sample_rate, 20.0);
        self.autogain_on = params.get_bool(self.ids[0]);
        self.build()?;
        self.autogain.snap(if self.autogain_on {
            self.autogain_gain
        } else {
            1.0
        });
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if !self.has_ir || buf.is_empty() {
            return;
        }
        let frames = frames.min(buf[0].len());
        if frames == 0 {
            return;
        }
        let autogain = p.get_bool(self.ids[0]);
        let tailsize = (p.get_f32(self.ids[2]) / 100.0).clamp(0.0, 1.0);
        self.width = (p.get_f32(self.ids[3]) / 100.0).clamp(0.0, 1.0);
        self.k_active =
            ((self.k_full as f32 * (1.0 - tailsize)).ceil() as usize).clamp(1, self.k_full);
        self.autogain
            .set_target(if autogain { self.autogain_gain } else { 1.0 });

        let chans = buf.len().min(self.fdl.len());
        let mono = chans == 1;
        let p_size = self.partition;
        for i in 0..frames {
            let g = self.autogain.next();
            for ch in 0..chans {
                let x = sanitize(buf[ch][i]);
                let y = self.out_block[ch][self.out_pos];
                self.in_hist[ch][p_size + self.fill] = x;
                buf[ch][i] = y * g;
            }
            self.fill += 1;
            self.out_pos += 1;
            if self.out_pos >= p_size {
                self.out_pos = 0;
            }
            if self.fill >= p_size {
                let slot = self.fdl_pos;
                for ch in 0..chans {
                    self.process_block(ch, slot, mono);
                }
                self.fdl_pos = (slot + 1) % self.k_full;
                for h in self.in_hist.iter_mut() {
                    h.copy_within(p_size..2 * p_size, 0);
                }
                self.fill = 0;
            }
        }
    }

    fn latency_frames(&self) -> usize {
        if self.has_ir {
            self.partition
        } else {
            0
        }
    }

    fn load_asset(&mut self, key: &str, path: &std::path::Path) -> Result<(), EffectError> {
        if key == "kernel-name" || key == "kernel-path" {
            self.load_ir_file(path)
        } else {
            Err(EffectError::Unsupported(format!(
                "convolver has no asset \"{key}\""
            )))
        }
    }

    fn reset(&mut self) {
        for h in self.in_hist.iter_mut() {
            h.fill(0.0);
        }
        for o in self.out_block.iter_mut() {
            o.fill(0.0);
        }
        for f in self.fdl.iter_mut() {
            f.fill(Complex::new(0.0, 0.0));
        }
        self.fill = 0;
        self.out_pos = 0;
        self.fdl_pos = 0;
        self.autogain.snap(if self.has_ir && self.autogain_on {
            self.autogain_gain
        } else {
            1.0
        });
    }

    fn release(&mut self) {
        self.ir_raw.clear();
        self.clear_ir();
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Reverberation
// ---------------------------------------------------------------------------

const FDN_LINES: usize = 8;
const FDN_BASE_MS: [f32; FDN_LINES] = [29.7, 37.1, 41.1, 43.7, 47.9, 53.3, 59.9, 67.7];
const ER_TAPS_L: [f32; 3] = [7.1, 13.7, 21.3];
const ER_TAPS_R: [f32; 3] = [9.3, 15.1, 23.9];

/// Schroeder allpass used for input diffusion.
struct Allpass {
    line: DelayLine,
    delay: usize,
    g: f32,
}

impl Allpass {
    fn new(max_samples: usize) -> Self {
        Self {
            line: DelayLine::new(max_samples),
            delay: 1,
            g: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let d = self.line.read_int(self.delay);
        let v = x + self.g * d;
        self.line.write(v);
        d - self.g * v
    }

    fn reset(&mut self) {
        self.line.clear();
    }
}

/// Fast Walsh-Hadamard transform of eight values (unnormalised).
fn fht8(v: &mut [f32; FDN_LINES]) {
    let mut h = 1;
    while h < FDN_LINES {
        let mut i = 0;
        while i < FDN_LINES {
            for j in i..i + h {
                let a = v[j];
                let b = v[j + h];
                v[j] = a + b;
                v[j + h] = a - b;
            }
            i += h * 2;
        }
        h *= 2;
    }
}

/// Eight-line feedback delay network with Hadamard mixing and damping.
pub struct Reverberation {
    sample_rate: f32,
    lines: [DelayLine; FDN_LINES],
    pre: DelayLine,
    ap: [Allpass; 2],
    lp_state: [f32; FDN_LINES],
    dc_x: [f32; FDN_LINES],
    dc_y: [f32; FDN_LINES],
    delay_samples: [Smoothed; FDN_LINES],
    gain: [Smoothed; FDN_LINES],
    damp: Smoothed,
    width: Smoothed,
    tail: Smoothed,
    pre_delay: Smoothed,
    er_l: [f32; 3],
    er_r: [f32; 3],
    ids: [u16; 7],
}

impl Reverberation {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            lines: std::array::from_fn(|_| DelayLine::new(1)),
            pre: DelayLine::new(1),
            ap: [Allpass::new(1), Allpass::new(1)],
            lp_state: [0.0; FDN_LINES],
            dc_x: [0.0; FDN_LINES],
            dc_y: [0.0; FDN_LINES],
            delay_samples: [Smoothed::new(1.0); FDN_LINES],
            gain: [Smoothed::new(0.0); FDN_LINES],
            damp: Smoothed::new(0.0),
            width: Smoothed::new(1.0),
            tail: Smoothed::new(1.0),
            pre_delay: Smoothed::new(1.0),
            er_l: [1.0; 3],
            er_r: [1.0; 3],
            ids: [u16::MAX; 7],
        }
    }

    fn configure(&mut self, p: &ParamStore, snap: bool) {
        let sr = self.sample_rate;
        let room = (p.get_f32(self.ids[0]) / 100.0).clamp(0.0, 1.0);
        let damping = (p.get_f32(self.ids[1]) / 100.0).clamp(0.0, 1.0);
        let width = (p.get_f32(self.ids[2]) / 100.0).clamp(0.0, 1.0);
        let pre_ms = p.get_f32(self.ids[3]).clamp(0.0, 200.0);
        let rt60 = p.get_f32(self.ids[4]).clamp(0.1, 10.0);
        let diffusion = (p.get_f32(self.ids[5]) / 100.0).clamp(0.0, 1.0);
        let tail = p.get_bool(self.ids[6]);
        let scale = 0.5 + room;

        let pre_samples = (pre_ms * sr / 1000.0).clamp(1.0, self.pre.capacity() as f32 - 4.0);
        let damp = damping * 0.85;
        let tail_gain = if tail { 1.0 } else { 0.0 };
        if snap {
            self.pre_delay.snap(pre_samples);
            self.damp.snap(damp);
            self.width.snap(width);
            self.tail.snap(tail_gain);
        } else {
            self.pre_delay.set_target(pre_samples);
            self.damp.set_target(damp);
            self.width.set_target(width);
            self.tail.set_target(tail_gain);
        }

        let ap_g = diffusion * 0.7;
        self.ap[0].g = ap_g;
        self.ap[1].g = ap_g;
        self.ap[0].delay =
            ((12.5 * scale * sr / 1000.0) as usize).clamp(1, self.ap[0].line.capacity() - 2);
        self.ap[1].delay =
            ((7.3 * scale * sr / 1000.0) as usize).clamp(1, self.ap[1].line.capacity() - 2);

        for j in 0..FDN_LINES {
            let ms = FDN_BASE_MS[j] * scale;
            let samples = (ms * sr / 1000.0).clamp(1.0, self.lines[j].capacity() as f32 - 4.0);
            let g = 10f32.powf(-3.0 * (samples / sr) / rt60).clamp(0.0, 0.9995);
            if snap {
                self.delay_samples[j].snap(samples);
                self.gain[j].snap(g);
            } else {
                self.delay_samples[j].set_target(samples);
                self.gain[j].set_target(g);
            }
        }
        for k in 0..3 {
            self.er_l[k] = (ER_TAPS_L[k] * scale * sr / 1000.0).max(1.0);
            self.er_r[k] = (ER_TAPS_R[k] * scale * sr / 1000.0).max(1.0);
        }
    }
}

impl Default for Reverberation {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for Reverberation {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate.max(1) as f32;
        self.ids = [
            id(params, "room-size"),
            id(params, "damping"),
            id(params, "width"),
            id(params, "pre-delay"),
            id(params, "decay-time"),
            id(params, "diffusion"),
            id(params, "tail"),
        ];
        let sr = self.sample_rate;
        let max_ms = FDN_BASE_MS.iter().copied().fold(0.0f32, f32::max) * 1.5;
        let max_samples = (max_ms * sr / 1000.0) as usize + 8;
        self.lines = std::array::from_fn(|_| DelayLine::new(max_samples));
        self.pre = DelayLine::new((0.25 * sr) as usize + 8);
        let ap_max = (18.75 * sr / 1000.0) as usize + 8;
        self.ap = [Allpass::new(ap_max), Allpass::new(ap_max)];
        for s in self.delay_samples.iter_mut() {
            s.set_time_ms(sr, 50.0);
        }
        for s in self.gain.iter_mut() {
            s.set_time_ms(sr, 50.0);
        }
        self.damp.set_time_ms(sr, 20.0);
        self.width.set_time_ms(sr, 20.0);
        self.tail.set_time_ms(sr, 10.0);
        self.pre_delay.set_time_ms(sr, 50.0);
        self.reset();
        self.configure(params, true);
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
        self.configure(p, false);
        let chans = buf.len().min(2);
        let norm = 0.353_553_4; // 1/sqrt(8)
        for i in 0..frames {
            let x = if chans == 2 {
                0.5 * (sanitize(buf[0][i]) + sanitize(buf[1][i]))
            } else {
                sanitize(buf[0][i])
            };
            self.pre.write(x);
            let pd = self.pre_delay.next();
            let mut er_l = 0.0;
            let mut er_r = 0.0;
            for k in 0..3 {
                er_l += self.pre.read(pd + self.er_l[k]);
                er_r += self.pre.read(pd + self.er_r[k]);
            }
            let er_l = er_l * 0.22;
            let er_r = er_r * 0.22;

            let mut diffused = self.ap[0].process(self.pre.read(pd));
            diffused = self.ap[1].process(diffused);

            let mut d = [0.0f32; FDN_LINES];
            for j in 0..FDN_LINES {
                d[j] = self.lines[j].read(self.delay_samples[j].next());
            }
            let damp = self.damp.next();
            for j in 0..FDN_LINES {
                let dc = d[j] - self.dc_x[j] + 0.995 * self.dc_y[j];
                self.dc_x[j] = d[j];
                self.dc_y[j] = flush(dc);
                let lp = self.lp_state[j] + (1.0 - damp) * (self.dc_y[j] - self.lp_state[j]);
                self.lp_state[j] = flush(lp);
                d[j] = self.lp_state[j];
            }

            let mut v = d;
            fht8(&mut v);
            for j in 0..FDN_LINES {
                let sign = if j & 1 == 0 { 1.0 } else { -1.0 };
                let fb = v[j] * norm * self.gain[j].next();
                self.lines[j].write(flush(diffused * sign + fb));
            }

            let mut wl = 0.0;
            let mut wr = 0.0;
            for j in 0..FDN_LINES {
                let s = if j & 1 == 0 { 1.0 } else { -1.0 };
                wl += d[j] * s;
                wr += d[j] * if j & 1 == 0 { 0.6 } else { -1.0 };
            }
            let mid = 0.5 * (wl + wr) * 0.35;
            let side = 0.5 * (wl - wr) * 0.35;
            let w = self.width.next();
            let tg = self.tail.next();
            let out_l = er_l + (mid + w * side) * tg;
            let out_r = er_r + (mid - w * side) * tg;
            if chans == 2 {
                buf[0][i] = out_l;
                buf[1][i] = out_r;
            } else {
                buf[0][i] = 0.5 * (out_l + out_r);
            }
        }
    }

    fn reset(&mut self) {
        for l in self.lines.iter_mut() {
            l.clear();
        }
        self.pre.clear();
        for a in self.ap.iter_mut() {
            a.reset();
        }
        self.lp_state = [0.0; FDN_LINES];
        self.dc_x = [0.0; FDN_LINES];
        self.dc_y = [0.0; FDN_LINES];
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Delay
// ---------------------------------------------------------------------------

/// Stereo delay with smoothed read positions and optional ping-pong routing.
/// The output is 100% wet; the graph applies the common dry/wet mix.
pub struct Delay {
    sample_rate: f32,
    lines: StereoDelay,
    ids: [u16; 4],
    time_l: Smoothed,
    time_r: Smoothed,
    feedback: Smoothed,
}

impl Delay {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            lines: StereoDelay::new(1),
            ids: [u16::MAX; 4],
            time_l: Smoothed::new(1.0),
            time_r: Smoothed::new(1.0),
            feedback: Smoothed::new(0.0),
        }
    }
}

impl Default for Delay {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for Delay {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate.max(1) as f32;
        self.ids = [
            id(params, "time-l"),
            id(params, "time-r"),
            id(params, "feedback"),
            id(params, "ping-pong"),
        ];
        let sr = self.sample_rate;
        self.lines = StereoDelay::new((2.0 * sr) as usize + 8);
        self.time_l.set_time_ms(sr, 50.0);
        self.time_r.set_time_ms(sr, 50.0);
        self.feedback.set_time_ms(sr, 20.0);
        let cap = self.lines.l.capacity() as f32 - 4.0;
        self.time_l
            .snap((params.get_f32(self.ids[0]) * sr / 1000.0).clamp(1.0, cap));
        self.time_r
            .snap((params.get_f32(self.ids[1]) * sr / 1000.0).clamp(1.0, cap));
        self.feedback
            .snap((params.get_f32(self.ids[2]) / 100.0).clamp(0.0, 0.95));
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
        let sr = self.sample_rate;
        let cap = self.lines.l.capacity() as f32 - 4.0;
        self.time_l
            .set_target((p.get_f32(self.ids[0]) * sr / 1000.0).clamp(1.0, cap));
        self.time_r
            .set_target((p.get_f32(self.ids[1]) * sr / 1000.0).clamp(1.0, cap));
        self.feedback
            .set_target((p.get_f32(self.ids[2]) / 100.0).clamp(0.0, 0.95));
        let ping = p.get_bool(self.ids[3]);
        let chans = buf.len().min(2);
        for i in 0..frames {
            let dl = self.time_l.next();
            let dr = self.time_r.next();
            let f = self.feedback.next();
            if chans == 2 {
                let xl = sanitize(buf[0][i]);
                let xr = sanitize(buf[1][i]);
                let yl = self.lines.l.read(dl);
                let yr = self.lines.r.read(dr);
                if ping {
                    self.lines.l.write(flush(xl + f * yr));
                    self.lines.r.write(flush(xr + f * yl));
                } else {
                    self.lines.l.write(flush(xl + f * yl));
                    self.lines.r.write(flush(xr + f * yr));
                }
                buf[0][i] = yl;
                buf[1][i] = yr;
            } else {
                let x = sanitize(buf[0][i]);
                let y = self.lines.l.read(dl);
                self.lines.l.write(flush(x + f * y));
                buf[0][i] = y;
            }
        }
    }

    fn reset(&mut self) {
        self.lines.clear();
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Crossfeed
// ---------------------------------------------------------------------------

/// Bauer-style crossfeed: the opposite channel is low-passed, delayed by the
/// interaural time difference and mixed in at the feed level.
pub struct Crossfeed {
    sample_rate: f32,
    lp: [Biquad; 2],
    delay: StereoDelay,
    delay_samples: f32,
    ids: [u16; 3],
    feed: Smoothed,
    level: Smoothed,
    last_fcut: f32,
}

impl Crossfeed {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            lp: [Biquad::new(), Biquad::new()],
            delay: StereoDelay::new(1),
            delay_samples: 14.0,
            ids: [u16::MAX; 3],
            feed: Smoothed::new(0.0),
            level: Smoothed::new(1.0),
            last_fcut: -1.0,
        }
    }
}

impl Default for Crossfeed {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for Crossfeed {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate.max(1) as f32;
        self.ids = [id(params, "fcut"), id(params, "feed"), id(params, "level")];
        let sr = self.sample_rate;
        self.delay = StereoDelay::new((0.001 * sr) as usize + 8);
        self.delay_samples = 0.000_3 * sr;
        self.feed.set_time_ms(sr, 20.0);
        self.level.set_time_ms(sr, 20.0);
        self.feed.snap(db_to_lin(params.get_f32(self.ids[1])));
        self.level.snap(db_to_lin(params.get_f32(self.ids[2])));
        self.last_fcut = -1.0;
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
        let fcut = p.get_f32(self.ids[0]).clamp(300.0, 2000.0);
        if (fcut - self.last_fcut).abs() > 0.01 {
            for ch in 0..2 {
                self.lp[ch].set(BiquadKind::LowPass, self.sample_rate, fcut, 0.707, 0.0);
            }
            self.last_fcut = fcut;
        }
        self.feed.set_target(db_to_lin(p.get_f32(self.ids[1])));
        self.level.set_target(db_to_lin(p.get_f32(self.ids[2])));
        let chans = buf.len().min(2);
        for i in 0..frames {
            let f = self.feed.next();
            let lv = self.level.next();
            if chans == 2 {
                let l = sanitize(buf[0][i]);
                let r = sanitize(buf[1][i]);
                let l_lp = self.lp[0].process(0, l);
                let r_lp = self.lp[1].process(1, r);
                let l_del = self.delay.l.read(self.delay_samples);
                let r_del = self.delay.r.read(self.delay_samples);
                self.delay.l.write(l_lp);
                self.delay.r.write(r_lp);
                buf[0][i] = lv * l + f * r_del;
                buf[1][i] = lv * r + f * l_del;
            } else {
                buf[0][i] = lv * sanitize(buf[0][i]);
            }
        }
    }

    fn reset(&mut self) {
        for b in self.lp.iter_mut() {
            b.reset();
        }
        self.delay.clear();
        self.last_fcut = -1.0;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Crosstalk canceller
// ---------------------------------------------------------------------------

/// Feed-forward crosstalk canceller: subtracts a delayed, scaled copy of the
/// dominant channel from the opposite one.
pub struct CrosstalkCanceller {
    sample_rate: f32,
    delay: DelayLine,
    ids: [u16; 3],
    amount: Smoothed,
    delay_samples: Smoothed,
}

impl CrosstalkCanceller {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            delay: DelayLine::new(1),
            ids: [u16::MAX; 3],
            amount: Smoothed::new(0.0),
            delay_samples: Smoothed::new(1.0),
        }
    }
}

impl Default for CrosstalkCanceller {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for CrosstalkCanceller {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate.max(1) as f32;
        self.ids = [
            id(params, "source"),
            id(params, "delay"),
            id(params, "amount"),
        ];
        let sr = self.sample_rate;
        self.delay = DelayLine::new(sr as usize + 8);
        self.amount.set_time_ms(sr, 20.0);
        self.delay_samples.set_time_ms(sr, 50.0);
        let cap = self.delay.capacity() as f32 - 4.0;
        self.amount
            .snap((params.get_f32(self.ids[2]) / 100.0).clamp(0.0, 1.0));
        self.delay_samples
            .snap((params.get_f32(self.ids[1]) * sr / 1000.0).clamp(1.0, cap));
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.len() < 2 {
            return;
        }
        let frames = frames.min(buf[0].len());
        if frames == 0 {
            return;
        }
        let sr = self.sample_rate;
        let cap = self.delay.capacity() as f32 - 4.0;
        self.amount
            .set_target((p.get_f32(self.ids[2]) / 100.0).clamp(0.0, 1.0));
        self.delay_samples
            .set_target((p.get_f32(self.ids[1]) * sr / 1000.0).clamp(1.0, cap));
        let (src, dst) = if p.get_enum(self.ids[0]) == 0 {
            (0, 1)
        } else {
            (1, 0)
        };
        for i in 0..frames {
            let a = self.amount.next();
            let d = self.delay_samples.next();
            let s = sanitize(buf[src][i]);
            let delayed = self.delay.read(d);
            self.delay.write(s);
            buf[dst][i] = sanitize(buf[dst][i]) - a * delayed;
        }
    }

    fn reset(&mut self) {
        self.delay.clear();
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Stereo tools
// ---------------------------------------------------------------------------

/// Mid/side balance, width and level control.
pub struct StereoTools {
    ids: [u16; 5],
    width: Smoothed,
    mid_gain: Smoothed,
    side_gain: Smoothed,
    bal_l: Smoothed,
    bal_r: Smoothed,
}

impl StereoTools {
    pub fn new() -> Self {
        Self {
            ids: [u16::MAX; 5],
            width: Smoothed::new(1.0),
            mid_gain: Smoothed::new(1.0),
            side_gain: Smoothed::new(1.0),
            bal_l: Smoothed::new(1.0),
            bal_r: Smoothed::new(1.0),
        }
    }
}

impl Default for StereoTools {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for StereoTools {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        let sr = sample_rate.max(1) as f32;
        self.ids = [
            id(params, "balance"),
            id(params, "width"),
            id(params, "mid-level"),
            id(params, "side-level"),
            id(params, "phase"),
        ];
        self.width.set_time_ms(sr, 20.0);
        self.mid_gain.set_time_ms(sr, 20.0);
        self.side_gain.set_time_ms(sr, 20.0);
        self.bal_l.set_time_ms(sr, 20.0);
        self.bal_r.set_time_ms(sr, 20.0);
        let balance = params.get_f32(self.ids[0]).clamp(-1.0, 1.0);
        let (gl, gr) = if balance > 0.0 {
            (1.0 - balance, 1.0)
        } else {
            (1.0, 1.0 + balance)
        };
        self.width.snap(params.get_f32(self.ids[1]).clamp(0.0, 2.0));
        self.mid_gain.snap(db_to_lin(params.get_f32(self.ids[2])));
        self.side_gain.snap(
            db_to_lin(params.get_f32(self.ids[3]))
                * if params.get_bool(self.ids[4]) {
                    -1.0
                } else {
                    1.0
                },
        );
        self.bal_l.snap(gl);
        self.bal_r.snap(gr);
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
        let balance = p.get_f32(self.ids[0]).clamp(-1.0, 1.0);
        let (gl, gr) = if balance > 0.0 {
            (1.0 - balance, 1.0)
        } else {
            (1.0, 1.0 + balance)
        };
        self.width
            .set_target(p.get_f32(self.ids[1]).clamp(0.0, 2.0));
        self.mid_gain.set_target(db_to_lin(p.get_f32(self.ids[2])));
        self.side_gain.set_target(
            db_to_lin(p.get_f32(self.ids[3])) * if p.get_bool(self.ids[4]) { -1.0 } else { 1.0 },
        );
        self.bal_l.set_target(gl);
        self.bal_r.set_target(gr);
        let chans = buf.len().min(2);
        for i in 0..frames {
            let w = self.width.next();
            let mg = self.mid_gain.next();
            let sg = self.side_gain.next() * w;
            let bl = self.bal_l.next();
            let br = self.bal_r.next();
            if chans == 2 {
                let l = sanitize(buf[0][i]);
                let r = sanitize(buf[1][i]);
                let mid = 0.5 * (l + r) * mg;
                let side = 0.5 * (l - r) * sg;
                buf[0][i] = (mid + side) * bl;
                buf[1][i] = (mid - side) * br;
            } else {
                buf[0][i] = sanitize(buf[0][i]) * mg;
            }
        }
    }

    fn reset(&mut self) {}

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
    fn convolver_delta_ir_delays_input_by_partition() {
        let params = store(EffectKind::Convolver);
        let mut conv = Convolver::new();
        conv.prepare(48_000, 128, 1, &params).unwrap();
        conv.load_ir(&[vec![1.0]], 48_000).unwrap();
        assert!(conv.has_ir());
        assert_eq!(conv.latency_frames(), 1024);

        let mut buf = vec![vec![0.0f32; 4096]];
        buf[0][0] = 1.0;
        conv.process(&mut buf, 4096, &params);
        assert!((buf[0][1024] - 1.0).abs() < 1e-4, "got {}", buf[0][1024]);
        for (i, &v) in buf[0].iter().enumerate() {
            if i != 1024 {
                assert!(v.abs() < 1e-5, "index {i}: {v}");
            }
        }
    }

    #[test]
    fn convolver_multi_partition_ir() {
        let params = store(EffectKind::Convolver);
        set(&params, "autogain", 0.0);
        let mut conv = Convolver::new();
        conv.prepare(48_000, 128, 1, &params).unwrap();
        let mut ir = vec![0.0f32; 1500];
        ir[0] = 1.0;
        ir[1024] = 0.5;
        conv.load_ir(&[ir], 48_000).unwrap();
        let mut buf = vec![vec![0.0f32; 4096]];
        buf[0][0] = 1.0;
        conv.process(&mut buf, 4096, &params);
        assert!((buf[0][1024] - 1.0).abs() < 1e-3, "first {}", buf[0][1024]);
        assert!((buf[0][2048] - 0.5).abs() < 1e-3, "second {}", buf[0][2048]);
    }

    #[test]
    fn convolver_handles_small_blocks() {
        let params = store(EffectKind::Convolver);
        set(&params, "autogain", 0.0);
        let mut conv = Convolver::new();
        conv.prepare(48_000, 128, 1, &params).unwrap();
        conv.load_ir(&[vec![1.0]], 48_000).unwrap();
        let mut out = vec![0.0f32; 4096];
        let mut buf = vec![vec![0.0f32; 128]];
        for block in 0..32 {
            buf[0].fill(0.0);
            if block == 0 {
                buf[0][0] = 1.0;
            }
            conv.process(&mut buf, 128, &params);
            out[block * 128..(block + 1) * 128].copy_from_slice(&buf[0]);
        }
        assert!((out[1024] - 1.0).abs() < 1e-4, "got {}", out[1024]);
    }

    #[test]
    fn convolver_without_ir_is_transparent() {
        let params = store(EffectKind::Convolver);
        let mut conv = Convolver::new();
        conv.prepare(48_000, 128, 2, &params).unwrap();
        assert!(!conv.has_ir());
        assert_eq!(conv.latency_frames(), 0);

        let mut buf = vec![vec![0.0f32; 64], vec![0.0f32; 64]];
        for i in 0..64 {
            buf[0][i] = (i as f32 * 0.1).sin();
            buf[1][i] = (i as f32 * 0.2).cos();
        }
        let orig = buf.clone();
        conv.process(&mut buf, 64, &params);
        assert_eq!(buf, orig);
    }

    #[test]
    fn convolver_autogain_normalizes_peak_response() {
        let params = store(EffectKind::Convolver);
        let mut conv = Convolver::new();
        conv.prepare(48_000, 128, 1, &params).unwrap();
        conv.load_ir(&[vec![0.1]], 48_000).unwrap();
        let mut buf = vec![vec![0.0f32; 2048]];
        buf[0][0] = 1.0;
        conv.process(&mut buf, 2048, &params);
        let peak = buf[0].iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!((peak - 1.0).abs() < 0.01, "peak {peak}");
    }

    #[test]
    fn convolver_ir_width_blends_channels() {
        let params = store(EffectKind::Convolver);
        set(&params, "autogain", 0.0);
        set(&params, "ir-width", 0.0);
        let mut conv = Convolver::new();
        conv.prepare(48_000, 128, 2, &params).unwrap();
        conv.load_ir(&[vec![1.0], vec![0.0]], 48_000).unwrap();

        let mut buf = vec![vec![0.0f32; 2048], vec![0.0f32; 2048]];
        buf[0][0] = 1.0;
        buf[1][0] = 1.0;
        conv.process(&mut buf, 2048, &params);
        assert!((buf[0][1024] - 0.5).abs() < 1e-3, "L {}", buf[0][1024]);
        assert!((buf[1][1024] - 0.5).abs() < 1e-3, "R {}", buf[1][1024]);

        set(&params, "ir-width", 100.0);
        conv.reset();
        let mut buf = vec![vec![0.0f32; 2048], vec![0.0f32; 2048]];
        buf[0][0] = 1.0;
        buf[1][0] = 1.0;
        conv.process(&mut buf, 2048, &params);
        assert!((buf[0][1024] - 1.0).abs() < 1e-3, "L {}", buf[0][1024]);
        assert!(buf[1][1024].abs() < 1e-3, "R {}", buf[1][1024]);
    }

    #[test]
    fn delay_impulse_arrives_at_set_time() {
        let params = store(EffectKind::Delay);
        set(&params, "time-l", 10.0);
        set(&params, "time-r", 20.0);
        set(&params, "feedback", 0.0);
        let mut d = Delay::new();
        d.prepare(48_000, 128, 2, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 2048], vec![0.0f32; 2048]];
        buf[0][0] = 1.0;
        buf[1][0] = 1.0;
        d.process(&mut buf, 2048, &params);
        let l = buf[0].iter().position(|&x| x.abs() > 0.5).unwrap();
        let r = buf[1].iter().position(|&x| x.abs() > 0.5).unwrap();
        assert!((l as i32 - 480).abs() <= 1, "left at {l}");
        assert!((r as i32 - 960).abs() <= 1, "right at {r}");
    }

    #[test]
    fn delay_feedback_produces_decaying_echoes() {
        let params = store(EffectKind::Delay);
        set(&params, "time-l", 10.0);
        set(&params, "time-r", 10.0);
        set(&params, "feedback", 50.0);
        let mut d = Delay::new();
        d.prepare(48_000, 128, 1, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 4096]];
        buf[0][0] = 1.0;
        d.process(&mut buf, 4096, &params);
        let e1 = buf[0][480].abs();
        let e2 = buf[0][960].abs();
        let e3 = buf[0][1440].abs();
        assert!(e1 > 0.9, "e1 {e1}");
        assert!((e2 - 0.5).abs() < 0.05, "e2 {e2}");
        assert!((e3 - 0.25).abs() < 0.05, "e3 {e3}");
    }

    #[test]
    fn reverb_tail_decays() {
        let params = store(EffectKind::Reverberation);
        set(&params, "decay-time", 0.5);
        set(&params, "room-size", 50.0);
        set(&params, "damping", 50.0);
        set(&params, "pre-delay", 0.0);
        let mut r = Reverberation::new();
        r.prepare(48_000, 128, 2, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 48_000], vec![0.0f32; 48_000]];
        buf[0][0] = 1.0;
        buf[1][0] = 1.0;
        r.process(&mut buf, 48_000, &params);
        let energy =
            |a: usize, b: usize| -> f32 { buf[0][a..b].iter().map(|x| x * x).sum::<f32>() };
        let early = energy(0, 4_800);
        let mid = energy(9_600, 14_400);
        let late = energy(43_200, 48_000);
        assert!(early > mid, "early {early} mid {mid}");
        assert!(mid > late, "mid {mid} late {late}");
        assert!(late < early * 1e-3, "late {late} early {early}");
    }

    #[test]
    fn reverb_tail_switch_mutes_late_tail() {
        let params = store(EffectKind::Reverberation);
        set(&params, "decay-time", 2.0);
        set(&params, "tail", 0.0);
        let mut r = Reverberation::new();
        r.prepare(48_000, 128, 2, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 48_000], vec![0.0f32; 48_000]];
        buf[0][0] = 1.0;
        buf[1][0] = 1.0;
        r.process(&mut buf, 48_000, &params);
        let late: f32 = buf[0][24_000..].iter().map(|x| x * x).sum();
        assert!(late < 1e-6, "late {late}");
    }

    #[test]
    fn crossfeed_leaks_low_frequencies_only() {
        let params = store(EffectKind::Crossfeed);
        set(&params, "fcut", 700.0);
        set(&params, "feed", 6.0);
        set(&params, "level", 0.0);
        let n = 4800;
        let run = |freq: f32| -> f32 {
            let mut c = Crossfeed::new();
            c.prepare(48_000, 128, 2, &params).unwrap();
            let mut buf = vec![vec![0.0f32; n], vec![0.0f32; n]];
            for i in 0..n {
                buf[0][i] = (2.0 * std::f32::consts::PI * freq * i as f32 / 48_000.0).sin();
            }
            c.process(&mut buf, n, &params);
            (buf[1][2400..].iter().map(|x| x * x).sum::<f32>() / 2400.0).sqrt()
        };
        let low = run(100.0);
        let high = run(5000.0);
        assert!(low > 0.1, "low {low}");
        assert!(high < 0.05, "high {high}");
    }

    #[test]
    fn crosstalk_canceller_subtracts_delayed_source() {
        let params = store(EffectKind::CrosstalkCanceller);
        set(&params, "source", 0.0);
        set(&params, "delay", 0.0);
        set(&params, "amount", 100.0);
        let mut c = CrosstalkCanceller::new();
        c.prepare(48_000, 128, 2, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 256], vec![0.0f32; 256]];
        buf[0][0] = 1.0;
        // The leakage in the right channel is the source delayed by one sample.
        buf[1][1] = 1.0;
        c.process(&mut buf, 256, &params);
        assert!((buf[0][0] - 1.0).abs() < 1e-6, "left {}", buf[0][0]);
        assert!(buf[1][1].abs() < 1e-6, "right {}", buf[1][1]);
    }

    #[test]
    fn reverb_stays_finite_under_sustained_feedback() {
        let params = store(EffectKind::Reverberation);
        set(&params, "decay-time", 10.0);
        set(&params, "room-size", 100.0);
        let mut r = Reverberation::new();
        r.prepare(48_000, 128, 2, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 4800], vec![0.0f32; 4800]];
        for i in 0..4800 {
            buf[0][i] = (i as f32 * 0.01).sin() * 0.5;
            buf[1][i] = (i as f32 * 0.013).cos() * 0.5;
        }
        for _ in 0..20 {
            r.process(&mut buf, 4800, &params);
            assert!(buf[0].iter().all(|x| x.is_finite()));
            assert!(buf[1].iter().all(|x| x.is_finite()));
        }
    }

    #[test]
    fn stereo_tools_width_and_phase() {
        let params = store(EffectKind::StereoTools);
        set(&params, "width", 0.0);
        let mut s = StereoTools::new();
        s.prepare(48_000, 128, 2, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 64], vec![0.0f32; 64]];
        for i in 0..64 {
            buf[0][i] = 1.0;
            buf[1][i] = -1.0;
        }
        s.process(&mut buf, 64, &params);
        for i in 0..64 {
            assert!(buf[0][i].abs() < 1e-6, "L {}", buf[0][i]);
            assert!(buf[1][i].abs() < 1e-6, "R {}", buf[1][i]);
        }

        set(&params, "width", 2.0);
        set(&params, "phase", 1.0);
        let mut s = StereoTools::new();
        s.prepare(48_000, 128, 2, &params).unwrap();
        let mut buf = vec![vec![0.0f32; 64], vec![0.0f32; 64]];
        for i in 0..64 {
            buf[0][i] = 1.0;
            buf[1][i] = -1.0;
        }
        s.process(&mut buf, 64, &params);
        assert!((buf[0][0] + 2.0).abs() < 1e-4, "L {}", buf[0][0]);
        assert!((buf[1][0] - 2.0).abs() < 1e-4, "R {}", buf[1][0]);
    }
}
