//! Voice & pitch effects: granular pitch shifting, autotune, harmonizer,
//! a broadcast-style speech chain and centre-channel suppression.

use std::f32::consts::PI;

use dsp_core::biquad::{Biquad, BiquadKind, Cascade};
use dsp_core::delay::DelayLine;
use dsp_core::envelope::{Envelope, GainComputer, Rms};
use dsp_core::param::{ParamStore, ParamValue};
use dsp_core::smoothing::{db_to_lin, hz_to_midi, lin_to_db, midi_to_hz, Smoothed};
use dsp_core::{AudioEffect, EffectError};

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
// Granular pitch shifter
// ---------------------------------------------------------------------------

/// Two-tap granular pitch shifter.
///
/// A sawtooth phase drives two read heads half a window apart. The heads are
/// crossfaded with equal-power weights, so when one head wraps around the delay
/// line the other one is at the centre of its range and masks the jump. The
/// delay line is sized for the largest window in `prepare`; changing the window
/// at runtime only changes a scalar, it never allocates.
struct GranularShifter {
    line: DelayLine,
    sample_rate: f32,
    window: f32,
    max_window: f32,
    phase: f32,
    ratio: f32,
    prepared: bool,
}

impl GranularShifter {
    fn new() -> Self {
        Self {
            line: DelayLine::new(64),
            sample_rate: 48_000.0,
            window: 1.0,
            max_window: 1.0,
            phase: 0.0,
            ratio: 1.0,
            prepared: false,
        }
    }

    fn prepare(&mut self, sample_rate: f32, max_window_ms: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.max_window = (self.sample_rate * max_window_ms.clamp(1.0, 200.0) / 1000.0).max(8.0);
        self.line = DelayLine::new(self.max_window as usize + 8);
        self.window = self.max_window;
        self.phase = 0.0;
        self.ratio = 1.0;
        self.prepared = true;
    }

    fn set_window_ms(&mut self, ms: f32) {
        let w = self.sample_rate * ms.clamp(1.0, 200.0) / 1000.0;
        self.window = w.clamp(8.0, self.max_window);
    }

    fn set_ratio(&mut self, ratio: f32) {
        // Five octaves either way covers the full semitones + octaves range.
        self.ratio = if ratio.is_finite() {
            ratio.clamp(0.031_25, 32.0)
        } else {
            1.0
        };
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        if !self.prepared {
            return sanitize(x);
        }
        let x = sanitize(x);
        self.line.write(x);
        // A ratio of exactly one is a pass-through; the granular path would
        // otherwise add a fixed comb delay.
        if (self.ratio - 1.0).abs() < 1e-4 {
            return x;
        }
        let p = self.phase;
        let d1 = p * self.window;
        let p2 = if p + 0.5 >= 1.0 { p - 0.5 } else { p + 0.5 };
        let d2 = p2 * self.window;
        let a = self.line.read(d1);
        let b = self.line.read(d2);
        let wa = (PI * p).sin();
        let wb = (PI * (p + 0.5)).sin().abs();
        let y = a * wa + b * wb;
        // The read pointer falls behind the write pointer at `ratio - 1`
        // samples per sample, i.e. the delay grows by that much.
        self.phase = (self.phase + (1.0 - self.ratio) / self.window).rem_euclid(1.0);
        sanitize(y)
    }

    fn reset(&mut self) {
        self.line.clear();
        self.phase = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Pitch shift
// ---------------------------------------------------------------------------

/// Two-tap granular pitch shifter.
///
/// `mix` is the effect's own wet level (100% = full wet output); the graph's
/// dry/wet controls handle blending with the unprocessed signal. The granular
/// window introduces up to `window` milliseconds of delay but the effect
/// reports zero latency, which is acceptable for a granular shifter.
pub struct PitchShift {
    sample_rate: f32,
    shifters: [GranularShifter; 2],
    ids: [u16; 5],
    mix: Smoothed,
}

impl PitchShift {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            shifters: [GranularShifter::new(), GranularShifter::new()],
            ids: [u16::MAX; 5],
            mix: Smoothed::new(1.0),
        }
    }
}

impl Default for PitchShift {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for PitchShift {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [
            id(params, "semitones"),
            id(params, "cents"),
            id(params, "octaves"),
            id(params, "window"),
            id(params, "mix"),
        ];
        for sh in self.shifters.iter_mut() {
            sh.prepare(self.sample_rate, 100.0);
        }
        self.mix.set_time_ms(self.sample_rate, 10.0);
        self.mix.snap(1.0);
        Ok(())
    }

    #[allow(clippy::needless_range_loop)]
    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let semitones = p.get_f32(self.ids[0]);
        let cents = p.get_f32(self.ids[1]);
        let octaves = p.get_f32(self.ids[2]);
        let window_ms = p.get_f32(self.ids[3]);
        let mix = (p.get_f32(self.ids[4]) / 100.0).clamp(0.0, 1.0);
        let ratio = 2f32
            .powf((semitones + cents / 100.0 + octaves * 12.0) / 12.0)
            .clamp(0.031_25, 32.0);
        for sh in self.shifters.iter_mut() {
            sh.set_window_ms(window_ms);
            sh.set_ratio(ratio);
        }
        self.mix.set_target(mix);
        let nch = buf.len().min(2);
        for i in 0..frames {
            let m = self.mix.next();
            for ch in 0..nch {
                let wet = self.shifters[ch].process(buf[ch][i]);
                buf[ch][i] = wet * m;
            }
        }
    }

    fn reset(&mut self) {
        for sh in self.shifters.iter_mut() {
            sh.reset();
        }
        self.mix.snap(1.0);
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Autotune
// ---------------------------------------------------------------------------

const DET_LEN: usize = 1024;
const DET_MASK: usize = DET_LEN - 1;
const DET_INTERVAL: usize = 512;
const YIN_WINDOW: usize = DET_LEN / 2;
const YIN_TAU_MAX: usize = DET_LEN / 2;
const YIN_THRESHOLD: f32 = 0.15;

fn scale_has(note: i32, scale: u16) -> bool {
    let pc = note.rem_euclid(12);
    match scale {
        1 => matches!(pc, 0 | 2 | 4 | 5 | 7 | 9 | 11),
        2 => matches!(pc, 0 | 2 | 3 | 5 | 7 | 8 | 10),
        _ => true,
    }
}

/// YIN pitch detector feeding a granular pitch shifter.
///
/// Detection runs on a 2x-decimated mono mix every 512 input samples, so the
/// cost is bounded and independent of the block size. The detected fundamental
/// is exposed through [`AudioEffect::telemetry`] in Hz.
pub struct Autotune {
    sample_rate: f32,
    det_sr: f32,
    shifters: [GranularShifter; 2],
    ids: [u16; 6],
    dec_lp: Biquad,
    dec_phase: bool,
    det_buf: Vec<f32>,
    det_pos: usize,
    det_lin: Vec<f32>,
    det_count: usize,
    yin_diff: Vec<f32>,
    yin_cum: Vec<f32>,
    level: Rms,
    detected_hz: f32,
    target_ratio: Smoothed,
    amount: f32,
    threshold_db: f32,
    min_note: i32,
    max_note: i32,
    scale: u16,
}

impl Autotune {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            det_sr: 24_000.0,
            shifters: [GranularShifter::new(), GranularShifter::new()],
            ids: [u16::MAX; 6],
            dec_lp: Biquad::new(),
            dec_phase: false,
            det_buf: Vec::new(),
            det_pos: 0,
            det_lin: Vec::new(),
            det_count: 0,
            yin_diff: Vec::new(),
            yin_cum: Vec::new(),
            level: Rms::new(),
            detected_hz: 0.0,
            target_ratio: Smoothed::new(1.0),
            amount: 1.0,
            threshold_db: -50.0,
            min_note: 36,
            max_note: 84,
            scale: 0,
        }
    }

    /// Run YIN on the last `DET_LEN` decimated samples and update the target
    /// correction ratio. Never produces NaN: every intermediate is validated.
    fn detect(&mut self) {
        for i in 0..DET_LEN {
            self.det_lin[i] = self.det_buf[(self.det_pos + i) & DET_MASK];
        }
        let level_db = lin_to_db(self.level.value());
        if !level_db.is_finite() || level_db < self.threshold_db {
            self.detected_hz = 0.0;
            self.target_ratio.set_target(1.0);
            return;
        }

        for tau in 1..=YIN_TAU_MAX {
            let mut sum = 0.0f32;
            for j in 0..YIN_WINDOW {
                let d = self.det_lin[j] - self.det_lin[j + tau];
                sum += d * d;
            }
            self.yin_diff[tau] = sum;
        }
        self.yin_cum[0] = 1.0;
        let mut running = 0.0f32;
        for tau in 1..=YIN_TAU_MAX {
            running += self.yin_diff[tau];
            self.yin_cum[tau] = if running > 1e-12 {
                self.yin_diff[tau] * tau as f32 / running
            } else {
                1.0
            };
        }

        // First dip below the aperiodicity threshold, refined to a local min.
        let mut best = 0usize;
        let mut tau = 2usize;
        while tau < YIN_TAU_MAX {
            if self.yin_cum[tau] < YIN_THRESHOLD {
                let mut t = tau;
                while t + 1 < YIN_TAU_MAX && self.yin_cum[t + 1] < self.yin_cum[t] {
                    t += 1;
                }
                best = t;
                break;
            }
            tau += 1;
        }
        if best == 0 {
            // Fall back to the global minimum, but only trust a clear one.
            let mut min_val = f32::MAX;
            for t in 2..YIN_TAU_MAX {
                if self.yin_cum[t] < min_val {
                    min_val = self.yin_cum[t];
                    best = t;
                }
            }
            if best == 0 || !min_val.is_finite() || min_val > 0.6 {
                self.detected_hz = 0.0;
                self.target_ratio.set_target(1.0);
                return;
            }
        }

        let a = self.yin_cum[best - 1];
        let b = self.yin_cum[best];
        let c = self.yin_cum[best + 1];
        let denom = a - 2.0 * b + c;
        let delta = if denom.abs() > 1e-9 {
            (0.5 * (a - c) / denom).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let tau_f = best as f32 + delta;
        if !tau_f.is_finite() || tau_f <= 1.0 {
            self.detected_hz = 0.0;
            self.target_ratio.set_target(1.0);
            return;
        }
        let f0 = self.det_sr / tau_f;
        if !f0.is_finite() || !(40.0..=2000.0).contains(&f0) {
            self.detected_hz = 0.0;
            self.target_ratio.set_target(1.0);
            return;
        }
        self.detected_hz = f0;

        let midi = hz_to_midi(f0);
        if !midi.is_finite()
            || midi < self.min_note as f32 - 0.5
            || midi > self.max_note as f32 + 0.5
        {
            self.target_ratio.set_target(1.0);
            return;
        }
        let rounded = midi.round() as i32;
        let mut best_note = rounded;
        let mut best_dist = f32::MAX;
        for n in (rounded - 6)..=(rounded + 6) {
            if n < self.min_note || n > self.max_note || !scale_has(n, self.scale) {
                continue;
            }
            let d = (n as f32 - midi).abs();
            if d < best_dist {
                best_dist = d;
                best_note = n;
            }
        }
        if best_dist == f32::MAX {
            self.target_ratio.set_target(1.0);
            return;
        }
        let target_hz = midi_to_hz(best_note as f32);
        let mut ratio = target_hz / f0;
        ratio = 1.0 + (ratio - 1.0) * self.amount;
        self.target_ratio.set_target(ratio.clamp(0.5, 2.0));
    }
}

impl Default for Autotune {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for Autotune {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.det_sr = self.sample_rate * 0.5;
        self.ids = [
            id(params, "speed"),
            id(params, "amount"),
            id(params, "threshold"),
            id(params, "min-note"),
            id(params, "max-note"),
            id(params, "scale"),
        ];
        self.dec_lp.set(
            BiquadKind::LowPass,
            self.sample_rate,
            self.det_sr * 0.45,
            0.707,
            0.0,
        );
        self.det_buf = vec![0.0; DET_LEN];
        self.det_lin = vec![0.0; DET_LEN];
        self.yin_diff = vec![0.0; YIN_TAU_MAX + 1];
        self.yin_cum = vec![0.0; YIN_TAU_MAX + 1];
        self.det_pos = 0;
        self.det_count = 0;
        self.dec_phase = false;
        self.level.set_window_ms(self.sample_rate, 20.0);
        for sh in self.shifters.iter_mut() {
            sh.prepare(self.sample_rate, 50.0);
        }
        self.target_ratio.set_time_ms(self.sample_rate, 100.0);
        self.target_ratio.snap(1.0);
        self.detected_hz = 0.0;
        Ok(())
    }

    #[allow(clippy::needless_range_loop)]
    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() || self.det_buf.is_empty() {
            return;
        }
        let speed = p.get_f32(self.ids[0]).clamp(0.0, 100.0);
        self.amount = (p.get_f32(self.ids[1]) / 100.0).clamp(0.0, 1.0);
        self.threshold_db = p.get_f32(self.ids[2]);
        self.min_note = (p.get_f32(self.ids[3]).round() as i32).clamp(0, 127);
        self.max_note = (p.get_f32(self.ids[4]).round() as i32)
            .clamp(0, 127)
            .max(self.min_note);
        self.scale = p.get_enum(self.ids[5]);
        // Speed 0 -> 500 ms correction, speed 100 -> 5 ms.
        let tc = (500.0 - 4.95 * speed).max(1.0);
        self.target_ratio.set_time_ms(self.sample_rate, tc);

        let nch = buf.len().min(2);
        for i in 0..frames {
            let mono = if nch >= 2 {
                (buf[0][i] + buf[1][i]) * 0.5
            } else {
                buf[0][i]
            };
            let mono = sanitize(mono);
            self.level.process(mono);
            let filtered = self.dec_lp.process(0, mono);
            self.dec_phase = !self.dec_phase;
            if !self.dec_phase {
                self.det_buf[self.det_pos] = filtered;
                self.det_pos = (self.det_pos + 1) & DET_MASK;
            }
            self.det_count += 1;
            if self.det_count >= DET_INTERVAL {
                self.det_count = 0;
                self.detect();
            }
            let ratio = self.target_ratio.next();
            for ch in 0..nch {
                self.shifters[ch].set_ratio(ratio);
                buf[ch][i] = self.shifters[ch].process(buf[ch][i]);
            }
        }
    }

    fn reset(&mut self) {
        for sh in self.shifters.iter_mut() {
            sh.reset();
        }
        self.dec_lp.reset();
        self.det_buf.fill(0.0);
        self.det_lin.fill(0.0);
        self.det_pos = 0;
        self.det_count = 0;
        self.dec_phase = false;
        self.level.reset();
        self.detected_hz = 0.0;
        self.target_ratio.snap(1.0);
    }

    fn telemetry(&self) -> Option<f32> {
        Some(self.detected_hz)
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Harmonizer
// ---------------------------------------------------------------------------

/// Dry signal plus up to two pitch-shifted harmony voices.
///
/// `voices` counts the total number of voices including the dry lead, so 1 is
/// dry only, 2 adds `interval-1` and 3 adds both intervals.
pub struct Harmonizer {
    sample_rate: f32,
    shifters: [[GranularShifter; 2]; 2],
    ids: [u16; 4],
    mix: Smoothed,
}

impl Harmonizer {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            shifters: [
                [GranularShifter::new(), GranularShifter::new()],
                [GranularShifter::new(), GranularShifter::new()],
            ],
            ids: [u16::MAX; 4],
            mix: Smoothed::new(0.5),
        }
    }
}

impl Default for Harmonizer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for Harmonizer {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [
            id(params, "interval-1"),
            id(params, "interval-2"),
            id(params, "voices"),
            id(params, "mix"),
        ];
        for voice in self.shifters.iter_mut() {
            for sh in voice.iter_mut() {
                sh.prepare(self.sample_rate, 50.0);
            }
        }
        self.mix.set_time_ms(self.sample_rate, 10.0);
        self.mix.snap(0.5);
        Ok(())
    }

    #[allow(clippy::needless_range_loop)]
    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let i1 = p.get_f32(self.ids[0]).clamp(-24.0, 24.0);
        let i2 = p.get_f32(self.ids[1]).clamp(-24.0, 24.0);
        let voices = (p.get_f32(self.ids[2]).round() as i32).clamp(1, 3) as usize;
        let mix = (p.get_f32(self.ids[3]) / 100.0).clamp(0.0, 1.0);
        let r1 = 2f32.powf(i1 / 12.0).clamp(0.125, 8.0);
        let r2 = 2f32.powf(i2 / 12.0).clamp(0.125, 8.0);
        for ch in 0..2 {
            self.shifters[0][ch].set_ratio(r1);
            self.shifters[1][ch].set_ratio(r2);
        }
        self.mix.set_target(mix);
        let nch = buf.len().min(2);
        for i in 0..frames {
            let m = self.mix.next();
            for ch in 0..nch {
                let dry = sanitize(buf[ch][i]);
                // Always run both voices so their delay lines stay warm when
                // the voice count changes.
                let v1 = self.shifters[0][ch].process(dry);
                let v2 = self.shifters[1][ch].process(dry);
                let mut wet = 0.0;
                if voices >= 2 {
                    wet += v1;
                }
                if voices >= 3 {
                    wet += v2;
                }
                buf[ch][i] = dry + wet * m;
            }
        }
    }

    fn reset(&mut self) {
        for voice in self.shifters.iter_mut() {
            for sh in voice.iter_mut() {
                sh.reset();
            }
        }
        self.mix.snap(0.5);
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Speech processor
// ---------------------------------------------------------------------------

const DEESS_SPLIT_HZ: f32 = 5_500.0;
const DEESS_THRESHOLD_DB: f32 = -30.0;

/// Broadcast voice chain: band limiting, compression and split-band de-essing.
pub struct SpeechProcessor {
    sample_rate: f32,
    hpf: [Cascade; 2],
    lpf: [Cascade; 2],
    deess_lp: [Cascade; 2],
    deess_hp: [Cascade; 2],
    comp_env: Envelope,
    deess_env: Envelope,
    gc: GainComputer,
    ids: [u16; 7],
    last: (f32, f32),
    deess_amount: f32,
    gr_db: f32,
}

impl SpeechProcessor {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            hpf: [Cascade::new(2), Cascade::new(2)],
            lpf: [Cascade::new(2), Cascade::new(2)],
            deess_lp: [Cascade::new(4), Cascade::new(4)],
            deess_hp: [Cascade::new(4), Cascade::new(4)],
            comp_env: Envelope::new(),
            deess_env: Envelope::new(),
            gc: GainComputer::new(),
            ids: [u16::MAX; 7],
            last: (f32::NAN, f32::NAN),
            deess_amount: 0.0,
            gr_db: 0.0,
        }
    }

    fn update_filters(&mut self, hpf: f32, lpf: f32) {
        let key = (hpf, lpf);
        if self.last == key {
            return;
        }
        for ch in 0..2 {
            self.hpf[ch].set(BiquadKind::HighPass, self.sample_rate, hpf, 0.0);
            self.lpf[ch].set(BiquadKind::LowPass, self.sample_rate, lpf, 0.0);
            self.deess_lp[ch].set(BiquadKind::LowPass, self.sample_rate, DEESS_SPLIT_HZ, 0.0);
            self.deess_hp[ch].set(BiquadKind::HighPass, self.sample_rate, DEESS_SPLIT_HZ, 0.0);
        }
        self.last = key;
    }
}

impl Default for SpeechProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for SpeechProcessor {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [
            id(params, "threshold"),
            id(params, "ratio"),
            id(params, "attack"),
            id(params, "release"),
            id(params, "hpf"),
            id(params, "lpf"),
            id(params, "deess"),
        ];
        self.last = (f32::NAN, f32::NAN);
        self.update_filters(80.0, 8_000.0);
        self.comp_env.set_times(self.sample_rate, 5.0, 100.0);
        self.deess_env.set_times(self.sample_rate, 1.0, 50.0);
        self.gc.threshold_db = -20.0;
        self.gc.ratio = 3.0;
        self.gc.knee_db = 3.0;
        self.gc.makeup_db = 0.0;
        self.gr_db = 0.0;
        Ok(())
    }

    #[allow(clippy::needless_range_loop)]
    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let threshold = p.get_f32(self.ids[0]);
        let ratio = p.get_f32(self.ids[1]).max(1.0);
        let attack = p.get_f32(self.ids[2]).max(0.1);
        let release = p.get_f32(self.ids[3]).max(1.0);
        let hpf = p.get_f32(self.ids[4]);
        let lpf = p.get_f32(self.ids[5]);
        self.deess_amount = (p.get_f32(self.ids[6]) / 100.0).clamp(0.0, 1.0);
        self.update_filters(hpf, lpf);
        self.comp_env.set_times(self.sample_rate, attack, release);
        self.gc.threshold_db = threshold;
        self.gc.ratio = ratio;
        self.gc.knee_db = 3.0;
        self.gc.makeup_db = 0.0;

        let nch = buf.len().min(2);
        for i in 0..frames {
            let mut peak = 0.0f32;
            for ch in 0..nch {
                let x = sanitize(buf[ch][i]);
                let y = self.hpf[ch].process(ch, x);
                let y = self.lpf[ch].process(ch, y);
                buf[ch][i] = y;
                peak = peak.max(y.abs());
            }
            let env = self.comp_env.process(peak);
            let gr = self.gc.compute(lin_to_db(env));
            let g = db_to_lin(gr);
            self.gr_db = gr;
            for ch in 0..nch {
                buf[ch][i] *= g;
            }

            let mut high_peak = 0.0f32;
            let mut low = [0.0f32; 2];
            let mut high = [0.0f32; 2];
            for ch in 0..nch {
                let y = buf[ch][i];
                low[ch] = self.deess_lp[ch].process(ch, y);
                high[ch] = self.deess_hp[ch].process(ch, y);
                high_peak = high_peak.max(high[ch].abs());
            }
            let denv = self.deess_env.process(high_peak);
            let ddb = lin_to_db(denv);
            let reduction = if ddb > DEESS_THRESHOLD_DB {
                ((ddb - DEESS_THRESHOLD_DB) * 0.75 * self.deess_amount).clamp(0.0, 12.0)
            } else {
                0.0
            };
            let dg = db_to_lin(-reduction);
            for ch in 0..nch {
                buf[ch][i] = low[ch] + high[ch] * dg;
            }
        }
    }

    fn reset(&mut self) {
        for ch in 0..2 {
            self.hpf[ch].reset();
            self.lpf[ch].reset();
            self.deess_lp[ch].reset();
            self.deess_hp[ch].reset();
        }
        self.comp_env.reset();
        self.deess_env.reset();
        self.last = (f32::NAN, f32::NAN);
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
// Voice suppressor
// ---------------------------------------------------------------------------

/// Attenuates centre-panned speech by gating the mid channel while keeping the
/// side channel intact.
pub struct VoiceSuppressor {
    sample_rate: f32,
    rms: Rms,
    gain: Smoothed,
    ids: [u16; 3],
    gr_db: f32,
}

impl VoiceSuppressor {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            rms: Rms::new(),
            gain: Smoothed::new(1.0),
            ids: [u16::MAX; 3],
            gr_db: 0.0,
        }
    }
}

impl Default for VoiceSuppressor {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEffect for VoiceSuppressor {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [
            id(params, "threshold"),
            id(params, "amount"),
            id(params, "width"),
        ];
        self.rms.set_window_ms(self.sample_rate, 30.0);
        self.gain.set_time_ms(self.sample_rate, 15.0);
        self.gain.snap(1.0);
        self.gr_db = 0.0;
        Ok(())
    }

    #[allow(clippy::needless_range_loop)]
    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let threshold = p.get_f32(self.ids[0]);
        let amount = (p.get_f32(self.ids[1]) / 100.0).clamp(0.0, 1.0);
        let width = (p.get_f32(self.ids[2]) / 100.0).clamp(0.0, 1.0);
        let removal = amount * width;
        let nch = buf.len().min(2);
        for i in 0..frames {
            let (mid, side) = if nch >= 2 {
                let l = sanitize(buf[0][i]);
                let r = sanitize(buf[1][i]);
                ((l + r) * 0.5, (l - r) * 0.5)
            } else {
                (sanitize(buf[0][i]), 0.0)
            };
            let level = self.rms.process(mid);
            let db = lin_to_db(level);
            let target = if db > threshold { 1.0 - removal } else { 1.0 };
            self.gain.set_target(target);
            let g = self.gain.next();
            self.gr_db = 20.0 * g.max(1e-6).log10();
            let m = mid * g;
            if nch >= 2 {
                buf[0][i] = m + side;
                buf[1][i] = m - side;
            } else {
                buf[0][i] = m;
            }
        }
    }

    fn reset(&mut self) {
        self.rms.reset();
        self.gain.snap(1.0);
        self.gr_db = 0.0;
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
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

    fn sine(freq: f32, sample_rate: f32, len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / sample_rate).sin())
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        if x.is_empty() {
            return 0.0;
        }
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// Single-bin DFT magnitude, enough to compare two candidate frequencies.
    fn goertzel(x: &[f32], sample_rate: f32, freq: f32) -> f32 {
        let w = 2.0 * PI * freq / sample_rate;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for &v in x {
            let s0 = v + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt() / x.len() as f32
    }

    #[test]
    fn pitch_shift_moves_a_tone_up_an_octave() {
        let sr = 48_000u32;
        let params = store(EffectKind::PitchShift);
        set_f32(&params, "semitones", 12.0);
        set_f32(&params, "mix", 100.0);
        let mut fx = PitchShift::new();
        fx.prepare(sr, 512, 2, &params).unwrap();

        let n = 24_000;
        let input = sine(440.0, sr as f32, n, 0.5);
        let mut buf = vec![input.clone(), input];
        fx.process(&mut buf, n, &params);

        assert_eq!(buf[0].len(), n);
        assert!(buf[0].iter().all(|x| x.is_finite()));
        let tail = &buf[0][n / 2..];
        let at_440 = goertzel(tail, sr as f32, 440.0);
        let at_880 = goertzel(tail, sr as f32, 880.0);
        assert!(at_880 > at_440 * 2.0, "440={at_440} 880={at_880}");
    }

    #[test]
    fn pitch_shift_handles_mono_and_non_finite_input() {
        let sr = 48_000u32;
        let params = store(EffectKind::PitchShift);
        set_f32(&params, "semitones", -5.0);
        let mut fx = PitchShift::new();
        fx.prepare(sr, 256, 1, &params).unwrap();

        let mut buf = vec![vec![0.0f32; 1024]];
        for (i, x) in buf[0].iter_mut().enumerate() {
            *x = if i % 97 == 0 {
                f32::NAN
            } else {
                (i as f32 * 0.01).sin()
            };
        }
        fx.process(&mut buf, 1024, &params);
        assert_eq!(buf[0].len(), 1024);
        assert!(buf[0].iter().all(|x| x.is_finite()));
    }

    #[test]
    fn pitch_shift_mix_scales_the_wet_output() {
        let sr = 48_000u32;
        let params = store(EffectKind::PitchShift);
        set_f32(&params, "semitones", 7.0);
        set_f32(&params, "mix", 0.0);
        let mut fx = PitchShift::new();
        fx.prepare(sr, 256, 2, &params).unwrap();

        let n = 16_384;
        let input = sine(220.0, sr as f32, n, 0.5);
        let mut buf = vec![input.clone(), input];
        fx.process(&mut buf, n, &params);
        // The mix smoother fades from its previous value over ~10 ms.
        let tail = &buf[0][n - 1024..];
        let peak = tail.iter().fold(0.0f32, |a, x| a.max(x.abs()));
        assert!(peak < 1e-4, "mix=0 should mute the wet output, peak={peak}");
    }

    #[test]
    fn autotune_detects_440_hz() {
        let sr = 48_000u32;
        let params = store(EffectKind::Autotune);
        set_f32(&params, "threshold", -60.0);
        set_f32(&params, "amount", 100.0);
        set_f32(&params, "speed", 100.0);
        let mut fx = Autotune::new();
        fx.prepare(sr, 512, 2, &params).unwrap();

        let n = 24_000;
        let input = sine(440.0, sr as f32, n, 0.5);
        let mut buf = vec![input.clone(), input];
        fx.process(&mut buf, n, &params);

        let hz = fx.telemetry().expect("telemetry");
        assert!((hz - 440.0).abs() < 8.0, "detected {hz} Hz");
        assert!(buf[0].iter().all(|x| x.is_finite()));
    }

    #[test]
    fn autotune_pulls_a_sharp_tone_towards_the_note() {
        let sr = 48_000u32;
        let params = store(EffectKind::Autotune);
        set_f32(&params, "threshold", -60.0);
        set_f32(&params, "amount", 100.0);
        set_f32(&params, "speed", 100.0);
        set_enum(&params, "scale", 0);
        let mut fx = Autotune::new();
        fx.prepare(sr, 512, 2, &params).unwrap();

        let n = 48_000;
        let input = sine(452.0, sr as f32, n, 0.5);
        let mut buf = vec![input.clone(), input];
        fx.process(&mut buf, n, &params);

        let tail = &buf[0][n / 2..];
        let at_440 = goertzel(tail, sr as f32, 440.0);
        let at_452 = goertzel(tail, sr as f32, 452.0);
        assert!(
            at_440 > at_452 * 1.5,
            "440={at_440} 452={at_452} (detected {:?})",
            fx.telemetry()
        );
    }

    #[test]
    fn harmonizer_adds_shifted_voices() {
        let sr = 48_000u32;
        let params = store(EffectKind::Harmonizer);
        set_f32(&params, "interval-1", 12.0);
        set_f32(&params, "interval-2", 7.0);
        set_f32(&params, "voices", 3.0);
        set_f32(&params, "mix", 100.0);
        let mut fx = Harmonizer::new();
        fx.prepare(sr, 512, 2, &params).unwrap();

        let n = 24_000;
        let input = sine(440.0, sr as f32, n, 0.3);
        let mut buf = vec![input.clone(), input];
        fx.process(&mut buf, n, &params);

        let tail = &buf[0][n / 2..];
        let octave = goertzel(tail, sr as f32, 880.0);
        let fifth = goertzel(tail, sr as f32, 659.25);
        assert!(octave > 0.01, "octave voice missing: {octave}");
        assert!(fifth > 0.01, "fifth voice missing: {fifth}");
        assert!(buf[0].iter().all(|x| x.is_finite()));
    }

    #[test]
    fn speech_processor_blocks_dc_and_compresses() {
        let sr = 48_000u32;
        let params = store(EffectKind::SpeechProcessor);
        set_f32(&params, "threshold", -30.0);
        set_f32(&params, "ratio", 8.0);
        set_f32(&params, "hpf", 100.0);
        set_f32(&params, "lpf", 8_000.0);
        set_f32(&params, "deess", 0.0);
        let mut fx = SpeechProcessor::new();
        fx.prepare(sr, 512, 2, &params).unwrap();

        let n = 9_600;
        let mut buf = vec![vec![1.0f32; n], vec![1.0f32; n]];
        fx.process(&mut buf, n, &params);
        let dc = rms(&buf[0][n / 2..]);
        assert!(dc < 0.05, "DC not removed: {dc}");

        fx.reset();
        let input = sine(1_000.0, sr as f32, n, 0.8);
        let mut buf = vec![input.clone(), input.clone()];
        fx.process(&mut buf, n, &params);
        let out = rms(&buf[0][n / 2..]);
        let inp = rms(&input[n / 2..]);
        assert!(out < inp * 0.8, "not compressed: in={inp} out={out}");
        assert!(fx.gain_reduction_db() < 0.0);
    }

    #[test]
    fn voice_suppressor_attenuates_centre_but_keeps_sides() {
        let sr = 48_000u32;
        let params = store(EffectKind::VoiceSuppressor);
        set_f32(&params, "threshold", -40.0);
        set_f32(&params, "amount", 100.0);
        set_f32(&params, "width", 100.0);
        let mut fx = VoiceSuppressor::new();
        fx.prepare(sr, 512, 2, &params).unwrap();

        let n = 24_000;
        let input = sine(300.0, sr as f32, n, 0.5);
        let input_rms = rms(&input[n / 2..]);

        let mut centre = vec![input.clone(), input.clone()];
        fx.process(&mut centre, n, &params);
        let centre_rms = rms(&centre[0][n / 2..]);
        assert!(
            centre_rms < input_rms * 0.2,
            "centre not suppressed: in={input_rms} out={centre_rms}"
        );

        fx.reset();
        let inverted: Vec<f32> = input.iter().map(|x| -x).collect();
        let mut side = vec![input.clone(), inverted];
        fx.process(&mut side, n, &params);
        let side_rms = rms(&side[0][n / 2..]);
        assert!(
            (side_rms - input_rms).abs() < input_rms * 0.1,
            "side damaged: in={input_rms} out={side_rms}"
        );
    }
}
