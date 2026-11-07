//! Native DSP effects for Project Resonance.
//!
//! Every effect is implemented from public DSP literature; no LSP Plugins or
//! other external plugin binaries are loaded. The catalog records how close
//! each implementation is to the EasyEffects reference.

pub mod catalog;
pub mod dynamics;
pub mod eq;
pub mod filter;
pub mod meter;
pub mod noise;
pub mod space;
pub mod tone;
pub mod voice;

use dsp_core::{AudioEffect, EffectKind};

pub use catalog::{
    all as all_descriptors, descriptor, param_id, AssetKind, AssetSpec, CompatLevel,
    EffectDescriptor, EffectGroup,
};

/// Construct an unprepared effect instance.
pub fn create(kind: EffectKind) -> Box<dyn AudioEffect> {
    match kind {
        EffectKind::Equalizer => Box::new(eq::Equalizer::new(false)),
        EffectKind::MidSideEqualizer => Box::new(eq::Equalizer::new(true)),
        EffectKind::Filter => Box::new(filter::Filter::new()),
        EffectKind::BassEnhancer => Box::new(filter::BassEnhancer::new()),
        EffectKind::BassLoudness => Box::new(filter::BassLoudness::new()),
        EffectKind::Loudness => Box::new(filter::Loudness::new()),
        EffectKind::Compressor => Box::new(dynamics::Compressor::new()),
        EffectKind::Limiter => Box::new(dynamics::Limiter::new()),
        EffectKind::Maximizer => Box::new(dynamics::Maximizer::new()),
        EffectKind::Expander => Box::new(dynamics::Expander::new()),
        EffectKind::Gate => Box::new(dynamics::Gate::new()),
        EffectKind::MultibandCompressor => Box::new(dynamics::MultibandCompressor::new()),
        EffectKind::MultibandGate => Box::new(dynamics::MultibandGate::new()),
        EffectKind::Deesser => Box::new(dynamics::Deesser::new()),
        EffectKind::Autogain => Box::new(dynamics::Autogain::new()),
        EffectKind::Convolver => Box::new(space::Convolver::new()),
        EffectKind::Reverberation => Box::new(space::Reverberation::new()),
        EffectKind::Delay => Box::new(space::Delay::new()),
        EffectKind::Crossfeed => Box::new(space::Crossfeed::new()),
        EffectKind::CrosstalkCanceller => Box::new(space::CrosstalkCanceller::new()),
        EffectKind::StereoTools => Box::new(space::StereoTools::new()),
        EffectKind::Exciter => Box::new(tone::Exciter::new()),
        EffectKind::Crystalizer => Box::new(tone::Crystalizer::new()),
        EffectKind::Crusher => Box::new(tone::Crusher::new()),
        EffectKind::PitchShift => Box::new(voice::PitchShift::new()),
        EffectKind::Autotune => Box::new(voice::Autotune::new()),
        EffectKind::Harmonizer => Box::new(voice::Harmonizer::new()),
        EffectKind::SpeechProcessor => Box::new(voice::SpeechProcessor::new()),
        EffectKind::VoiceSuppressor => Box::new(voice::VoiceSuppressor::new()),
        EffectKind::NoiseReduction => Box::new(noise::NoiseReduction::new()),
        EffectKind::DeepNoiseRemover => Box::new(noise::DeepNoiseRemover::new()),
        EffectKind::EchoCanceller => Box::new(noise::EchoCanceller::new()),
        EffectKind::LevelMeter => Box::new(meter::LevelMeter::new()),
    }
}

/// Build a prepared effect for a pipeline.
pub fn build(
    kind: EffectKind,
    sample_rate: u32,
    max_frames: usize,
    channels: usize,
    params: &dsp_core::ParamStore,
) -> Result<Box<dyn AudioEffect>, dsp_core::EffectError> {
    let mut effect = create(kind);
    effect.prepare(sample_rate, max_frames, channels, params)?;
    Ok(effect)
}
