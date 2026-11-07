//! Level meter: a pass-through effect that exposes peak / true-peak / loudness
//! telemetry. The actual measurement is done by the pipeline meter bank; this
//! effect only keeps a peak-hold value for the per-slot display.

use dsp_core::envelope::PeakHold;
use dsp_core::param::{ParamStore, ParamValue};
use dsp_core::smoothing::lin_to_db;
use dsp_core::{AudioEffect, EffectError};

pub struct LevelMeter {
    sample_rate: f32,
    hold: [PeakHold; 2],
    ids: [u16; 2],
    peak_db: [f32; 2],
}

impl LevelMeter {
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            hold: [PeakHold::new(), PeakHold::new()],
            ids: [u16::MAX; 2],
            peak_db: [-120.0; 2],
        }
    }
}

impl AudioEffect for LevelMeter {
    fn prepare(
        &mut self,
        sample_rate: u32,
        _max_frames: usize,
        _channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError> {
        self.sample_rate = sample_rate as f32;
        self.ids = [
            params
                .spec_by_key("peak-hold")
                .map(|(i, _)| i)
                .unwrap_or(u16::MAX),
            params
                .spec_by_key("scale")
                .map(|(i, _)| i)
                .unwrap_or(u16::MAX),
        ];
        Ok(())
    }

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, p: &ParamStore) {
        let hold_ms = p.get_f32(self.ids[0]);
        for ch in 0..buf.len().min(2) {
            self.hold[ch].set_hold_ms(self.sample_rate, hold_ms);
            let mut peak = 0.0f32;
            for &x in &buf[ch][..frames] {
                peak = peak.max(self.hold[ch].process(x));
            }
            self.peak_db[ch] = lin_to_db(peak);
        }
    }

    fn reset(&mut self) {
        for h in self.hold.iter_mut() {
            h.reset();
        }
        self.peak_db = [-120.0; 2];
    }

    fn telemetry(&self) -> Option<f32> {
        Some(self.peak_db[0].max(self.peak_db[1]))
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}
