//! Machine-readable effect catalog: names, groups, parameter schemas, band
//! layouts, asset requirements and the compatibility level we claim for each
//! effect.
//!
//! The catalog is the single source of truth for the HTTP API, the WebUI and
//! the preset layer. Parameter ids are *not* part of the catalog: effects
//! resolve their ids by key at `prepare` time so that the order of this table
//! can change without breaking DSP code.

use dsp_core::param::{BandField, BandLayout, ParamKind, ParamSpec, Unit};
use dsp_core::EffectKind;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectGroup {
    Eq,
    Dynamics,
    Space,
    Tone,
    Voice,
    Noise,
    Meter,
}

impl EffectGroup {
    pub const fn as_str(self) -> &'static str {
        match self {
            EffectGroup::Eq => "eq",
            EffectGroup::Dynamics => "dynamics",
            EffectGroup::Space => "space",
            EffectGroup::Tone => "tone",
            EffectGroup::Voice => "voice",
            EffectGroup::Noise => "noise",
            EffectGroup::Meter => "meter",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            EffectGroup::Eq => "EQ & filters",
            EffectGroup::Dynamics => "Dynamics",
            EffectGroup::Space => "Space & time",
            EffectGroup::Tone => "Tone shaping",
            EffectGroup::Voice => "Voice & pitch",
            EffectGroup::Noise => "Noise & echo",
            EffectGroup::Meter => "Measurement",
        }
    }
}

/// How close our DSP is to the EasyEffects/LSP reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatLevel {
    /// Same algorithm class and parameters; acoustic comparison expected to
    /// match within tight tolerances.
    Exact,
    /// Same controls and broadly the same behaviour, different implementation
    /// details (detector, oversampling, ...).
    Approximate,
    /// Model-free algorithmic substitute for a model-based upstream effect.
    Substitute,
    /// Measurement only.
    Meter,
}

impl CompatLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            CompatLevel::Exact => "exact",
            CompatLevel::Approximate => "approximate",
            CompatLevel::Substitute => "substitute",
            CompatLevel::Meter => "meter",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    ImpulseResponse,
    Model,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AssetSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: AssetKind,
    pub extensions: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct EffectDescriptor {
    pub kind: EffectKind,
    pub name: &'static str,
    pub group: EffectGroup,
    pub summary: &'static str,
    pub compat: CompatLevel,
    /// `"zero"`, `"block"` (one processing block) or `"adaptive"`.
    pub latency: &'static str,
    pub params: &'static [ParamSpec],
    pub bands: Option<BandLayout>,
    pub assets: &'static [AssetSpec],
}

macro_rules! pf {
    ($key:literal, $label:literal, $group:literal, $min:expr, $max:expr, $default:expr, $step:expr, $unit:expr) => {
        ParamSpec {
            key: $key,
            label: $label,
            group: $group,
            kind: ParamKind::Float {
                min: $min,
                max: $max,
                default: $default,
                step: $step,
                unit: $unit,
                log: false,
            },
            advanced: false,
        }
    };
    ($key:literal, $label:literal, $group:literal, $min:expr, $max:expr, $default:expr, $step:expr, $unit:expr, log) => {
        ParamSpec {
            key: $key,
            label: $label,
            group: $group,
            kind: ParamKind::Float {
                min: $min,
                max: $max,
                default: $default,
                step: $step,
                unit: $unit,
                log: true,
            },
            advanced: false,
        }
    };
    ($key:literal, $label:literal, $group:literal, $min:expr, $max:expr, $default:expr, $step:expr, $unit:expr, adv) => {
        ParamSpec {
            key: $key,
            label: $label,
            group: $group,
            kind: ParamKind::Float {
                min: $min,
                max: $max,
                default: $default,
                step: $step,
                unit: $unit,
                log: false,
            },
            advanced: true,
        }
    };
}

macro_rules! pb {
    ($key:literal, $label:literal, $group:literal, $default:expr) => {
        ParamSpec {
            key: $key,
            label: $label,
            group: $group,
            kind: ParamKind::Bool { default: $default },
            advanced: false,
        }
    };
}

macro_rules! pe {
    ($key:literal, $label:literal, $group:literal, $default:expr, $labels:expr) => {
        ParamSpec {
            key: $key,
            label: $label,
            group: $group,
            kind: ParamKind::Enum {
                default: $default,
                labels: $labels,
            },
            advanced: false,
        }
    };
}

macro_rules! pi {
    ($key:literal, $label:literal, $group:literal, $min:expr, $max:expr, $default:expr, $unit:expr) => {
        ParamSpec {
            key: $key,
            label: $label,
            group: $group,
            kind: ParamKind::Int {
                min: $min,
                max: $max,
                default: $default,
                unit: $unit,
            },
            advanced: false,
        }
    };
}

const BAND_TYPES: &[&str] = &[
    "Bell",
    "Lo-shelf",
    "Hi-shelf",
    "Lo-pass",
    "Hi-pass",
    "Notch",
    "Band-pass",
    "All-pass",
];

static EQ_BAND_FIELDS: [BandField; 8] = [
    BandField {
        key: "type",
        label: "Type",
        kind: ParamKind::Enum {
            default: 0,
            labels: BAND_TYPES,
        },
    },
    BandField {
        key: "frequency",
        label: "Frequency",
        kind: ParamKind::Float {
            min: 10.0,
            max: 24_000.0,
            default: 1_000.0,
            step: 1.0,
            unit: Unit::Hz,
            log: true,
        },
    },
    BandField {
        key: "gain",
        label: "Gain",
        kind: ParamKind::Float {
            min: -36.0,
            max: 36.0,
            default: 0.0,
            step: 0.1,
            unit: Unit::Db,
            log: false,
        },
    },
    BandField {
        key: "q",
        label: "Q",
        kind: ParamKind::Float {
            min: 0.1,
            max: 10.0,
            default: 1.0,
            step: 0.01,
            unit: Unit::None,
            log: true,
        },
    },
    BandField {
        key: "width",
        label: "Width",
        kind: ParamKind::Float {
            min: 0.0,
            max: 10.0,
            default: 4.0,
            step: 0.1,
            unit: Unit::Octaves,
            log: false,
        },
    },
    BandField {
        key: "slope",
        label: "Slope",
        kind: ParamKind::Float {
            min: 0.0,
            max: 10.0,
            default: 0.0,
            step: 0.1,
            unit: Unit::None,
            log: false,
        },
    },
    BandField {
        key: "solo",
        label: "Solo",
        kind: ParamKind::Bool { default: false },
    },
    BandField {
        key: "mute",
        label: "Mute",
        kind: ParamKind::Bool { default: false },
    },
];

static EQ_BANDS: BandLayout = BandLayout {
    max_bands: 32,
    channels: 2,
    fields: &EQ_BAND_FIELDS,
    channel_labels: &["left", "right"],
};

static MS_BANDS: BandLayout = BandLayout {
    max_bands: 32,
    channels: 2,
    fields: &EQ_BAND_FIELDS,
    channel_labels: &["mid", "side"],
};

static IR_ASSET: [AssetSpec; 1] = [AssetSpec {
    key: "kernel-name",
    label: "Impulse response",
    kind: AssetKind::ImpulseResponse,
    extensions: &["irs", "wav"],
}];

static MODEL_ASSET: [AssetSpec; 1] = [AssetSpec {
    key: "model-name",
    label: "Noise model",
    kind: AssetKind::Model,
    extensions: &["onnx", "rnnn", "bin"],
}];

static NO_ASSETS: [AssetSpec; 0] = [];

static EQ_PARAMS: [ParamSpec; 4] = [
    pi!("num-bands", "Bands", "Bands", 1, 32, 32, Unit::Count),
    pb!("split-channels", "Split channels", "Bands", false),
    pf!(
        "balance",
        "Balance",
        "Bands",
        -1.0,
        1.0,
        0.0,
        0.01,
        Unit::None
    ),
    pf!(
        "pitch-left",
        "Pitch left",
        "Bands",
        -12.0,
        12.0,
        0.0,
        0.01,
        Unit::Semitones
    ),
];

static MS_EQ_PARAMS: [ParamSpec; 3] = [
    pi!("num-bands", "Bands", "Bands", 1, 32, 32, Unit::Count),
    pf!(
        "balance",
        "Balance",
        "Bands",
        -1.0,
        1.0,
        0.0,
        0.01,
        Unit::None
    ),
    pf!(
        "pitch-left",
        "Pitch mid",
        "Bands",
        -12.0,
        12.0,
        0.0,
        0.01,
        Unit::Semitones
    ),
];

static FILTER_PARAMS: [ParamSpec; 4] = [
    pe!(
        "type",
        "Type",
        "Filter",
        0,
        &[
            "Low-pass",
            "High-pass",
            "Band-pass",
            "Notch",
            "Low-shelf",
            "High-shelf"
        ]
    ),
    pf!(
        "frequency",
        "Frequency",
        "Filter",
        20.0,
        20_000.0,
        1_000.0,
        1.0,
        Unit::Hz,
        log
    ),
    pf!(
        "resonance",
        "Resonance",
        "Filter",
        0.1,
        10.0,
        0.7,
        0.01,
        Unit::None
    ),
    pe!(
        "mode",
        "Slope",
        "Filter",
        1,
        &["12 dB/oct", "24 dB/oct", "48 dB/oct"]
    ),
];

static BASS_ENHANCER_PARAMS: [ParamSpec; 6] = [
    pf!(
        "amount",
        "Amount",
        "Enhance",
        0.0,
        100.0,
        0.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "harmonics",
        "Harmonics",
        "Enhance",
        0.0,
        100.0,
        8.5,
        0.1,
        Unit::Percent
    ),
    pe!("scope", "Scope", "Enhance", 0, &["Low", "Mid", "High"]),
    pf!(
        "blend",
        "Blend",
        "Enhance",
        0.0,
        100.0,
        0.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "floor",
        "Floor",
        "Enhance",
        10.0,
        100.0,
        20.0,
        1.0,
        Unit::Hz
    ),
    pb!("floor-active", "Floor active", "Enhance", false),
];

static BASS_LOUDNESS_PARAMS: [ParamSpec; 2] = [
    pf!(
        "amount",
        "Amount",
        "Loudness",
        0.0,
        100.0,
        0.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "threshold",
        "Threshold",
        "Loudness",
        -60.0,
        0.0,
        -20.0,
        0.1,
        Unit::Db
    ),
];

static LOUDNESS_PARAMS: [ParamSpec; 2] = [
    pf!(
        "loudness",
        "Loudness",
        "Loudness",
        0.0,
        100.0,
        0.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "reference",
        "Reference",
        "Loudness",
        60.0,
        90.0,
        75.0,
        1.0,
        Unit::Db
    ),
];

static COMPRESSOR_PARAMS: [ParamSpec; 21] = [
    pe!(
        "mode",
        "Mode",
        "Dynamics",
        0,
        &["Downward", "Upward", "Boosting"]
    ),
    pf!(
        "threshold",
        "Threshold",
        "Dynamics",
        -60.0,
        0.0,
        -18.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "ratio",
        "Ratio",
        "Dynamics",
        1.0,
        20.0,
        4.0,
        0.1,
        Unit::Ratio
    ),
    pf!("knee", "Knee", "Dynamics", 0.0, 12.0, 6.0, 0.1, Unit::Db),
    pf!(
        "attack",
        "Attack",
        "Dynamics",
        0.1,
        500.0,
        20.0,
        0.1,
        Unit::Ms,
        log
    ),
    pf!(
        "release",
        "Release",
        "Dynamics",
        1.0,
        2000.0,
        250.0,
        1.0,
        Unit::Ms,
        log
    ),
    pf!(
        "makeup",
        "Makeup",
        "Dynamics",
        0.0,
        24.0,
        0.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "release-threshold",
        "Release threshold",
        "Dynamics",
        -60.0,
        0.0,
        -30.0,
        0.1,
        Unit::Db,
        adv
    ),
    pf!(
        "boost-threshold",
        "Boost threshold",
        "Dynamics",
        -60.0,
        0.0,
        -30.0,
        0.1,
        Unit::Db,
        adv
    ),
    pf!(
        "boost-amount",
        "Boost amount",
        "Dynamics",
        0.0,
        24.0,
        0.0,
        0.1,
        Unit::Db,
        adv
    ),
    pf!(
        "stereo-split",
        "Stereo split",
        "Dynamics",
        0.0,
        1.0,
        1.0,
        0.01,
        Unit::None,
        adv
    ),
    pf!(
        "hpf-frequency",
        "Sidechain HPF",
        "Sidechain",
        10.0,
        2000.0,
        20.0,
        1.0,
        Unit::Hz,
        log
    ),
    pe!(
        "hpf-mode",
        "HPF slope",
        "Sidechain",
        0,
        &["Off", "12 dB/oct", "24 dB/oct"]
    ),
    pf!(
        "lpf-frequency",
        "Sidechain LPF",
        "Sidechain",
        1000.0,
        20_000.0,
        20_000.0,
        1.0,
        Unit::Hz,
        log
    ),
    pe!(
        "lpf-mode",
        "LPF slope",
        "Sidechain",
        0,
        &["Off", "12 dB/oct", "24 dB/oct"]
    ),
    pf!(
        "lookahead",
        "Lookahead",
        "Dynamics",
        0.0,
        20.0,
        0.0,
        0.1,
        Unit::Ms,
        adv
    ),
    pf!(
        "sidechain-preamp",
        "Sidechain preamp",
        "Sidechain",
        -60.0,
        24.0,
        0.0,
        0.1,
        Unit::Db,
        adv
    ),
    pf!(
        "sidechain-reactivity",
        "Reactivity",
        "Sidechain",
        0.0,
        100.0,
        10.0,
        0.1,
        Unit::Percent,
        adv
    ),
    pe!(
        "sidechain-type",
        "Detector type",
        "Sidechain",
        0,
        &["Feed-forward", "Feed-back"]
    ),
    pe!(
        "sidechain-mode",
        "Detector mode",
        "Sidechain",
        0,
        &["Peak", "RMS"]
    ),
    pe!(
        "sidechain-source",
        "Source",
        "Sidechain",
        0,
        &["Internal", "External"]
    ),
];

static LIMITER_PARAMS: [ParamSpec; 8] = [
    pf!(
        "threshold",
        "Threshold",
        "Limiter",
        -60.0,
        0.0,
        -3.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "lookahead",
        "Lookahead",
        "Limiter",
        0.0,
        20.0,
        5.0,
        0.1,
        Unit::Ms
    ),
    pf!(
        "attack",
        "Attack",
        "Limiter",
        0.1,
        100.0,
        5.0,
        0.1,
        Unit::Ms
    ),
    pf!(
        "release",
        "Release",
        "Limiter",
        1.0,
        1000.0,
        50.0,
        1.0,
        Unit::Ms
    ),
    pe!(
        "oversampling",
        "Oversampling",
        "Limiter",
        0,
        &["None", "Half x2", "Full x2", "Half x4", "Full x4"]
    ),
    pe!(
        "dithering",
        "Dithering",
        "Limiter",
        0,
        &["None", "16 bit", "24 bit"]
    ),
    pb!("alr", "ALR", "Limiter", false),
    pb!("gain-boost", "Gain boost", "Limiter", false),
];

static MAXIMIZER_PARAMS: [ParamSpec; 4] = [
    pf!(
        "threshold",
        "Threshold",
        "Maximizer",
        -60.0,
        0.0,
        -3.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "ceiling",
        "Ceiling",
        "Maximizer",
        -6.0,
        0.0,
        -0.1,
        0.01,
        Unit::Db
    ),
    pf!(
        "release",
        "Release",
        "Maximizer",
        1.0,
        1000.0,
        50.0,
        1.0,
        Unit::Ms
    ),
    pe!(
        "oversampling",
        "Oversampling",
        "Maximizer",
        0,
        &["None", "Half x2", "Full x2", "Half x4", "Full x4"]
    ),
];

static EXPANDER_PARAMS: [ParamSpec; 8] = [
    pf!(
        "threshold",
        "Threshold",
        "Expander",
        -60.0,
        0.0,
        -30.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "ratio",
        "Ratio",
        "Expander",
        1.0,
        20.0,
        2.0,
        0.1,
        Unit::Ratio
    ),
    pf!(
        "attack",
        "Attack",
        "Expander",
        0.1,
        500.0,
        10.0,
        0.1,
        Unit::Ms,
        log
    ),
    pf!(
        "release",
        "Release",
        "Expander",
        1.0,
        2000.0,
        100.0,
        1.0,
        Unit::Ms,
        log
    ),
    pf!("knee", "Knee", "Expander", 0.0, 12.0, 6.0, 0.1, Unit::Db),
    pf!(
        "makeup",
        "Makeup",
        "Expander",
        0.0,
        24.0,
        0.0,
        0.1,
        Unit::Db
    ),
    pe!("detector", "Detector", "Expander", 0, &["Peak", "RMS"]),
    pf!(
        "stereo-link",
        "Stereo link",
        "Expander",
        0.0,
        1.0,
        1.0,
        0.01,
        Unit::None
    ),
];

static GATE_PARAMS: [ParamSpec; 8] = [
    pf!(
        "threshold",
        "Threshold",
        "Gate",
        -80.0,
        0.0,
        -40.0,
        0.1,
        Unit::Db
    ),
    pf!("attack", "Attack", "Gate", 0.1, 100.0, 5.0, 0.1, Unit::Ms),
    pf!(
        "release",
        "Release",
        "Gate",
        1.0,
        2000.0,
        100.0,
        1.0,
        Unit::Ms
    ),
    pf!("hold", "Hold", "Gate", 0.0, 1000.0, 20.0, 1.0, Unit::Ms),
    pf!("range", "Range", "Gate", -80.0, 0.0, -60.0, 0.1, Unit::Db),
    pf!("knee", "Knee", "Gate", 0.0, 12.0, 3.0, 0.1, Unit::Db),
    pf!(
        "hysteresis",
        "Hysteresis",
        "Gate",
        0.0,
        20.0,
        3.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "stereo-link",
        "Stereo link",
        "Gate",
        0.0,
        1.0,
        1.0,
        0.01,
        Unit::None
    ),
];

static MULTIBAND_COMP_PARAMS: [ParamSpec; 27] = [
    pf!(
        "crossover-1",
        "Crossover 1",
        "Crossovers",
        20.0,
        20_000.0,
        200.0,
        1.0,
        Unit::Hz,
        log
    ),
    pf!(
        "crossover-2",
        "Crossover 2",
        "Crossovers",
        20.0,
        20_000.0,
        1_000.0,
        1.0,
        Unit::Hz,
        log
    ),
    pf!(
        "crossover-3",
        "Crossover 3",
        "Crossovers",
        20.0,
        20_000.0,
        5_000.0,
        1.0,
        Unit::Hz,
        log
    ),
    pe!("mode", "Crossover slope", "Crossovers", 0, &["LR4", "LR8"]),
    pf!(
        "band0-threshold",
        "Band 1 threshold",
        "Band 1",
        -60.0,
        0.0,
        -18.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band0-ratio",
        "Band 1 ratio",
        "Band 1",
        1.0,
        20.0,
        4.0,
        0.1,
        Unit::Ratio
    ),
    pf!(
        "band0-attack",
        "Band 1 attack",
        "Band 1",
        0.1,
        500.0,
        20.0,
        0.1,
        Unit::Ms,
        log
    ),
    pf!(
        "band0-release",
        "Band 1 release",
        "Band 1",
        1.0,
        2000.0,
        250.0,
        1.0,
        Unit::Ms,
        log
    ),
    pf!(
        "band0-makeup",
        "Band 1 makeup",
        "Band 1",
        0.0,
        24.0,
        0.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band0-knee",
        "Band 1 knee",
        "Band 1",
        0.0,
        12.0,
        6.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band1-threshold",
        "Band 2 threshold",
        "Band 2",
        -60.0,
        0.0,
        -18.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band1-ratio",
        "Band 2 ratio",
        "Band 2",
        1.0,
        20.0,
        4.0,
        0.1,
        Unit::Ratio
    ),
    pf!(
        "band1-attack",
        "Band 2 attack",
        "Band 2",
        0.1,
        500.0,
        20.0,
        0.1,
        Unit::Ms,
        log
    ),
    pf!(
        "band1-release",
        "Band 2 release",
        "Band 2",
        1.0,
        2000.0,
        250.0,
        1.0,
        Unit::Ms,
        log
    ),
    pf!(
        "band1-makeup",
        "Band 2 makeup",
        "Band 2",
        0.0,
        24.0,
        0.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band1-knee",
        "Band 2 knee",
        "Band 2",
        0.0,
        12.0,
        6.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band2-threshold",
        "Band 3 threshold",
        "Band 3",
        -60.0,
        0.0,
        -18.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band2-ratio",
        "Band 3 ratio",
        "Band 3",
        1.0,
        20.0,
        4.0,
        0.1,
        Unit::Ratio
    ),
    pf!(
        "band2-attack",
        "Band 3 attack",
        "Band 3",
        0.1,
        500.0,
        20.0,
        0.1,
        Unit::Ms,
        log
    ),
    pf!(
        "band2-release",
        "Band 3 release",
        "Band 3",
        1.0,
        2000.0,
        250.0,
        1.0,
        Unit::Ms,
        log
    ),
    pf!(
        "band2-makeup",
        "Band 3 makeup",
        "Band 3",
        0.0,
        24.0,
        0.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band2-knee",
        "Band 3 knee",
        "Band 3",
        0.0,
        12.0,
        6.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band3-threshold",
        "Band 4 threshold",
        "Band 4",
        -60.0,
        0.0,
        -18.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band3-ratio",
        "Band 4 ratio",
        "Band 4",
        1.0,
        20.0,
        4.0,
        0.1,
        Unit::Ratio
    ),
    pf!(
        "band3-attack",
        "Band 4 attack",
        "Band 4",
        0.1,
        500.0,
        20.0,
        0.1,
        Unit::Ms,
        log
    ),
    pf!(
        "band3-release",
        "Band 4 release",
        "Band 4",
        1.0,
        2000.0,
        250.0,
        1.0,
        Unit::Ms,
        log
    ),
    pf!(
        "band3-makeup",
        "Band 4 makeup",
        "Band 4",
        0.0,
        24.0,
        0.0,
        0.1,
        Unit::Db
    ),
];

static MULTIBAND_GATE_PARAMS: [ParamSpec; 19] = [
    pf!(
        "crossover-1",
        "Crossover 1",
        "Crossovers",
        20.0,
        20_000.0,
        200.0,
        1.0,
        Unit::Hz,
        log
    ),
    pf!(
        "crossover-2",
        "Crossover 2",
        "Crossovers",
        20.0,
        20_000.0,
        1_000.0,
        1.0,
        Unit::Hz,
        log
    ),
    pf!(
        "crossover-3",
        "Crossover 3",
        "Crossovers",
        20.0,
        20_000.0,
        5_000.0,
        1.0,
        Unit::Hz,
        log
    ),
    pf!(
        "band0-threshold",
        "Band 1 threshold",
        "Band 1",
        -80.0,
        0.0,
        -40.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band0-attack",
        "Band 1 attack",
        "Band 1",
        0.1,
        100.0,
        5.0,
        0.1,
        Unit::Ms
    ),
    pf!(
        "band0-release",
        "Band 1 release",
        "Band 1",
        1.0,
        2000.0,
        100.0,
        1.0,
        Unit::Ms
    ),
    pf!(
        "band0-range",
        "Band 1 range",
        "Band 1",
        -80.0,
        0.0,
        -60.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band1-threshold",
        "Band 2 threshold",
        "Band 2",
        -80.0,
        0.0,
        -40.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band1-attack",
        "Band 2 attack",
        "Band 2",
        0.1,
        100.0,
        5.0,
        0.1,
        Unit::Ms
    ),
    pf!(
        "band1-release",
        "Band 2 release",
        "Band 2",
        1.0,
        2000.0,
        100.0,
        1.0,
        Unit::Ms
    ),
    pf!(
        "band1-range",
        "Band 2 range",
        "Band 2",
        -80.0,
        0.0,
        -60.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band2-threshold",
        "Band 3 threshold",
        "Band 3",
        -80.0,
        0.0,
        -40.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band2-attack",
        "Band 3 attack",
        "Band 3",
        0.1,
        100.0,
        5.0,
        0.1,
        Unit::Ms
    ),
    pf!(
        "band2-release",
        "Band 3 release",
        "Band 3",
        1.0,
        2000.0,
        100.0,
        1.0,
        Unit::Ms
    ),
    pf!(
        "band2-range",
        "Band 3 range",
        "Band 3",
        -80.0,
        0.0,
        -60.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band3-threshold",
        "Band 4 threshold",
        "Band 4",
        -80.0,
        0.0,
        -40.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "band3-attack",
        "Band 4 attack",
        "Band 4",
        0.1,
        100.0,
        5.0,
        0.1,
        Unit::Ms
    ),
    pf!(
        "band3-release",
        "Band 4 release",
        "Band 4",
        1.0,
        2000.0,
        100.0,
        1.0,
        Unit::Ms
    ),
    pf!(
        "band3-range",
        "Band 4 range",
        "Band 4",
        -80.0,
        0.0,
        -60.0,
        0.1,
        Unit::Db
    ),
];

static DEESSER_PARAMS: [ParamSpec; 8] = [
    pf!(
        "threshold",
        "Threshold",
        "De-esser",
        -60.0,
        0.0,
        -18.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "ratio",
        "Ratio",
        "De-esser",
        1.0,
        20.0,
        4.0,
        0.1,
        Unit::Ratio
    ),
    pf!(
        "makeup",
        "Makeup",
        "De-esser",
        0.0,
        24.0,
        0.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "attack",
        "Attack",
        "De-esser",
        0.1,
        100.0,
        5.0,
        0.1,
        Unit::Ms
    ),
    pf!(
        "release",
        "Release",
        "De-esser",
        1.0,
        1000.0,
        50.0,
        1.0,
        Unit::Ms
    ),
    pe!("detector", "Detector", "De-esser", 0, &["Peak", "RMS"]),
    pe!("mode", "Mode", "De-esser", 0, &["Wide", "Split"]),
    pf!(
        "split-frequency",
        "Split frequency",
        "De-esser",
        1000.0,
        16_000.0,
        6_000.0,
        1.0,
        Unit::Hz,
        log
    ),
];

static AUTOGAIN_PARAMS: [ParamSpec; 4] = [
    pf!(
        "target",
        "Target loudness",
        "Auto gain",
        -40.0,
        0.0,
        -14.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "silence-threshold",
        "Silence threshold",
        "Auto gain",
        -80.0,
        -20.0,
        -60.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "maximum-gain",
        "Maximum gain",
        "Auto gain",
        0.0,
        40.0,
        20.0,
        0.1,
        Unit::Db
    ),
    pe!(
        "fallback",
        "Fallback",
        "Auto gain",
        0,
        &["Last gain", "Target"]
    ),
];

static CONVOLVER_PARAMS: [ParamSpec; 4] = [
    pb!("autogain", "Autogain", "Convolver", true),
    pe!(
        "blocksize",
        "Block size",
        "Convolver",
        2,
        &["256", "512", "1024", "2048", "4096"]
    ),
    pf!(
        "tailsize",
        "Tail size",
        "Convolver",
        0.0,
        100.0,
        0.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "ir-width",
        "IR width",
        "Convolver",
        0.0,
        100.0,
        100.0,
        0.1,
        Unit::Percent
    ),
];

static REVERB_PARAMS: [ParamSpec; 7] = [
    pf!(
        "room-size",
        "Room size",
        "Reverb",
        0.0,
        100.0,
        50.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "damping",
        "Damping",
        "Reverb",
        0.0,
        100.0,
        50.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "width",
        "Width",
        "Reverb",
        0.0,
        100.0,
        100.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "pre-delay",
        "Pre-delay",
        "Reverb",
        0.0,
        200.0,
        20.0,
        0.1,
        Unit::Ms
    ),
    pf!(
        "decay-time",
        "Decay time",
        "Reverb",
        0.1,
        10.0,
        1.5,
        0.01,
        Unit::Seconds
    ),
    pf!(
        "diffusion",
        "Diffusion",
        "Reverb",
        0.0,
        100.0,
        50.0,
        0.1,
        Unit::Percent
    ),
    pb!("tail", "Tail", "Reverb", true),
];

static DELAY_PARAMS: [ParamSpec; 4] = [
    pf!(
        "time-l",
        "Time left",
        "Delay",
        1.0,
        2000.0,
        250.0,
        1.0,
        Unit::Ms
    ),
    pf!(
        "time-r",
        "Time right",
        "Delay",
        1.0,
        2000.0,
        250.0,
        1.0,
        Unit::Ms
    ),
    pf!(
        "feedback",
        "Feedback",
        "Delay",
        0.0,
        100.0,
        30.0,
        0.1,
        Unit::Percent
    ),
    pb!("ping-pong", "Ping-pong", "Delay", false),
];

static CROSSFEED_PARAMS: [ParamSpec; 3] = [
    pf!(
        "fcut",
        "Cutoff",
        "Crossfeed",
        300.0,
        2000.0,
        700.0,
        1.0,
        Unit::Hz,
        log
    ),
    pf!("feed", "Feed", "Crossfeed", 0.0, 15.0, 4.5, 0.1, Unit::Db),
    pf!("level", "Level", "Crossfeed", 0.0, 15.0, 0.0, 0.1, Unit::Db),
];

static CROSSTALK_PARAMS: [ParamSpec; 3] = [
    pe!("source", "Source", "Canceller", 0, &["Left", "Right"]),
    pf!(
        "delay",
        "Delay",
        "Canceller",
        0.0,
        1000.0,
        0.0,
        0.1,
        Unit::Ms
    ),
    pf!(
        "amount",
        "Amount",
        "Canceller",
        0.0,
        100.0,
        50.0,
        0.1,
        Unit::Percent
    ),
];

static STEREO_TOOLS_PARAMS: [ParamSpec; 5] = [
    pf!(
        "balance",
        "Balance",
        "Stereo",
        -1.0,
        1.0,
        0.0,
        0.01,
        Unit::None
    ),
    pf!("width", "Width", "Stereo", 0.0, 2.0, 1.0, 0.01, Unit::None),
    pf!(
        "mid-level",
        "Mid level",
        "Stereo",
        -24.0,
        24.0,
        0.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "side-level",
        "Side level",
        "Stereo",
        -24.0,
        24.0,
        0.0,
        0.1,
        Unit::Db
    ),
    pb!("phase", "Invert phase", "Stereo", false),
];

static EXCITER_PARAMS: [ParamSpec; 6] = [
    pf!(
        "amount",
        "Amount",
        "Exciter",
        0.0,
        100.0,
        0.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "harmonics",
        "Harmonics",
        "Exciter",
        0.0,
        100.0,
        8.5,
        0.1,
        Unit::Percent
    ),
    pe!("scope", "Scope", "Exciter", 2, &["Low", "Mid", "High"]),
    pf!(
        "blend",
        "Blend",
        "Exciter",
        0.0,
        100.0,
        0.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "ceil",
        "Ceiling",
        "Exciter",
        1000.0,
        20_000.0,
        16_000.0,
        1.0,
        Unit::Hz,
        log
    ),
    pb!("ceil-active", "Ceiling active", "Exciter", false),
];

static CRYSTALIZER_PARAMS: [ParamSpec; 2] = [
    pf!(
        "amount",
        "Amount",
        "Crystalizer",
        0.0,
        100.0,
        0.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "intensity",
        "Intensity",
        "Crystalizer",
        -10.0,
        10.0,
        0.0,
        0.1,
        Unit::None
    ),
];

static CRUSHER_PARAMS: [ParamSpec; 3] = [
    pi!("bit-depth", "Bit depth", "Crusher", 2, 32, 16, Unit::Count),
    pf!(
        "sample-rate",
        "Sample rate",
        "Crusher",
        1000.0,
        192_000.0,
        48_000.0,
        1.0,
        Unit::Hz,
        log
    ),
    pf!(
        "mix",
        "Mix",
        "Crusher",
        0.0,
        100.0,
        100.0,
        0.1,
        Unit::Percent
    ),
];

static PITCH_PARAMS: [ParamSpec; 5] = [
    pf!(
        "semitones",
        "Semitones",
        "Pitch",
        -24.0,
        24.0,
        0.0,
        0.01,
        Unit::Semitones
    ),
    pf!(
        "cents",
        "Cents",
        "Pitch",
        -100.0,
        100.0,
        0.0,
        1.0,
        Unit::Cents
    ),
    pi!("octaves", "Octaves", "Pitch", -3, 3, 0, Unit::Octaves),
    pf!(
        "window",
        "Window",
        "Pitch",
        10.0,
        100.0,
        50.0,
        1.0,
        Unit::Ms
    ),
    pf!("mix", "Mix", "Pitch", 0.0, 100.0, 100.0, 0.1, Unit::Percent),
];

static AUTOTUNE_PARAMS: [ParamSpec; 6] = [
    pf!(
        "speed",
        "Speed",
        "Autotune",
        0.0,
        100.0,
        20.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "amount",
        "Amount",
        "Autotune",
        0.0,
        100.0,
        100.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "threshold",
        "Threshold",
        "Autotune",
        -80.0,
        0.0,
        -50.0,
        0.1,
        Unit::Db
    ),
    pi!("min-note", "Min note", "Autotune", 0, 127, 36, Unit::Count),
    pi!("max-note", "Max note", "Autotune", 0, 127, 84, Unit::Count),
    pe!(
        "scale",
        "Scale",
        "Autotune",
        0,
        &["Chromatic", "Major", "Minor"]
    ),
];

static HARMONIZER_PARAMS: [ParamSpec; 4] = [
    pf!(
        "interval-1",
        "Interval 1",
        "Harmonizer",
        -24.0,
        24.0,
        7.0,
        1.0,
        Unit::Semitones
    ),
    pf!(
        "interval-2",
        "Interval 2",
        "Harmonizer",
        -24.0,
        24.0,
        12.0,
        1.0,
        Unit::Semitones
    ),
    pi!("voices", "Voices", "Harmonizer", 1, 3, 2, Unit::Count),
    pf!(
        "mix",
        "Mix",
        "Harmonizer",
        0.0,
        100.0,
        50.0,
        0.1,
        Unit::Percent
    ),
];

static SPEECH_PARAMS: [ParamSpec; 7] = [
    pf!(
        "threshold",
        "Threshold",
        "Speech",
        -60.0,
        0.0,
        -20.0,
        0.1,
        Unit::Db
    ),
    pf!("ratio", "Ratio", "Speech", 1.0, 20.0, 3.0, 0.1, Unit::Ratio),
    pf!("attack", "Attack", "Speech", 0.1, 100.0, 5.0, 0.1, Unit::Ms),
    pf!(
        "release",
        "Release",
        "Speech",
        1.0,
        1000.0,
        100.0,
        1.0,
        Unit::Ms
    ),
    pf!(
        "hpf",
        "High-pass",
        "Speech",
        20.0,
        500.0,
        80.0,
        1.0,
        Unit::Hz,
        log
    ),
    pf!(
        "lpf",
        "Low-pass",
        "Speech",
        2000.0,
        20_000.0,
        8_000.0,
        1.0,
        Unit::Hz,
        log
    ),
    pf!(
        "deess",
        "De-ess",
        "Speech",
        0.0,
        100.0,
        30.0,
        0.1,
        Unit::Percent
    ),
];

static VOICE_SUPPRESSOR_PARAMS: [ParamSpec; 3] = [
    pf!(
        "threshold",
        "Threshold",
        "Suppressor",
        -80.0,
        0.0,
        -40.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "amount",
        "Amount",
        "Suppressor",
        0.0,
        100.0,
        100.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "width",
        "Width",
        "Suppressor",
        0.0,
        100.0,
        50.0,
        0.1,
        Unit::Percent
    ),
];

static NOISE_REDUCTION_PARAMS: [ParamSpec; 4] = [
    pf!(
        "amount",
        "Amount",
        "Noise",
        0.0,
        100.0,
        50.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "noise-floor",
        "Noise floor",
        "Noise",
        -100.0,
        -20.0,
        -60.0,
        0.1,
        Unit::Db
    ),
    pf!(
        "release",
        "Release",
        "Noise",
        1.0,
        1000.0,
        100.0,
        1.0,
        Unit::Ms
    ),
    pe!(
        "model",
        "Algorithm",
        "Noise",
        0,
        &["Spectral subtraction", "Wiener"]
    ),
];

static DEEP_NR_PARAMS: [ParamSpec; 2] = [
    pe!(
        "model",
        "Model",
        "Deep noise remover",
        2,
        &["DPDFNet (bundled)", "RNNoise (bundled)", "Spectral gate"]
    ),
    pf!(
        "amount",
        "Amount",
        "Deep noise remover",
        0.0,
        100.0,
        100.0,
        0.1,
        Unit::Percent
    ),
];

static ECHO_CANCELLER_PARAMS: [ParamSpec; 3] = [
    pf!(
        "filter-length",
        "Filter length",
        "Echo canceller",
        10.0,
        500.0,
        200.0,
        1.0,
        Unit::Ms
    ),
    pf!(
        "adaptation",
        "Adaptation",
        "Echo canceller",
        0.0,
        100.0,
        50.0,
        0.1,
        Unit::Percent
    ),
    pf!(
        "amount",
        "Amount",
        "Echo canceller",
        0.0,
        100.0,
        100.0,
        0.1,
        Unit::Percent
    ),
];

static LEVEL_METER_PARAMS: [ParamSpec; 2] = [
    pf!(
        "peak-hold",
        "Peak hold",
        "Meter",
        0.0,
        5000.0,
        1000.0,
        10.0,
        Unit::Ms
    ),
    pe!("scale", "Scale", "Meter", 0, &["dBFS", "LUFS"]),
];

macro_rules! desc {
    ($kind:expr, $name:literal, $group:expr, $summary:literal, $compat:expr, $latency:literal, $params:expr, $bands:expr, $assets:expr) => {
        EffectDescriptor {
            kind: $kind,
            name: $name,
            group: $group,
            summary: $summary,
            compat: $compat,
            latency: $latency,
            params: $params,
            bands: $bands,
            assets: $assets,
        }
    };
}

static EQUALIZER: EffectDescriptor = desc!(
    EffectKind::Equalizer,
    "Equalizer",
    EffectGroup::Eq,
    "32-band parametric EQ with per-channel bands and pitch-scaled curves.",
    CompatLevel::Exact,
    "zero",
    &EQ_PARAMS,
    Some(EQ_BANDS),
    &NO_ASSETS
);

static MID_SIDE_EQUALIZER: EffectDescriptor = desc!(
    EffectKind::MidSideEqualizer,
    "Mid-Side Equalizer",
    EffectGroup::Eq,
    "Parametric EQ operating on the mid and side signals independently.",
    CompatLevel::Exact,
    "zero",
    &MS_EQ_PARAMS,
    Some(MS_BANDS),
    &NO_ASSETS
);

static FILTER: EffectDescriptor = desc!(
    EffectKind::Filter,
    "Filter",
    EffectGroup::Eq,
    "Resonant low/high/band-pass, notch and shelf filter up to 48 dB/oct.",
    CompatLevel::Exact,
    "zero",
    &FILTER_PARAMS,
    None,
    &NO_ASSETS
);

static BASS_ENHANCER: EffectDescriptor = desc!(
    EffectKind::BassEnhancer,
    "Bass Enhancer",
    EffectGroup::Eq,
    "Adds harmonics to the low band so bass is audible on small speakers.",
    CompatLevel::Approximate,
    "zero",
    &BASS_ENHANCER_PARAMS,
    None,
    &NO_ASSETS
);

static BASS_LOUDNESS: EffectDescriptor = desc!(
    EffectKind::BassLoudness,
    "Bass Loudness",
    EffectGroup::Eq,
    "Level-dependent low-shelf boost modelled on equal-loudness contours.",
    CompatLevel::Approximate,
    "zero",
    &BASS_LOUDNESS_PARAMS,
    None,
    &NO_ASSETS
);

static LOUDNESS: EffectDescriptor = desc!(
    EffectKind::Loudness,
    "Loudness",
    EffectGroup::Eq,
    "Equal-loudness compensation that follows the playback level.",
    CompatLevel::Approximate,
    "zero",
    &LOUDNESS_PARAMS,
    None,
    &NO_ASSETS
);

static COMPRESSOR: EffectDescriptor = desc!(
    EffectKind::Compressor,
    "Compressor",
    EffectGroup::Dynamics,
    "Feed-forward/feedback compressor with knee, sidechain filters and lookahead.",
    CompatLevel::Approximate,
    "zero",
    &COMPRESSOR_PARAMS,
    None,
    &NO_ASSETS
);

static LIMITER: EffectDescriptor = desc!(
    EffectKind::Limiter,
    "Limiter",
    EffectGroup::Dynamics,
    "Lookahead brickwall limiter with optional oversampling and dithering.",
    CompatLevel::Approximate,
    "adaptive",
    &LIMITER_PARAMS,
    None,
    &NO_ASSETS
);

static MAXIMIZER: EffectDescriptor = desc!(
    EffectKind::Maximizer,
    "Maximizer",
    EffectGroup::Dynamics,
    "Upward and downward loudness maximizer with a hard ceiling.",
    CompatLevel::Approximate,
    "adaptive",
    &MAXIMIZER_PARAMS,
    None,
    &NO_ASSETS
);

static EXPANDER: EffectDescriptor = desc!(
    EffectKind::Expander,
    "Expander",
    EffectGroup::Dynamics,
    "Downward expander for restoring dynamics and reducing noise.",
    CompatLevel::Approximate,
    "zero",
    &EXPANDER_PARAMS,
    None,
    &NO_ASSETS
);

static GATE: EffectDescriptor = desc!(
    EffectKind::Gate,
    "Gate",
    EffectGroup::Dynamics,
    "Noise gate with hold, hysteresis and a configurable floor.",
    CompatLevel::Approximate,
    "zero",
    &GATE_PARAMS,
    None,
    &NO_ASSETS
);

static MULTIBAND_COMPRESSOR: EffectDescriptor = desc!(
    EffectKind::MultibandCompressor,
    "Multiband Compressor",
    EffectGroup::Dynamics,
    "Four-band compressor with Linkwitz-Riley crossovers.",
    CompatLevel::Approximate,
    "zero",
    &MULTIBAND_COMP_PARAMS,
    None,
    &NO_ASSETS
);

static MULTIBAND_GATE: EffectDescriptor = desc!(
    EffectKind::MultibandGate,
    "Multiband Gate",
    EffectGroup::Dynamics,
    "Four-band gate for frequency-selective noise control.",
    CompatLevel::Approximate,
    "zero",
    &MULTIBAND_GATE_PARAMS,
    None,
    &NO_ASSETS
);

static DEESSER: EffectDescriptor = desc!(
    EffectKind::Deesser,
    "De-esser",
    EffectGroup::Dynamics,
    "Split-band sibilance control with wide and split detection modes.",
    CompatLevel::Approximate,
    "zero",
    &DEESSER_PARAMS,
    None,
    &NO_ASSETS
);

static AUTOGAIN: EffectDescriptor = desc!(
    EffectKind::Autogain,
    "Auto Gain",
    EffectGroup::Dynamics,
    "Slow loudness matching towards a target with silence handling.",
    CompatLevel::Approximate,
    "zero",
    &AUTOGAIN_PARAMS,
    None,
    &NO_ASSETS
);

static CONVOLVER: EffectDescriptor = desc!(
    EffectKind::Convolver,
    "Convolver",
    EffectGroup::Space,
    "Partitioned FFT convolution with IR autogain and width control.",
    CompatLevel::Exact,
    "block",
    &CONVOLVER_PARAMS,
    None,
    &IR_ASSET
);

static REVERBERATION: EffectDescriptor = desc!(
    EffectKind::Reverberation,
    "Reverberation",
    EffectGroup::Space,
    "Feedback delay network reverb with early reflections and damping.",
    CompatLevel::Approximate,
    "zero",
    &REVERB_PARAMS,
    None,
    &NO_ASSETS
);

static DELAY: EffectDescriptor = desc!(
    EffectKind::Delay,
    "Delay",
    EffectGroup::Space,
    "Stereo delay with feedback, ping-pong routing and tempo-free times.",
    CompatLevel::Exact,
    "zero",
    &DELAY_PARAMS,
    None,
    &NO_ASSETS
);

static CROSSFEED: EffectDescriptor = desc!(
    EffectKind::Crossfeed,
    "Crossfeed",
    EffectGroup::Space,
    "Bauer-style crossfeed for headphone listening.",
    CompatLevel::Exact,
    "zero",
    &CROSSFEED_PARAMS,
    None,
    &NO_ASSETS
);

static CROSSTALK_CANCELLER: EffectDescriptor = desc!(
    EffectKind::CrosstalkCanceller,
    "Crosstalk Canceller",
    EffectGroup::Space,
    "Cancels the opposite channel's leakage for speaker playback.",
    CompatLevel::Approximate,
    "zero",
    &CROSSTALK_PARAMS,
    None,
    &NO_ASSETS
);

static STEREO_TOOLS: EffectDescriptor = desc!(
    EffectKind::StereoTools,
    "Stereo Tools",
    EffectGroup::Space,
    "Mid/side balance, width and level control.",
    CompatLevel::Exact,
    "zero",
    &STEREO_TOOLS_PARAMS,
    None,
    &NO_ASSETS
);

static EXCITER: EffectDescriptor = desc!(
    EffectKind::Exciter,
    "Exciter",
    EffectGroup::Tone,
    "Band-limited harmonic exciter with a ceiling filter.",
    CompatLevel::Approximate,
    "zero",
    &EXCITER_PARAMS,
    None,
    &NO_ASSETS
);

static CRYSTALIZER: EffectDescriptor = desc!(
    EffectKind::Crystalizer,
    "Crystalizer",
    EffectGroup::Tone,
    "Transient-aware high-frequency enhancement.",
    CompatLevel::Approximate,
    "zero",
    &CRYSTALIZER_PARAMS,
    None,
    &NO_ASSETS
);

static CRUSHER: EffectDescriptor = desc!(
    EffectKind::Crusher,
    "Crusher",
    EffectGroup::Tone,
    "Bit-depth and sample-rate reduction.",
    CompatLevel::Exact,
    "zero",
    &CRUSHER_PARAMS,
    None,
    &NO_ASSETS
);

static PITCH_SHIFT: EffectDescriptor = desc!(
    EffectKind::PitchShift,
    "Pitch Shift",
    EffectGroup::Voice,
    "Two-tap granular pitch shifter with independent mix.",
    CompatLevel::Substitute,
    "zero",
    &PITCH_PARAMS,
    None,
    &NO_ASSETS
);

static AUTOTUNE: EffectDescriptor = desc!(
    EffectKind::Autotune,
    "Autotune",
    EffectGroup::Voice,
    "YIN pitch detection with scale-quantised correction.",
    CompatLevel::Substitute,
    "zero",
    &AUTOTUNE_PARAMS,
    None,
    &NO_ASSETS
);

static HARMONIZER: EffectDescriptor = desc!(
    EffectKind::Harmonizer,
    "Harmonizer",
    EffectGroup::Voice,
    "Two pitch-shifted harmony voices above the dry signal.",
    CompatLevel::Substitute,
    "zero",
    &HARMONIZER_PARAMS,
    None,
    &NO_ASSETS
);

static SPEECH_PROCESSOR: EffectDescriptor = desc!(
    EffectKind::SpeechProcessor,
    "Speech Processor",
    EffectGroup::Voice,
    "Broadcast-style voice chain: band limiting, compression and de-essing.",
    CompatLevel::Substitute,
    "zero",
    &SPEECH_PARAMS,
    None,
    &NO_ASSETS
);

static VOICE_SUPPRESSOR: EffectDescriptor = desc!(
    EffectKind::VoiceSuppressor,
    "Voice Suppressor",
    EffectGroup::Voice,
    "Attenuates centre-panned speech while keeping the sides.",
    CompatLevel::Substitute,
    "zero",
    &VOICE_SUPPRESSOR_PARAMS,
    None,
    &NO_ASSETS
);

static NOISE_REDUCTION: EffectDescriptor = desc!(
    EffectKind::NoiseReduction,
    "Noise Reduction",
    EffectGroup::Noise,
    "STFT spectral subtraction with a tracked noise floor.",
    CompatLevel::Substitute,
    "block",
    &NOISE_REDUCTION_PARAMS,
    None,
    &NO_ASSETS
);

static DEEP_NOISE_REMOVER: EffectDescriptor = desc!(
    EffectKind::DeepNoiseRemover,
    "Deep Noise Remover",
    EffectGroup::Noise,
    "Model-free spectral gate that stands in for the upstream neural model.",
    CompatLevel::Substitute,
    "block",
    &DEEP_NR_PARAMS,
    None,
    &MODEL_ASSET
);

static ECHO_CANCELLER: EffectDescriptor = desc!(
    EffectKind::EchoCanceller,
    "Echo Canceller",
    EffectGroup::Noise,
    "NLMS adaptive echo canceller for microphone paths.",
    CompatLevel::Substitute,
    "zero",
    &ECHO_CANCELLER_PARAMS,
    None,
    &NO_ASSETS
);

static LEVEL_METER: EffectDescriptor = desc!(
    EffectKind::LevelMeter,
    "Level Meter",
    EffectGroup::Meter,
    "Peak, true peak, RMS and loudness measurement.",
    CompatLevel::Meter,
    "zero",
    &LEVEL_METER_PARAMS,
    None,
    &NO_ASSETS
);

pub fn descriptor(kind: EffectKind) -> &'static EffectDescriptor {
    match kind {
        EffectKind::Equalizer => &EQUALIZER,
        EffectKind::MidSideEqualizer => &MID_SIDE_EQUALIZER,
        EffectKind::Filter => &FILTER,
        EffectKind::BassEnhancer => &BASS_ENHANCER,
        EffectKind::BassLoudness => &BASS_LOUDNESS,
        EffectKind::Loudness => &LOUDNESS,
        EffectKind::Compressor => &COMPRESSOR,
        EffectKind::Limiter => &LIMITER,
        EffectKind::Maximizer => &MAXIMIZER,
        EffectKind::Expander => &EXPANDER,
        EffectKind::Gate => &GATE,
        EffectKind::MultibandCompressor => &MULTIBAND_COMPRESSOR,
        EffectKind::MultibandGate => &MULTIBAND_GATE,
        EffectKind::Deesser => &DEESSER,
        EffectKind::Autogain => &AUTOGAIN,
        EffectKind::Convolver => &CONVOLVER,
        EffectKind::Reverberation => &REVERBERATION,
        EffectKind::Delay => &DELAY,
        EffectKind::Crossfeed => &CROSSFEED,
        EffectKind::CrosstalkCanceller => &CROSSTALK_CANCELLER,
        EffectKind::StereoTools => &STEREO_TOOLS,
        EffectKind::Exciter => &EXCITER,
        EffectKind::Crystalizer => &CRYSTALIZER,
        EffectKind::Crusher => &CRUSHER,
        EffectKind::PitchShift => &PITCH_SHIFT,
        EffectKind::Autotune => &AUTOTUNE,
        EffectKind::Harmonizer => &HARMONIZER,
        EffectKind::SpeechProcessor => &SPEECH_PROCESSOR,
        EffectKind::VoiceSuppressor => &VOICE_SUPPRESSOR,
        EffectKind::NoiseReduction => &NOISE_REDUCTION,
        EffectKind::DeepNoiseRemover => &DEEP_NOISE_REMOVER,
        EffectKind::EchoCanceller => &ECHO_CANCELLER,
        EffectKind::LevelMeter => &LEVEL_METER,
    }
}

pub fn all() -> impl Iterator<Item = &'static EffectDescriptor> {
    EffectKind::ALL.into_iter().map(descriptor)
}

/// Look up a parameter id by its EasyEffects key.
pub fn param_id(store: &dsp_core::ParamStore, key: &str) -> u16 {
    store.spec_by_key(key).map(|(id, _)| id).unwrap_or(u16::MAX)
}
