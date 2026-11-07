//! Stable effect identifiers shared by the DSP engine, preset layer and API.

use serde::{Deserialize, Serialize};

/// Every effect that EasyEffects 8.3.2 ships, plus the analyzer tap.
///
/// The string form is the EasyEffects plugin base name and is used verbatim in
/// preset instance ids (`equalizer#0`) and in the HTTP API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Equalizer,
    MidSideEqualizer,
    Filter,
    BassEnhancer,
    BassLoudness,
    Loudness,
    Compressor,
    Limiter,
    Maximizer,
    Expander,
    Gate,
    MultibandCompressor,
    MultibandGate,
    Deesser,
    Autogain,
    Convolver,
    Reverberation,
    Delay,
    Crossfeed,
    CrosstalkCanceller,
    StereoTools,
    Exciter,
    Crystalizer,
    Crusher,
    PitchShift,
    Autotune,
    Harmonizer,
    SpeechProcessor,
    VoiceSuppressor,
    NoiseReduction,
    DeepNoiseRemover,
    EchoCanceller,
    LevelMeter,
}

impl EffectKind {
    pub const ALL: [EffectKind; 33] = [
        EffectKind::Equalizer,
        EffectKind::MidSideEqualizer,
        EffectKind::Filter,
        EffectKind::BassEnhancer,
        EffectKind::BassLoudness,
        EffectKind::Loudness,
        EffectKind::Compressor,
        EffectKind::Limiter,
        EffectKind::Maximizer,
        EffectKind::Expander,
        EffectKind::Gate,
        EffectKind::MultibandCompressor,
        EffectKind::MultibandGate,
        EffectKind::Deesser,
        EffectKind::Autogain,
        EffectKind::Convolver,
        EffectKind::Reverberation,
        EffectKind::Delay,
        EffectKind::Crossfeed,
        EffectKind::CrosstalkCanceller,
        EffectKind::StereoTools,
        EffectKind::Exciter,
        EffectKind::Crystalizer,
        EffectKind::Crusher,
        EffectKind::PitchShift,
        EffectKind::Autotune,
        EffectKind::Harmonizer,
        EffectKind::SpeechProcessor,
        EffectKind::VoiceSuppressor,
        EffectKind::NoiseReduction,
        EffectKind::DeepNoiseRemover,
        EffectKind::EchoCanceller,
        EffectKind::LevelMeter,
    ];

    /// EasyEffects preset key / API id.
    pub const fn as_str(self) -> &'static str {
        match self {
            EffectKind::Equalizer => "equalizer",
            EffectKind::MidSideEqualizer => "mid_side_equalizer",
            EffectKind::Filter => "filter",
            EffectKind::BassEnhancer => "bass_enhancer",
            EffectKind::BassLoudness => "bass_loudness",
            EffectKind::Loudness => "loudness",
            EffectKind::Compressor => "compressor",
            EffectKind::Limiter => "limiter",
            EffectKind::Maximizer => "maximizer",
            EffectKind::Expander => "expander",
            EffectKind::Gate => "gate",
            EffectKind::MultibandCompressor => "multiband_compressor",
            EffectKind::MultibandGate => "multiband_gate",
            EffectKind::Deesser => "deesser",
            EffectKind::Autogain => "autogain",
            EffectKind::Convolver => "convolver",
            EffectKind::Reverberation => "reverberation",
            EffectKind::Delay => "delay",
            EffectKind::Crossfeed => "crossfeed",
            EffectKind::CrosstalkCanceller => "crosstalk_canceller",
            EffectKind::StereoTools => "stereo_tools",
            EffectKind::Exciter => "exciter",
            EffectKind::Crystalizer => "crystalizer",
            EffectKind::Crusher => "crusher",
            EffectKind::PitchShift => "pitch_shift",
            EffectKind::Autotune => "autotune",
            EffectKind::Harmonizer => "harmonizer",
            EffectKind::SpeechProcessor => "speech_processor",
            EffectKind::VoiceSuppressor => "voice_suppressor",
            EffectKind::NoiseReduction => "noise_reduction",
            EffectKind::DeepNoiseRemover => "deep_noise_remover",
            EffectKind::EchoCanceller => "echo_canceller",
            EffectKind::LevelMeter => "level_meter",
        }
    }

    pub fn parse(s: &str) -> Option<EffectKind> {
        // Accept both the EasyEffects snake_case key and a kebab-case spelling.
        let normalized = s.replace('-', "_");
        EffectKind::ALL
            .into_iter()
            .find(|k| k.as_str() == normalized)
    }

    /// Effects that only observe the signal and never modify it.
    pub const fn is_analyzer(self) -> bool {
        matches!(self, EffectKind::LevelMeter)
    }

    /// Effects that are only meaningful on the input (microphone) pipeline.
    pub const fn input_only(self) -> bool {
        matches!(
            self,
            EffectKind::EchoCanceller
                | EffectKind::VoiceSuppressor
                | EffectKind::SpeechProcessor
                | EffectKind::DeepNoiseRemover
        )
    }
}

impl std::fmt::Display for EffectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
