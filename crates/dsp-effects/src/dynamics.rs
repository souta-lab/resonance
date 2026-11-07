//! Dynamics: compressor, limiter, maximizer, expander, gate, multiband
//! dynamics, de-esser and auto gain.

use dsp_core::biquad::{BiquadKind, Cascade};
use dsp_core::delay::StereoDelay;
use dsp_core::envelope::{Envelope, GainComputer, Rms};
use dsp_core::param::{ParamStore, ParamValue};
use dsp_core::smoothing::{db_to_lin, lin_to_db, Smoothed};
use dsp_core::{AudioEffect, EffectError};

fn id(p: &ParamStore, key: &str) -> u16 {
    p.spec_by_key(key).map(|(i, _)| i).unwrap_or(u16::MAX)
}

fn peak_of(buf: &[Vec<f32>], frames: usize) -> f32 {
    let mut peak = 0.0f32;
    for ch in buf.iter() {
        for &x in &ch[..frames.min(ch.len())] {
            peak = peak.max(x.abs());
        }
    }
    peak
}

// ---------------------------------------------------------------------------
// Compressor
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct CompIds {
    mode: u16,
    threshold: u16,
    ratio: u16,
    knee: u16,
    attack: u16,
    release: u16,
    makeup: u16,
    release_threshold: u16,
    boost_threshold: u16,
    boost_amount: u16,
    hpf_freq: u16,
    hpf_mode: u16,
    lpf_freq: u16,
    lpf_mode: u16,
    lookahead: u16,
    sc_preamp: u16,
    sc_mode: u16,
}

pub struct Compressor {
    sample_rate: f32,
    env: Envelope,
    rms: Rms,
    gain: Smoothed,
    gc: GainComputer,
    ids: CompIds,
    lookahead: StereoDelay,
    lookahead_samples: usize,
    hpf: [Cascade; 2],
    lpf: [Cascade; 2],
    hpf_key: (u16, f32),
    lpf_key: (u16, f32),
    gr_db: f32,
}

impl Compressor {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            env: Envelope::new(),
            rms: Rms::new(),
            gain: Smoothed::new(1.0),
            gc: GainComputer::new(),
            ids: CompIds::default(),
            lookahead: StereoDelay::new(4096),
            lookahead_samples: 0,
            hpf: [Cascade::new(2), Cascade::new(2)],
            lpf: [Cascade::new(2), Cascade::new(2)],
            hpf_key: (u16::MAX, 0.0),
            lpf_key: (u16::MAX, 0.0),
            gr_db: 0.0,
        }
    }
}

impl AudioEffect for Compressor {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = CompIds {
            mode: id(params, "mode"),
            threshold: id(params, "threshold"),
            ratio: id(params, "ratio"),
            knee: id(params, "knee"),
            attack: id(params, "attack"),
            release: id(params, "release"),
            makeup: id(params, "makeup"),
            release_threshold: id(params, "release-threshold"),
            boost_threshold: id(params, "boost-threshold"),
            boost_amount: id(params, "boost-amount"),
            hpf_freq: id(params, "hpf-frequency"),
            hpf_mode: id(params, "hpf-mode"),
            lpf_freq: id(params, "lpf-frequency"),
            lpf_mode: id(params, "lpf-mode"),
            lookahead: id(params, "lookahead"),
            sc_preamp: id(params, "sidechain-preamp"),
            sc_mode: id(params, "sidechain-mode"),
        };
        self.lookahead = StereoDelay::new((sample_rate as usize / 10).max(64));
        self.gain.set_time_ms(self.sample_rate, 5.0);
        self.rms.set_window_ms(self.sample_rate, 10.0);
        let lookahead_ms = params.get_f32(self.ids.lookahead);
        self.lookahead_samples = ((lookahead_ms * self.sample_rate / 1000.0) as usize)
            .min(self.lookahead.l.capacity() - 4);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let frames = frames.min(buf[0].len());
        let mode = p.get_enum(self.ids.mode);
        let threshold = p.get_f32(self.ids.threshold);
        let ratio = p.get_f32(self.ids.ratio).max(1.0);
        let knee = p.get_f32(self.ids.knee);
        let attack = p.get_f32(self.ids.attack);
        let release = p.get_f32(self.ids.release);
        let makeup = p.get_f32(self.ids.makeup);
        let release_threshold = p.get_f32(self.ids.release_threshold);
        let boost_threshold = p.get_f32(self.ids.boost_threshold);
        let boost_amount = p.get_f32(self.ids.boost_amount);
        let lookahead_ms = p.get_f32(self.ids.lookahead);
        let sc_preamp = p.get_f32(self.ids.sc_preamp);
        let use_rms = p.get_enum(self.ids.sc_mode) == 1;

        self.env.set_times(self.sample_rate, attack, release);
        self.gc.threshold_db = threshold;
        self.gc.ratio = ratio;
        self.gc.knee_db = knee;
        self.gc.makeup_db = 0.0;

        let hpf_mode = p.get_enum(self.ids.hpf_mode);
        let hpf_freq = p.get_f32(self.ids.hpf_freq);
        if hpf_mode > 0 && self.hpf_key != (hpf_mode, hpf_freq) {
            let order = if hpf_mode == 1 { 2 } else { 4 };
            for ch in 0..2 {
                self.hpf[ch] = Cascade::new(order);
                self.hpf[ch].set(BiquadKind::HighPass, self.sample_rate, hpf_freq, 0.0);
            }
            self.hpf_key = (hpf_mode, hpf_freq);
        }
        let lpf_mode = p.get_enum(self.ids.lpf_mode);
        let lpf_freq = p.get_f32(self.ids.lpf_freq);
        if lpf_mode > 0 && self.lpf_key != (lpf_mode, lpf_freq) {
            let order = if lpf_mode == 1 { 2 } else { 4 };
            for ch in 0..2 {
                self.lpf[ch] = Cascade::new(order);
                self.lpf[ch].set(BiquadKind::LowPass, self.sample_rate, lpf_freq, 0.0);
            }
            self.lpf_key = (lpf_mode, lpf_freq);
        }

        let la_samples = (lookahead_ms * self.sample_rate / 1000.0) as usize;
        self.lookahead_samples = la_samples.min(self.lookahead.l.capacity() - 4);

        let makeup_lin = db_to_lin(makeup);
        let chans = buf.len().min(2);
        let mut gr_sum = 0.0f32;

        for i in 0..frames {
            let mut sc = 0.0f32;
            for ch in 0..chans {
                let mut x = buf[ch][i];
                if hpf_mode > 0 {
                    x = self.hpf[ch].process(ch, x);
                }
                if lpf_mode > 0 {
                    x = self.lpf[ch].process(ch, x);
                }
                sc = sc.max(x.abs());
            }
            let det = if use_rms { self.rms.process(sc) } else { sc };
            let env = self.env.process(det);
            let env_db = lin_to_db(env) + sc_preamp;

            let gr = match mode {
                1 => {
                    // Upward compression: lift quiet material.
                    let under = (threshold - env_db).max(0.0);
                    (under * (1.0 - 1.0 / ratio)).min(24.0)
                }
                2 => {
                    let down = self.gc.compute(env_db);
                    let under = (boost_threshold - env_db).max(0.0);
                    down + (under * (1.0 - 1.0 / ratio)).min(boost_amount)
                }
                _ => {
                    let mut g = self.gc.compute(env_db);
                    if env_db < release_threshold {
                        // Faster release below the release threshold.
                        g *= 0.999;
                    }
                    g
                }
            };
            gr_sum += gr;
            self.gain.set_target(db_to_lin(gr));
            let g = self.gain.next();

            for ch in 0..chans {
                let x = buf[ch][i];
                let delayed = if self.lookahead_samples > 0 {
                    let line = if ch == 0 {
                        &mut self.lookahead.l
                    } else {
                        &mut self.lookahead.r
                    };
                    line.write(x);
                    line.read(self.lookahead_samples as f32)
                } else {
                    x
                };
                buf[ch][i] = delayed * g * makeup_lin;
            }
        }
        self.gr_db = if frames > 0 {
            gr_sum / frames as f32
        } else {
            0.0
        };
    }

    fn latency_frames(&self) -> usize {
        self.lookahead_samples
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    fn reset(&mut self) {
        self.env.reset();
        self.rms.reset();
        self.gain.snap(1.0);
        self.lookahead.clear();
        for c in self.hpf.iter_mut().chain(self.lpf.iter_mut()) {
            c.reset();
        }
        self.gr_db = 0.0;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Limiter
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct LimiterIds {
    threshold: u16,
    lookahead: u16,
    attack: u16,
    release: u16,
    gain_boost: u16,
}

pub struct Limiter {
    sample_rate: f32,
    ids: LimiterIds,
    delay: StereoDelay,
    delay_samples: usize,
    gain: Smoothed,
    ceiling: f32,
    gr_db: f32,
    last_peak: f32,
}

impl Limiter {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            ids: LimiterIds::default(),
            delay: StereoDelay::new(8192),
            delay_samples: 0,
            gain: Smoothed::new(1.0),
            ceiling: 1.0,
            gr_db: 0.0,
            last_peak: 0.0,
        }
    }
}

impl AudioEffect for Limiter {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = LimiterIds {
            threshold: id(params, "threshold"),
            lookahead: id(params, "lookahead"),
            attack: id(params, "attack"),
            release: id(params, "release"),
            gain_boost: id(params, "gain-boost"),
        };
        self.delay = StereoDelay::new((sample_rate as usize / 10).max(64));
        self.gain.set_time_ms(self.sample_rate, 2.0);
        let lookahead_ms = params.get_f32(self.ids.lookahead);
        self.delay_samples =
            ((lookahead_ms * self.sample_rate / 1000.0) as usize).min(self.delay.l.capacity() - 4);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let frames = frames.min(buf[0].len());
        let threshold_db = p.get_f32(self.ids.threshold);
        let lookahead_ms = p.get_f32(self.ids.lookahead);
        let attack = p.get_f32(self.ids.attack);
        let release = p.get_f32(self.ids.release);
        let gain_boost = p.get_bool(self.ids.gain_boost);
        self.ceiling = db_to_lin(threshold_db);
        self.gain.set_time_ms(self.sample_rate, attack.max(0.1));

        let la = (lookahead_ms * self.sample_rate / 1000.0) as usize;
        self.delay_samples = la.min(self.delay.l.capacity() - 4);

        // Peak of the incoming block, with 4x linear interpolation for a
        // true-peak-ish estimate.
        let mut peak = 0.0f32;
        for ch in 0..buf.len().min(2) {
            let mut last = 0.0f32;
            for &x in &buf[ch][..frames] {
                peak = peak.max(x.abs());
                for k in 1..4 {
                    let v = last + (x - last) * (k as f32 * 0.25);
                    peak = peak.max(v.abs());
                }
                last = x;
            }
        }
        self.last_peak = peak;

        let target = if peak > self.ceiling && peak > 1e-9 {
            self.ceiling / peak
        } else {
            1.0
        };
        // Attack is fast, release is slow.
        self.gain.set_target(target);
        let mut gr_sum = 0.0f32;
        let chans = buf.len().min(2);
        for i in 0..frames {
            let g = self.gain.next();
            gr_sum += lin_to_db(g);
            for ch in 0..chans {
                let x = buf[ch][i];
                let delayed = if self.delay_samples > 0 {
                    let line = if ch == 0 {
                        &mut self.delay.l
                    } else {
                        &mut self.delay.r
                    };
                    line.write(x);
                    line.read(self.delay_samples as f32)
                } else {
                    x
                };
                let mut y = delayed * g;
                if gain_boost {
                    y *= 1.0 / self.ceiling.max(1e-6);
                }
                // Brickwall safety clip.
                y = y.clamp(-self.ceiling, self.ceiling);
                buf[ch][i] = y;
            }
        }
        self.gr_db = if frames > 0 {
            gr_sum / frames as f32
        } else {
            0.0
        };
        let _ = release;
    }

    fn latency_frames(&self) -> usize {
        self.delay_samples
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    fn reset(&mut self) {
        self.delay.clear();
        self.gain.snap(1.0);
        self.gr_db = 0.0;
        self.last_peak = 0.0;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Maximizer
// ---------------------------------------------------------------------------

pub struct Maximizer {
    sample_rate: f32,
    ids: [u16; 4],
    gain: Smoothed,
    ceiling: f32,
    gr_db: f32,
}

impl Maximizer {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            ids: [u16::MAX; 4],
            gain: Smoothed::new(1.0),
            ceiling: 1.0,
            gr_db: 0.0,
        }
    }
}

impl AudioEffect for Maximizer {
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
            id(params, "ceiling"),
            id(params, "release"),
            id(params, "oversampling"),
        ];
        self.gain.set_time_ms(self.sample_rate, 20.0);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        let threshold = db_to_lin(p.get_f32(self.ids[0]));
        let ceiling = db_to_lin(p.get_f32(self.ids[1]));
        let release = p.get_f32(self.ids[2]);
        self.ceiling = ceiling;
        self.gain.set_time_ms(self.sample_rate, release.max(1.0));
        let peak = peak_of(buf, frames).max(1e-9);
        // Upward gain to reach the threshold, then a ceiling clamp.
        let target = (threshold / peak).clamp(0.25, 8.0);
        self.gain.set_target(target);
        let mut gr_sum = 0.0;
        for i in 0..frames {
            let g = self.gain.next();
            gr_sum += lin_to_db(g);
            for ch in 0..buf.len().min(2) {
                let y = buf[ch][i] * g;
                buf[ch][i] = y.clamp(-ceiling, ceiling);
            }
        }
        self.gr_db = if frames > 0 {
            gr_sum / frames as f32
        } else {
            0.0
        };
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    fn reset(&mut self) {
        self.gain.snap(1.0);
        self.gr_db = 0.0;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Expander
// ---------------------------------------------------------------------------

pub struct Expander {
    sample_rate: f32,
    ids: [u16; 8],
    env: Envelope,
    rms: Rms,
    gain: Smoothed,
    gr_db: f32,
}

impl Expander {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            ids: [u16::MAX; 8],
            env: Envelope::new(),
            rms: Rms::new(),
            gain: Smoothed::new(1.0),
            gr_db: 0.0,
        }
    }
}

impl AudioEffect for Expander {
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
            id(params, "knee"),
            id(params, "makeup"),
            id(params, "detector"),
            id(params, "stereo-link"),
        ];
        self.rms.set_window_ms(self.sample_rate, 10.0);
        self.gain.set_time_ms(self.sample_rate, 5.0);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        let threshold = p.get_f32(self.ids[0]);
        let ratio = p.get_f32(self.ids[1]).max(1.0);
        let attack = p.get_f32(self.ids[2]);
        let release = p.get_f32(self.ids[3]);
        let knee = p.get_f32(self.ids[4]);
        let makeup = db_to_lin(p.get_f32(self.ids[5]));
        let use_rms = p.get_enum(self.ids[6]) == 1;
        self.env.set_times(self.sample_rate, attack, release);
        let mut gr_sum = 0.0;
        for i in 0..frames {
            let mut sc = 0.0f32;
            for ch in 0..buf.len().min(2) {
                sc = sc.max(buf[ch][i].abs());
            }
            let det = if use_rms { self.rms.process(sc) } else { sc };
            let env = self.env.process(det);
            let env_db = lin_to_db(env);
            let under = (threshold - env_db).max(0.0);
            let gr = if under <= knee * 0.5 {
                -under * (1.0 - 1.0 / ratio) * (under / knee.max(1e-3)) * 0.5
            } else {
                -under * (1.0 - 1.0 / ratio)
            };
            gr_sum += gr;
            self.gain.set_target(db_to_lin(gr));
            let g = self.gain.next() * makeup;
            for ch in 0..buf.len().min(2) {
                buf[ch][i] *= g;
            }
        }
        self.gr_db = if frames > 0 {
            gr_sum / frames as f32
        } else {
            0.0
        };
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    fn reset(&mut self) {
        self.env.reset();
        self.rms.reset();
        self.gain.snap(1.0);
        self.gr_db = 0.0;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Gate
// ---------------------------------------------------------------------------

pub struct Gate {
    sample_rate: f32,
    ids: [u16; 8],
    env: Envelope,
    gain: Smoothed,
    hold_counter: usize,
    open: bool,
    gr_db: f32,
}

impl Gate {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            ids: [u16::MAX; 8],
            env: Envelope::new(),
            gain: Smoothed::new(0.0),
            hold_counter: 0,
            open: false,
            gr_db: 0.0,
        }
    }
}

impl AudioEffect for Gate {
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
            id(params, "attack"),
            id(params, "release"),
            id(params, "hold"),
            id(params, "range"),
            id(params, "knee"),
            id(params, "hysteresis"),
            id(params, "stereo-link"),
        ];
        self.gain.set_time_ms(self.sample_rate, 2.0);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        let threshold = p.get_f32(self.ids[0]);
        let attack = p.get_f32(self.ids[1]);
        let release = p.get_f32(self.ids[2]);
        let hold = p.get_f32(self.ids[3]);
        let range = p.get_f32(self.ids[4]);
        let hysteresis = p.get_f32(self.ids[6]);
        self.env.set_times(self.sample_rate, attack, release);
        let hold_samples = (hold * self.sample_rate / 1000.0) as usize;
        let floor = db_to_lin(range);
        let mut gr_sum = 0.0;
        for i in 0..frames {
            let mut sc = 0.0f32;
            for ch in 0..buf.len().min(2) {
                sc = sc.max(buf[ch][i].abs());
            }
            let env_db = lin_to_db(self.env.process(sc));
            if env_db > threshold {
                self.open = true;
                self.hold_counter = hold_samples;
            } else if env_db < threshold - hysteresis {
                if self.hold_counter > 0 {
                    self.hold_counter -= 1;
                } else {
                    self.open = false;
                }
            }
            let target = if self.open { 1.0 } else { floor };
            self.gain.set_target(target);
            let g = self.gain.next();
            gr_sum += lin_to_db(g);
            for ch in 0..buf.len().min(2) {
                buf[ch][i] *= g;
            }
        }
        self.gr_db = if frames > 0 {
            gr_sum / frames as f32
        } else {
            0.0
        };
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    fn reset(&mut self) {
        self.env.reset();
        self.gain.snap(0.0);
        self.hold_counter = 0;
        self.open = false;
        self.gr_db = 0.0;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Multiband split
// ---------------------------------------------------------------------------

struct Crossover {
    lp: Cascade,
    hp: Cascade,
}

impl Crossover {
    fn new() -> Self {
        Self {
            lp: Cascade::new(4),
            hp: Cascade::new(4),
        }
    }

    fn set(&mut self, sample_rate: f32, freq: f32) {
        self.lp.set(BiquadKind::LowPass, sample_rate, freq, 0.0);
        self.hp.set(BiquadKind::HighPass, sample_rate, freq, 0.0);
    }

    fn split(&mut self, ch: usize, x: f32) -> (f32, f32) {
        (self.lp.process(ch, x), self.hp.process(ch, x))
    }

    fn reset(&mut self) {
        self.lp.reset();
        self.hp.reset();
    }
}

struct BandDynamics {
    env: Envelope,
    gain: Smoothed,
    gc: GainComputer,
}

impl BandDynamics {
    fn new() -> Self {
        Self {
            env: Envelope::new(),
            gain: Smoothed::new(1.0),
            gc: GainComputer::new(),
        }
    }
}

pub struct MultibandCompressor {
    sample_rate: f32,
    ids: Vec<u16>,
    crossovers: [Crossover; 3],
    bands: [BandDynamics; 4],
    buffers: Vec<Vec<Vec<f32>>>,
    gr_db: f32,
}

impl MultibandCompressor {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            ids: Vec::new(),
            crossovers: [Crossover::new(), Crossover::new(), Crossover::new()],
            bands: std::array::from_fn(|_| BandDynamics::new()),
            buffers: Vec::new(),
            gr_db: 0.0,
        }
    }
}

impl AudioEffect for MultibandCompressor {
    fn prepare(
        &mut self,
        sample_rate: u32,
        max_frames: usize,
        channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = vec![
            id(params, "crossover-1"),
            id(params, "crossover-2"),
            id(params, "crossover-3"),
        ];
        for b in 0..4 {
            for key in ["threshold", "ratio", "attack", "release", "makeup", "knee"] {
                self.ids.push(id(params, &format!("band{b}-{key}")));
            }
        }
        self.buffers = vec![vec![vec![0.0; max_frames.max(16)]; channels.clamp(1, 2)]; 4];
        for band in self.bands.iter_mut() {
            band.gain.set_time_ms(self.sample_rate, 5.0);
        }
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let frames = frames.min(buf[0].len());
        let chans = buf.len().min(2);
        let f1 = p.get_f32(self.ids[0]);
        let f2 = p.get_f32(self.ids[1]);
        let f3 = p.get_f32(self.ids[2]);
        for xo in self.crossovers.iter_mut() {
            xo.reset();
        }
        self.crossovers[0].set(self.sample_rate, f1);
        self.crossovers[1].set(self.sample_rate, f2);
        self.crossovers[2].set(self.sample_rate, f3);

        for ch in 0..chans {
            for i in 0..frames {
                let x = buf[ch][i];
                let (low, rest) = self.crossovers[0].split(ch, x);
                let (mid1, rest2) = self.crossovers[1].split(ch, rest);
                let (mid2, high) = self.crossovers[2].split(ch, rest2);
                self.buffers[0][ch][i] = low;
                self.buffers[1][ch][i] = mid1;
                self.buffers[2][ch][i] = mid2;
                self.buffers[3][ch][i] = high;
            }
        }

        let mut gr_sum = 0.0;
        for b in 0..4 {
            let base = 3 + b * 6;
            let threshold = p.get_f32(self.ids[base]);
            let ratio = p.get_f32(self.ids[base + 1]).max(1.0);
            let attack = p.get_f32(self.ids[base + 2]);
            let release = p.get_f32(self.ids[base + 3]);
            let makeup = db_to_lin(p.get_f32(self.ids[base + 4]));
            let knee = p.get_f32(self.ids[base + 5]);
            let band = &mut self.bands[b];
            band.env.set_times(self.sample_rate, attack, release);
            band.gc.threshold_db = threshold;
            band.gc.ratio = ratio;
            band.gc.knee_db = knee;
            band.gc.makeup_db = 0.0;
            for i in 0..frames {
                let mut sc = 0.0f32;
                for ch in 0..chans {
                    sc = sc.max(self.buffers[b][ch][i].abs());
                }
                let env = band.env.process(sc);
                let gr = band.gc.compute(lin_to_db(env));
                gr_sum += gr;
                band.gain.set_target(db_to_lin(gr));
                let g = band.gain.next() * makeup;
                for ch in 0..chans {
                    self.buffers[b][ch][i] *= g;
                }
            }
        }

        for ch in 0..chans {
            for i in 0..frames {
                buf[ch][i] = self.buffers[0][ch][i]
                    + self.buffers[1][ch][i]
                    + self.buffers[2][ch][i]
                    + self.buffers[3][ch][i];
            }
        }
        self.gr_db = gr_sum / (frames.max(1) * 4) as f32;
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    fn reset(&mut self) {
        for xo in self.crossovers.iter_mut() {
            xo.reset();
        }
        for band in self.bands.iter_mut() {
            band.env.reset();
            band.gain.snap(1.0);
        }
        self.gr_db = 0.0;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

pub struct MultibandGate {
    sample_rate: f32,
    ids: Vec<u16>,
    crossovers: [Crossover; 3],
    envs: [Envelope; 4],
    gains: [Smoothed; 4],
    buffers: Vec<Vec<Vec<f32>>>,
    gr_db: f32,
}

impl MultibandGate {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            ids: Vec::new(),
            crossovers: [Crossover::new(), Crossover::new(), Crossover::new()],
            envs: std::array::from_fn(|_| Envelope::new()),
            gains: std::array::from_fn(|_| Smoothed::new(0.0)),
            buffers: Vec::new(),
            gr_db: 0.0,
        }
    }
}

impl AudioEffect for MultibandGate {
    fn prepare(
        &mut self,
        sample_rate: u32,
        max_frames: usize,
        channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = vec![
            id(params, "crossover-1"),
            id(params, "crossover-2"),
            id(params, "crossover-3"),
        ];
        for b in 0..4 {
            for key in ["threshold", "attack", "release", "range"] {
                self.ids.push(id(params, &format!("band{b}-{key}")));
            }
        }
        self.buffers = vec![vec![vec![0.0; max_frames.max(16)]; channels.clamp(1, 2)]; 4];
        for g in self.gains.iter_mut() {
            g.set_time_ms(self.sample_rate, 2.0);
        }
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        if buf.is_empty() {
            return;
        }
        let frames = frames.min(buf[0].len());
        let chans = buf.len().min(2);
        for xo in self.crossovers.iter_mut() {
            xo.reset();
        }
        self.crossovers[0].set(self.sample_rate, p.get_f32(self.ids[0]));
        self.crossovers[1].set(self.sample_rate, p.get_f32(self.ids[1]));
        self.crossovers[2].set(self.sample_rate, p.get_f32(self.ids[2]));
        for ch in 0..chans {
            for i in 0..frames {
                let x = buf[ch][i];
                let (low, rest) = self.crossovers[0].split(ch, x);
                let (mid1, rest2) = self.crossovers[1].split(ch, rest);
                let (mid2, high) = self.crossovers[2].split(ch, rest2);
                self.buffers[0][ch][i] = low;
                self.buffers[1][ch][i] = mid1;
                self.buffers[2][ch][i] = mid2;
                self.buffers[3][ch][i] = high;
            }
        }
        let mut gr_sum = 0.0;
        for b in 0..4 {
            let base = 3 + b * 4;
            let threshold = p.get_f32(self.ids[base]);
            let attack = p.get_f32(self.ids[base + 1]);
            let release = p.get_f32(self.ids[base + 2]);
            let range = db_to_lin(p.get_f32(self.ids[base + 3]));
            self.envs[b].set_times(self.sample_rate, attack, release);
            for i in 0..frames {
                let mut sc = 0.0f32;
                for ch in 0..chans {
                    sc = sc.max(self.buffers[b][ch][i].abs());
                }
                let env_db = lin_to_db(self.envs[b].process(sc));
                let target = if env_db > threshold { 1.0 } else { range };
                self.gains[b].set_target(target);
                let g = self.gains[b].next();
                gr_sum += lin_to_db(g);
                for ch in 0..chans {
                    self.buffers[b][ch][i] *= g;
                }
            }
        }
        for ch in 0..chans {
            for i in 0..frames {
                buf[ch][i] = self.buffers[0][ch][i]
                    + self.buffers[1][ch][i]
                    + self.buffers[2][ch][i]
                    + self.buffers[3][ch][i];
            }
        }
        self.gr_db = gr_sum / (frames.max(1) * 4) as f32;
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    fn reset(&mut self) {
        for xo in self.crossovers.iter_mut() {
            xo.reset();
        }
        for e in self.envs.iter_mut() {
            e.reset();
        }
        for g in self.gains.iter_mut() {
            g.snap(0.0);
        }
        self.gr_db = 0.0;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// De-esser
// ---------------------------------------------------------------------------

pub struct Deesser {
    sample_rate: f32,
    ids: [u16; 8],
    lp: [Cascade; 2],
    hp: [Cascade; 2],
    env: Envelope,
    gain: Smoothed,
    gr_db: f32,
}

impl Deesser {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            ids: [u16::MAX; 8],
            lp: [Cascade::new(4), Cascade::new(4)],
            hp: [Cascade::new(4), Cascade::new(4)],
            env: Envelope::new(),
            gain: Smoothed::new(1.0),
            gr_db: 0.0,
        }
    }
}

impl AudioEffect for Deesser {
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
            id(params, "makeup"),
            id(params, "attack"),
            id(params, "release"),
            id(params, "detector"),
            id(params, "mode"),
            id(params, "split-frequency"),
        ];
        self.gain.set_time_ms(self.sample_rate, 3.0);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        let threshold = p.get_f32(self.ids[0]);
        let ratio = p.get_f32(self.ids[1]).max(1.0);
        let makeup = db_to_lin(p.get_f32(self.ids[2]));
        let attack = p.get_f32(self.ids[3]);
        let release = p.get_f32(self.ids[4]);
        let split_mode = p.get_enum(self.ids[6]) == 1;
        let split_freq = p.get_f32(self.ids[7]);
        self.env.set_times(self.sample_rate, attack, release);
        for ch in 0..2 {
            self.lp[ch].set(BiquadKind::LowPass, self.sample_rate, split_freq, 0.0);
            self.hp[ch].set(BiquadKind::HighPass, self.sample_rate, split_freq, 0.0);
        }
        let mut gr_sum = 0.0;
        for i in 0..frames {
            let mut sc = 0.0f32;
            for ch in 0..buf.len().min(2) {
                let high = self.hp[ch].process(ch, buf[ch][i]);
                sc = sc.max(high.abs());
            }
            let env_db = lin_to_db(self.env.process(sc));
            let over = (env_db - threshold).max(0.0);
            let gr = -over * (1.0 - 1.0 / ratio);
            gr_sum += gr;
            self.gain.set_target(db_to_lin(gr));
            let g = self.gain.next() * makeup;
            for ch in 0..buf.len().min(2) {
                let x = buf[ch][i];
                let low = self.lp[ch].process(ch, x);
                let high = self.hp[ch].process(ch, x);
                buf[ch][i] = if split_mode { low + high * g } else { x * g };
            }
        }
        self.gr_db = if frames > 0 {
            gr_sum / frames as f32
        } else {
            0.0
        };
    }

    fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    fn reset(&mut self) {
        for c in self.lp.iter_mut().chain(self.hp.iter_mut()) {
            c.reset();
        }
        self.env.reset();
        self.gain.snap(1.0);
        self.gr_db = 0.0;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Auto gain
// ---------------------------------------------------------------------------

pub struct Autogain {
    sample_rate: f32,
    ids: [u16; 4],
    rms: Rms,
    gain: Smoothed,
    last_gain_db: f32,
}

impl Autogain {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            ids: [u16::MAX; 4],
            rms: Rms::new(),
            gain: Smoothed::new(1.0),
            last_gain_db: 0.0,
        }
    }
}

impl AudioEffect for Autogain {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [
            id(params, "target"),
            id(params, "silence-threshold"),
            id(params, "maximum-gain"),
            id(params, "fallback"),
        ];
        self.rms.set_window_ms(self.sample_rate, 300.0);
        self.gain.set_time_ms(self.sample_rate, 500.0);
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        let target = p.get_f32(self.ids[0]);
        let silence = p.get_f32(self.ids[1]);
        let max_gain = p.get_f32(self.ids[2]);
        let mut sum = 0.0f64;
        let mut n = 0usize;
        for ch in 0..buf.len().min(2) {
            for &x in &buf[ch][..frames] {
                sum += (x as f64) * (x as f64);
                n += 1;
            }
        }
        let rms = if n > 0 {
            (sum / n as f64).sqrt() as f32
        } else {
            0.0
        };
        let level_db = lin_to_db(rms);
        let target_gain_db = if level_db > silence {
            (target - level_db).clamp(-max_gain, max_gain)
        } else {
            self.last_gain_db
        };
        self.last_gain_db = target_gain_db;
        self.gain.set_target(db_to_lin(target_gain_db));
        for i in 0..frames {
            let g = self.gain.next();
            for ch in 0..buf.len().min(2) {
                buf[ch][i] *= g;
            }
        }
    }

    fn reset(&mut self) {
        self.rms.reset();
        self.gain.snap(1.0);
        self.last_gain_db = 0.0;
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}
