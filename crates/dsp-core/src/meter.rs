//! Lock-free meter and telemetry storage.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use serde::Serialize;

pub const SPECTRUM_BINS: usize = 96;

#[derive(Debug, Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub const fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }

    #[inline]
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    #[inline]
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }

    #[inline]
    pub fn fetch_max(&self, v: f32) {
        let mut cur = self.0.load(Ordering::Relaxed);
        loop {
            let cur_f = f32::from_bits(cur);
            if v <= cur_f {
                return;
            }
            match self.0.compare_exchange_weak(
                cur,
                v.to_bits(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => cur = actual,
            }
        }
    }
}

impl Clone for AtomicF32 {
    fn clone(&self) -> Self {
        Self::new(self.get())
    }
}

#[derive(Debug)]
pub struct SlotMeters {
    pub gain_reduction_db: AtomicF32,
    pub input_peak_db: AtomicF32,
    pub output_peak_db: AtomicF32,
    pub latency_frames: AtomicU32,
}

impl Default for SlotMeters {
    fn default() -> Self {
        Self {
            gain_reduction_db: AtomicF32::new(0.0),
            input_peak_db: AtomicF32::new(-120.0),
            output_peak_db: AtomicF32::new(-120.0),
            latency_frames: AtomicU32::new(0),
        }
    }
}

#[derive(Debug)]
pub struct MeterBank {
    pub peak_db: [AtomicF32; 2],
    pub true_peak_db: [AtomicF32; 2],
    pub rms_db: [AtomicF32; 2],
    pub lufs_m: AtomicF32,
    pub lufs_s: AtomicF32,
    pub lufs_i: AtomicF32,
    pub gain_reduction_db: AtomicF32,
    pub dsp_p50_us: AtomicF32,
    pub dsp_p99_us: AtomicF32,
    pub dsp_last_us: AtomicF32,
    pub xruns: AtomicU32,
    pub frames: AtomicU64,
    pub latency_frames: AtomicU32,
    pub sample_rate: AtomicU32,
    pub quantum: AtomicU32,
    pub spectrum: [AtomicF32; SPECTRUM_BINS],
    pub spectrum_seq: AtomicU32,
}

impl Default for MeterBank {
    fn default() -> Self {
        Self {
            peak_db: [AtomicF32::new(-120.0), AtomicF32::new(-120.0)],
            true_peak_db: [AtomicF32::new(-120.0), AtomicF32::new(-120.0)],
            rms_db: [AtomicF32::new(-120.0), AtomicF32::new(-120.0)],
            lufs_m: AtomicF32::new(-120.0),
            lufs_s: AtomicF32::new(-120.0),
            lufs_i: AtomicF32::new(-120.0),
            gain_reduction_db: AtomicF32::new(0.0),
            dsp_p50_us: AtomicF32::new(0.0),
            dsp_p99_us: AtomicF32::new(0.0),
            dsp_last_us: AtomicF32::new(0.0),
            xruns: AtomicU32::new(0),
            frames: AtomicU64::new(0),
            latency_frames: AtomicU32::new(0),
            sample_rate: AtomicU32::new(48_000),
            quantum: AtomicU32::new(128),
            spectrum: std::array::from_fn(|_| AtomicF32::new(-120.0)),
            spectrum_seq: AtomicU32::new(0),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MeterSnapshot {
    pub peak_dbfs: [f32; 2],
    pub true_peak_dbfs: [f32; 2],
    pub rms_dbfs: [f32; 2],
    pub lufs_m: f32,
    pub lufs_s: f32,
    pub lufs_i: f32,
    pub gain_reduction_db: f32,
    pub dsp_p50_us: f32,
    pub dsp_p99_us: f32,
    pub dsp_last_us: f32,
    pub xruns: u32,
    pub frames: u64,
    pub latency_frames: u32,
    pub sample_rate: u32,
    pub quantum: u32,
}

impl MeterBank {
    pub fn snapshot(&self) -> MeterSnapshot {
        MeterSnapshot {
            peak_dbfs: [self.peak_db[0].get(), self.peak_db[1].get()],
            true_peak_dbfs: [self.true_peak_db[0].get(), self.true_peak_db[1].get()],
            rms_dbfs: [self.rms_db[0].get(), self.rms_db[1].get()],
            lufs_m: self.lufs_m.get(),
            lufs_s: self.lufs_s.get(),
            lufs_i: self.lufs_i.get(),
            gain_reduction_db: self.gain_reduction_db.get(),
            dsp_p50_us: self.dsp_p50_us.get(),
            dsp_p99_us: self.dsp_p99_us.get(),
            dsp_last_us: self.dsp_last_us.get(),
            xruns: self.xruns.load(Ordering::Relaxed),
            frames: self.frames.load(Ordering::Relaxed),
            latency_frames: self.latency_frames.load(Ordering::Relaxed),
            sample_rate: self.sample_rate.load(Ordering::Relaxed),
            quantum: self.quantum.load(Ordering::Relaxed),
        }
    }

    pub fn spectrum(&self) -> Vec<f32> {
        let seq = self.spectrum_seq.load(Ordering::Relaxed);
        let bins: Vec<f32> = self.spectrum.iter().map(|b| b.get()).collect();
        let seq2 = self.spectrum_seq.load(Ordering::Relaxed);
        if seq != seq2 {
            // A write raced with the read; the next poll will be coherent.
            return bins;
        }
        bins
    }

    pub fn reset_peaks(&self) {
        for i in 0..2 {
            self.peak_db[i].set(-120.0);
            self.true_peak_db[i].set(-120.0);
        }
    }
}
