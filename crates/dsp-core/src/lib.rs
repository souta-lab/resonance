//! Resonance DSP core.
//!
//! Design rules enforced by this crate:
//!
//! * The audio callback never allocates, locks (blocking), logs, or performs IO.
//! * Parameters live in [`ParamStore`], a fixed array of atomics shared between
//!   the control thread and the audio thread. The audio thread only reads.
//! * Graph changes are prepared off-thread and handed over through
//!   [`GraphSwap`]. The audio thread uses `try_lock`; if the control thread is
//!   mid-install the audio thread simply keeps the previous graph for one block.
//! * Retired graphs are handed back to the control thread so that the last
//!   reference is never dropped on the audio thread.

#![allow(
    clippy::needless_range_loop,
    clippy::new_without_default,
    clippy::excessive_precision,
    clippy::large_enum_variant,
    clippy::manual_is_multiple_of,
    clippy::vec_box,
    clippy::should_implement_trait
)]

pub mod biquad;
pub mod delay;
pub mod effect;
pub mod envelope;
pub mod graph;
pub mod kind;
pub mod lufs;
pub mod meter;
pub mod param;
pub mod resample;
pub mod smoothing;
pub mod spectrum;

pub use effect::{AudioEffect, EffectError};
pub use graph::{Engine, GraphSwap, Pipeline, PipelineKind, Slot};
pub use kind::EffectKind;
pub use meter::{AtomicF32, MeterBank, MeterSnapshot, SlotMeters};
pub use param::{
    BandField, BandLayout, ParamKind, ParamSpec, ParamStore, ParamValue, Unit, BAND_BASE,
    COMMON_PARAMS, EFFECT_PARAM_BASE, P_BYPASS, P_DRY, P_INPUT_GAIN, P_OUTPUT_GAIN, P_WET,
};
pub use smoothing::{db_to_lin, lin_to_db, Smoothed};

/// Maximum number of channels the engine processes.
pub const MAX_CHANNELS: usize = 2;

/// Default PipeWire quantum used when the host does not tell us otherwise.
pub const DEFAULT_QUANTUM: usize = 128;
