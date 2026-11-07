//! Effect trait and error type.

use crate::param::{ParamStore, ParamValue};

#[derive(Debug, thiserror::Error)]
pub enum EffectError {
    #[error("effect is not prepared")]
    NotPrepared,
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("asset error: {0}")]
    Asset(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// A stereo (or mono) audio effect.
///
/// `process` works in place on planar buffers. Effects that need the dry signal
/// copy it into their own preallocated scratch space during `prepare`.
pub trait AudioEffect: Send {
    /// Called on the control thread before the effect is installed in a graph.
    /// Effects resolve their parameter ids here (see `ParamStore::spec_by_key`)
    /// and allocate all buffers they will need.
    fn prepare(
        &mut self,
        sample_rate: u32,
        max_frames: usize,
        channels: usize,
        params: &ParamStore,
    ) -> Result<(), EffectError>;

    fn process(&mut self, buf: &mut [Vec<f32>], frames: usize, params: &ParamStore);

    fn latency_frames(&self) -> usize {
        0
    }

    fn reset(&mut self);

    /// Current gain reduction in dB (negative), for meters.
    fn gain_reduction_db(&self) -> f32 {
        0.0
    }

    /// Optional per-effect telemetry (e.g. detected pitch).
    fn telemetry(&self) -> Option<f32> {
        None
    }

    /// Called when the effect is removed from the graph so that large assets can
    /// be released on the control thread.
    fn release(&mut self) {}

    /// Load an external asset (impulse response, model) on the control thread.
    /// Effects without assets keep the default implementation.
    fn load_asset(&mut self, _key: &str, _path: &std::path::Path) -> Result<(), EffectError> {
        Err(EffectError::Unsupported(
            "this effect does not load external assets".to_string(),
        ))
    }

    fn set_param(&mut self, _id: u16, _value: ParamValue) -> bool {
        false
    }
}
