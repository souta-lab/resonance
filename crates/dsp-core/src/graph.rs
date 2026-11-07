//! Effect graph, block processing and the RT-safe graph hand-over.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::effect::AudioEffect;
use crate::kind::EffectKind;
use crate::lufs::LoudnessMeter;
use crate::meter::{MeterBank, SlotMeters};
use crate::param::{ParamStore, P_BYPASS, P_DRY, P_INPUT_GAIN, P_OUTPUT_GAIN, P_WET};
use crate::smoothing::{db_to_lin, lin_to_db};
use crate::spectrum::SpectrumAnalyzer;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineKind {
    Input,
    Output,
}

impl PipelineKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            PipelineKind::Input => "input",
            PipelineKind::Output => "output",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "input" => Some(PipelineKind::Input),
            "output" => Some(PipelineKind::Output),
            _ => None,
        }
    }
}

impl std::fmt::Display for PipelineKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One effect instance in a chain.
pub struct Slot {
    pub instance_id: String,
    pub kind: EffectKind,
    pub params: Arc<ParamStore>,
    pub effect: Box<dyn AudioEffect>,
    pub meters: Arc<SlotMeters>,
    mix: f32,
    active: bool,
    last_in_gain: f32,
    last_out_gain: f32,
}

impl Slot {
    pub fn new(
        instance_id: impl Into<String>,
        kind: EffectKind,
        params: Arc<ParamStore>,
        effect: Box<dyn AudioEffect>,
        meters: Arc<SlotMeters>,
    ) -> Self {
        let bypass = params.get_bool(P_BYPASS);
        let dry = params.get_f32(P_DRY).max(0.0);
        let wet = params.get_f32(P_WET).max(0.0);
        let mix = if bypass {
            0.0
        } else if dry + wet <= 1e-6 {
            1.0
        } else {
            (wet / (dry + wet)).clamp(0.0, 1.0)
        };
        let last_in_gain = db_to_lin(params.get_f32(P_INPUT_GAIN));
        let last_out_gain = db_to_lin(params.get_f32(P_OUTPUT_GAIN));
        Self {
            instance_id: instance_id.into(),
            kind,
            params,
            effect,
            meters,
            mix,
            active: false,
            last_in_gain,
            last_out_gain,
        }
    }

    fn process(
        &mut self,
        buf: &mut [Vec<f32>],
        scratch: &mut [Vec<f32>],
        frames: usize,
        _sample_rate: f32,
    ) {
        let p = &self.params;
        let bypass = p.get_bool(P_BYPASS);
        let dry = p.get_f32(P_DRY).max(0.0);
        let wet = p.get_f32(P_WET).max(0.0);
        let target = if bypass {
            0.0
        } else if dry + wet <= 1e-6 {
            1.0
        } else {
            (wet / (dry + wet)).clamp(0.0, 1.0)
        };

        if target <= 0.0 && self.mix <= 0.0 {
            if self.active {
                self.effect.reset();
                self.active = false;
            }
            self.meters.gain_reduction_db.set(0.0);
            return;
        }
        if !self.active {
            self.effect.reset();
            self.active = true;
        }

        let in_gain = db_to_lin(p.get_f32(P_INPUT_GAIN));
        let out_gain = db_to_lin(p.get_f32(P_OUTPUT_GAIN));
        let need_dry = target < 0.999 || self.mix < 0.999;

        if need_dry {
            for ch in 0..buf.len() {
                scratch[ch][..frames].copy_from_slice(&buf[ch][..frames]);
            }
        }

        let g0 = self.last_in_gain;
        if (in_gain - g0).abs() > 1e-6 {
            let step = (in_gain - g0) / frames as f32;
            for i in 0..frames {
                let g = g0 + step * i as f32;
                for ch in 0..buf.len() {
                    buf[ch][i] *= g;
                }
            }
        } else if (in_gain - 1.0).abs() > 1e-6 {
            for ch in 0..buf.len() {
                for x in buf[ch][..frames].iter_mut() {
                    *x *= in_gain;
                }
            }
        }
        self.last_in_gain = in_gain;

        self.effect.process(buf, frames, p);

        let g0 = self.last_out_gain;
        if (out_gain - g0).abs() > 1e-6 {
            let step = (out_gain - g0) / frames as f32;
            for i in 0..frames {
                let g = g0 + step * i as f32;
                for ch in 0..buf.len() {
                    buf[ch][i] *= g;
                }
            }
        } else if (out_gain - 1.0).abs() > 1e-6 {
            for ch in 0..buf.len() {
                for x in buf[ch][..frames].iter_mut() {
                    *x *= out_gain;
                }
            }
        }
        self.last_out_gain = out_gain;

        let m0 = self.mix;
        if need_dry || (target - m0).abs() > 1e-6 {
            let step = (target - m0) / frames as f32;
            for i in 0..frames {
                let m = m0 + step * i as f32;
                for ch in 0..buf.len() {
                    let d = scratch[ch][i];
                    buf[ch][i] = d + (buf[ch][i] - d) * m;
                }
            }
        }
        self.mix = target;

        self.meters
            .gain_reduction_db
            .set(self.effect.gain_reduction_db());
        self.meters
            .latency_frames
            .store(self.effect.latency_frames() as u32, Ordering::Relaxed);
    }
}

/// A complete input or output chain.
pub struct Pipeline {
    pub kind: PipelineKind,
    pub slots: Vec<Slot>,
    pub sample_rate: u32,
    pub max_frames: usize,
    pub channels: usize,
    pub bypass: bool,
    scratch: Vec<Vec<f32>>,
    mono: Vec<f32>,
    bypass_mix: f32,
    last_sample: [f32; 2],
    loudness: LoudnessMeter,
    spectrum: SpectrumAnalyzer,
    spectrum_counter: u32,
}

impl Pipeline {
    pub fn new(kind: PipelineKind, sample_rate: u32, max_frames: usize, channels: usize) -> Self {
        let channels = channels.clamp(1, 2);
        let max_frames = max_frames.max(16);
        Self {
            kind,
            slots: Vec::new(),
            sample_rate,
            max_frames,
            channels,
            bypass: false,
            scratch: vec![vec![0.0; max_frames]; channels],
            mono: vec![0.0; max_frames],
            bypass_mix: 1.0,
            last_sample: [0.0; 2],
            loudness: LoudnessMeter::new(sample_rate as f32),
            spectrum: SpectrumAnalyzer::new(sample_rate as f32),
            spectrum_counter: 0,
        }
    }

    pub fn add_slot(&mut self, slot: Slot) {
        self.slots.push(slot);
    }

    pub fn remove_slot(&mut self, index: usize) -> Option<Slot> {
        if index < self.slots.len() {
            Some(self.slots.remove(index))
        } else {
            None
        }
    }

    pub fn latency_frames(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| !s.params.get_bool(P_BYPASS))
            .map(|s| s.effect.latency_frames())
            .sum()
    }

    pub fn reset(&mut self) {
        for slot in self.slots.iter_mut() {
            slot.effect.reset();
            slot.active = false;
        }
        self.loudness.reset();
        self.spectrum.reset();
        self.last_sample = [0.0; 2];
    }

    pub fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, meters: &MeterBank) {
        if buf.is_empty() {
            return;
        }
        let frames = frames.min(self.max_frames).min(buf[0].len());
        if frames == 0 {
            return;
        }
        let target = if self.bypass { 0.0 } else { 1.0 };
        let need_dry = target < 0.999 || self.bypass_mix < 0.999;

        if need_dry {
            for ch in 0..buf.len().min(self.scratch.len()) {
                self.scratch[ch][..frames].copy_from_slice(&buf[ch][..frames]);
            }
        }

        if target > 0.0 || self.bypass_mix > 0.0 {
            let sr = self.sample_rate as f32;
            let Pipeline { slots, scratch, .. } = self;
            for slot in slots.iter_mut() {
                slot.process(buf, scratch, frames, sr);
            }
        }

        if need_dry {
            let m0 = self.bypass_mix;
            let step = (target - m0) / frames as f32;
            for i in 0..frames {
                let m = m0 + step * i as f32;
                for ch in 0..buf.len().min(self.scratch.len()) {
                    let d = self.scratch[ch][i];
                    buf[ch][i] = d + (buf[ch][i] - d) * m;
                }
            }
        }
        self.bypass_mix = target;

        self.update_meters(buf, frames, meters);
    }

    fn update_meters(&mut self, buf: &[Vec<f32>], frames: usize, meters: &MeterBank) {
        let mut peak = [0.0f32; 2];
        let mut true_peak = [0.0f32; 2];
        let mut sum_sq = [0.0f64; 2];

        for ch in 0..buf.len().min(2) {
            let mut p = 0.0f32;
            let mut tp = 0.0f32;
            let mut s = 0.0f64;
            let mut last = self.last_sample[ch];
            for &x in &buf[ch][..frames] {
                let a = x.abs();
                if a > p {
                    p = a;
                }
                if a > tp {
                    tp = a;
                }
                for k in 1..4 {
                    let v = last + (x - last) * (k as f32 * 0.25);
                    let av = v.abs();
                    if av > tp {
                        tp = av;
                    }
                }
                s += (x as f64) * (x as f64);
                last = x;
                self.loudness.process(ch, x);
            }
            self.last_sample[ch] = last;
            peak[ch] = p;
            true_peak[ch] = tp;
            sum_sq[ch] = s;
        }

        for ch in 0..2 {
            meters.peak_db[ch].set(lin_to_db(peak[ch]));
            meters.true_peak_db[ch].set(lin_to_db(true_peak[ch]));
            let rms = (sum_sq[ch] / frames as f64).sqrt() as f32;
            meters.rms_db[ch].set(lin_to_db(rms));
        }
        meters.lufs_m.set(self.loudness.momentary_lufs());
        meters.lufs_s.set(self.loudness.short_term_lufs());
        meters.lufs_i.set(self.loudness.integrated_lufs());
        meters
            .latency_frames
            .store(self.latency_frames() as u32, Ordering::Relaxed);
        meters
            .sample_rate
            .store(self.sample_rate, Ordering::Relaxed);

        self.spectrum_counter = self.spectrum_counter.wrapping_add(1);
        if self.spectrum_counter % 4 == 0 {
            let n = frames.min(self.mono.len());
            let chans = buf.len().min(2);
            for i in 0..n {
                let mut m = 0.0;
                for ch in 0..chans {
                    m += buf[ch][i];
                }
                self.mono[i] = m / chans as f32;
            }
            if self.spectrum.push(&self.mono[..n]) {
                let bins = *self.spectrum.bins();
                meters.spectrum_seq.fetch_add(1, Ordering::Relaxed);
                for (i, b) in bins.iter().enumerate() {
                    meters.spectrum[i].set(*b);
                }
                meters.spectrum_seq.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

struct SwapSlot {
    incoming: Option<Box<Pipeline>>,
    retired: Vec<Box<Pipeline>>,
}

/// Hand-over point between the control thread and the audio thread.
pub struct GraphSwap {
    inner: Mutex<SwapSlot>,
}

impl GraphSwap {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(SwapSlot {
                incoming: None,
                retired: Vec::new(),
            }),
        })
    }

    /// Install a new graph. Returns graphs that are now unreferenced; drop them
    /// on the control thread.
    pub fn install(&self, pipeline: Box<Pipeline>) -> Vec<Box<Pipeline>> {
        let mut slot = self.inner.lock();
        let previous = slot.incoming.replace(pipeline);
        let mut retired = std::mem::take(&mut slot.retired);
        drop(slot);
        if let Some(previous) = previous {
            retired.push(previous);
        }
        retired
    }

    /// Audio-thread side: take a pending graph without blocking.
    pub fn take_incoming(&self) -> Option<Box<Pipeline>> {
        self.inner.try_lock().and_then(|mut s| s.incoming.take())
    }

    /// Audio-thread side: hand a retired graph back. Returns false if the
    /// control thread holds the lock; the caller retries next block.
    pub fn retire(&self, pipeline: Box<Pipeline>) -> Result<(), Box<Pipeline>> {
        match self.inner.try_lock() {
            Some(mut slot) => {
                slot.retired.push(pipeline);
                Ok(())
            }
            None => Err(pipeline),
        }
    }

    pub fn drain_retired(&self) -> Vec<Box<Pipeline>> {
        let mut slot = self.inner.lock();
        std::mem::take(&mut slot.retired)
    }
}

impl Default for GraphSwap {
    fn default() -> Self {
        Self {
            inner: Mutex::new(SwapSlot {
                incoming: None,
                retired: Vec::new(),
            }),
        }
    }
}

/// Owns the live graph on the audio thread.
pub struct Engine {
    pub pipeline: Pipeline,
    pub swap: Arc<GraphSwap>,
    pub meters: Arc<MeterBank>,
    pending_retire: Option<Box<Pipeline>>,
    times: [f32; 256],
    time_idx: usize,
    block_count: u64,
    period_us: f32,
    realtime: bool,
}

impl Engine {
    pub fn new(pipeline: Pipeline, swap: Arc<GraphSwap>, meters: Arc<MeterBank>) -> Self {
        let period_us = pipeline.max_frames as f32 / pipeline.sample_rate as f32 * 1e6;
        Self {
            pipeline,
            swap,
            meters,
            pending_retire: None,
            times: [0.0; 256],
            time_idx: 0,
            block_count: 0,
            period_us,
            realtime: false,
        }
    }

    /// Xrun accounting is only meaningful when `process` is called from a
    /// real-time callback. The `pw-cat` bridge runs in a blocking loop, so it
    /// disables this to avoid reporting pipe buffering as xruns.
    pub fn set_realtime(&mut self, realtime: bool) {
        self.realtime = realtime;
    }

    pub fn process(&mut self, buf: &mut [Vec<f32>], frames: usize) {
        if let Some(pending) = self.pending_retire.take() {
            if let Err(pending) = self.swap.retire(pending) {
                self.pending_retire = Some(pending);
            }
        }
        if let Some(new) = self.swap.take_incoming() {
            let old = std::mem::replace(&mut self.pipeline, *new);
            let old = Box::new(old);
            if let Err(old) = self.swap.retire(old) {
                self.pending_retire = Some(old);
            }
        }

        let start = Instant::now();
        self.pipeline.process(buf, frames, &self.meters);
        let elapsed_us = start.elapsed().as_secs_f32() * 1e6;

        self.meters.dsp_last_us.set(elapsed_us);
        self.times[self.time_idx] = elapsed_us;
        self.time_idx = (self.time_idx + 1) % self.times.len();
        self.block_count += 1;

        if self.block_count % 64 == 0 {
            let mut sorted = self.times;
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            self.meters.dsp_p50_us.set(sorted[128]);
            self.meters.dsp_p99_us.set(sorted[253]);
        }

        let period = if frames > 0 {
            frames as f32 / self.pipeline.sample_rate as f32 * 1e6
        } else {
            self.period_us
        };
        if self.realtime && elapsed_us > period {
            self.meters.xruns.fetch_add(1, Ordering::Relaxed);
        }
        self.meters
            .frames
            .fetch_add(frames as u64, Ordering::Relaxed);
        self.meters.quantum.store(frames as u32, Ordering::Relaxed);
    }
}
