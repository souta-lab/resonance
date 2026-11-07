//! Parameter model.
//!
//! Parameters are addressed by a `u16` id. Ids `0..16` are reserved for the
//! common per-instance controls that every EasyEffects plugin exposes. Effect
//! specific parameters start at [`EFFECT_PARAM_BASE`]. Band parameters (used by
//! the equalizers) live at [`BAND_BASE`] and are laid out by [`BandLayout`].
//!
//! Values are stored as `f32` bits in atomics so that the audio thread can read
//! them without locks. Booleans are `0.0`/`1.0`, enums are their index.

use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

pub const P_INPUT_GAIN: u16 = 0;
pub const P_OUTPUT_GAIN: u16 = 1;
pub const P_DRY: u16 = 2;
pub const P_WET: u16 = 3;
pub const P_BYPASS: u16 = 4;
pub const EFFECT_PARAM_BASE: u16 = 16;
pub const BAND_BASE: u16 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    F32(f32),
    Bool(bool),
    Int(i32),
    Enum(u16),
}

impl ParamValue {
    pub fn as_f32(self) -> f32 {
        match self {
            ParamValue::F32(v) => v,
            ParamValue::Bool(b) => {
                if b {
                    1.0
                } else {
                    0.0
                }
            }
            ParamValue::Int(i) => i as f32,
            ParamValue::Enum(e) => e as f32,
        }
    }

    pub fn as_bool(self) -> bool {
        self.as_f32() != 0.0
    }

    pub fn as_u16(self) -> u16 {
        let v = self.as_f32();
        if v.is_finite() {
            v.clamp(0.0, u16::MAX as f32) as u16
        } else {
            0
        }
    }

    pub fn as_i32(self) -> i32 {
        let v = self.as_f32();
        if v.is_finite() {
            v.round() as i32
        } else {
            0
        }
    }

    pub fn is_finite(self) -> bool {
        match self {
            ParamValue::F32(v) => v.is_finite(),
            _ => true,
        }
    }
}

impl From<f32> for ParamValue {
    fn from(v: f32) -> Self {
        ParamValue::F32(v)
    }
}
impl From<bool> for ParamValue {
    fn from(v: bool) -> Self {
        ParamValue::Bool(v)
    }
}
impl From<i32> for ParamValue {
    fn from(v: i32) -> Self {
        ParamValue::Int(v)
    }
}
impl From<u16> for ParamValue {
    fn from(v: u16) -> Self {
        ParamValue::Enum(v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    None,
    Db,
    Hz,
    Ms,
    Percent,
    Ratio,
    Seconds,
    Octaves,
    Degrees,
    Count,
    Semitones,
    Cents,
    Bpm,
    DbPerOct,
}

impl Unit {
    pub const fn as_str(self) -> &'static str {
        match self {
            Unit::None => "",
            Unit::Db => "dB",
            Unit::Hz => "Hz",
            Unit::Ms => "ms",
            Unit::Percent => "%",
            Unit::Ratio => ":1",
            Unit::Seconds => "s",
            Unit::Octaves => "oct",
            Unit::Degrees => "°",
            Unit::Count => "",
            Unit::Semitones => "st",
            Unit::Cents => "ct",
            Unit::Bpm => "BPM",
            Unit::DbPerOct => "dB/oct",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamKind {
    Float {
        min: f32,
        max: f32,
        default: f32,
        step: f32,
        unit: Unit,
        log: bool,
    },
    Bool {
        default: bool,
    },
    Int {
        min: i32,
        max: i32,
        default: i32,
        unit: Unit,
    },
    Enum {
        default: u16,
        labels: &'static [&'static str],
    },
}

impl ParamKind {
    pub fn default_value(&self) -> ParamValue {
        match *self {
            ParamKind::Float { default, .. } => ParamValue::F32(default),
            ParamKind::Bool { default } => ParamValue::Bool(default),
            ParamKind::Int { default, .. } => ParamValue::Int(default),
            ParamKind::Enum { default, .. } => ParamValue::Enum(default),
        }
    }

    /// Clamp / coerce an incoming value to this parameter's domain.
    pub fn coerce(&self, v: ParamValue) -> ParamValue {
        match *self {
            ParamKind::Float { min, max, .. } => {
                let mut x = v.as_f32();
                if !x.is_finite() {
                    x = min;
                }
                ParamValue::F32(x.clamp(min, max))
            }
            ParamKind::Bool { .. } => ParamValue::Bool(v.as_bool()),
            ParamKind::Int { min, max, .. } => ParamValue::Int(v.as_i32().clamp(min, max)),
            ParamKind::Enum { labels, .. } => {
                let max = labels.len().saturating_sub(1) as u16;
                ParamValue::Enum(v.as_u16().min(max))
            }
        }
    }

    pub fn is_float(&self) -> bool {
        matches!(self, ParamKind::Float { .. })
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ParamSpec {
    /// EasyEffects JSON key (kebab-case).
    pub key: &'static str,
    pub label: &'static str,
    /// UI grouping inside the effect faceplate.
    pub group: &'static str,
    pub kind: ParamKind,
    /// Hidden behind the "advanced" disclosure in the UI.
    pub advanced: bool,
}

impl ParamSpec {
    pub fn default_value(&self) -> ParamValue {
        self.kind.default_value()
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct BandField {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct BandLayout {
    pub max_bands: usize,
    pub channels: usize,
    pub fields: &'static [BandField],
    /// Display names for the band channels, e.g. `["left", "right"]`.
    pub channel_labels: &'static [&'static str],
}

impl BandLayout {
    pub fn slots(&self) -> usize {
        self.max_bands * self.channels * self.fields.len()
    }

    pub fn field_index(&self, key: &str) -> Option<usize> {
        self.fields.iter().position(|f| f.key == key)
    }
}

/// Common controls shared by every plugin instance.
pub const COMMON_PARAMS: [ParamSpec; 5] = [
    ParamSpec {
        key: "input-gain",
        label: "Input",
        group: "Gain",
        kind: ParamKind::Float {
            min: -60.0,
            max: 24.0,
            default: 0.0,
            step: 0.1,
            unit: Unit::Db,
            log: false,
        },
        advanced: false,
    },
    ParamSpec {
        key: "output-gain",
        label: "Output",
        group: "Gain",
        kind: ParamKind::Float {
            min: -60.0,
            max: 24.0,
            default: 0.0,
            step: 0.1,
            unit: Unit::Db,
            log: false,
        },
        advanced: false,
    },
    ParamSpec {
        key: "dry",
        label: "Dry",
        group: "Mix",
        kind: ParamKind::Float {
            min: 0.0,
            max: 1.0,
            default: 1.0,
            step: 0.01,
            unit: Unit::None,
            log: false,
        },
        advanced: true,
    },
    ParamSpec {
        key: "wet",
        label: "Wet",
        group: "Mix",
        kind: ParamKind::Float {
            min: 0.0,
            max: 1.0,
            default: 1.0,
            step: 0.01,
            unit: Unit::None,
            log: false,
        },
        advanced: true,
    },
    ParamSpec {
        key: "bypass",
        label: "Bypass",
        group: "Gain",
        kind: ParamKind::Bool { default: false },
        advanced: false,
    },
];

/// Fixed-size, lock-free parameter block.
pub struct ParamStore {
    values: Box<[AtomicU32]>,
    specs: &'static [ParamSpec],
    bands: Option<BandLayout>,
}

impl ParamStore {
    pub fn new(specs: &'static [ParamSpec], bands: Option<BandLayout>) -> Self {
        let band_slots = bands.map(|b| b.slots()).unwrap_or(0);
        let len = if band_slots > 0 {
            BAND_BASE as usize + band_slots
        } else {
            EFFECT_PARAM_BASE as usize + specs.len()
        };
        let mut values = Vec::with_capacity(len);
        for spec in COMMON_PARAMS.iter() {
            values.push(AtomicU32::new(spec.default_value().as_f32().to_bits()));
        }
        for _ in COMMON_PARAMS.len()..EFFECT_PARAM_BASE as usize {
            values.push(AtomicU32::new(0.0f32.to_bits()));
        }
        for spec in specs.iter() {
            values.push(AtomicU32::new(spec.default_value().as_f32().to_bits()));
        }
        if band_slots > 0 {
            while values.len() < BAND_BASE as usize {
                values.push(AtomicU32::new(0.0f32.to_bits()));
            }
            for _ in 0..band_slots {
                values.push(AtomicU32::new(0.0f32.to_bits()));
            }
        }
        // Fill band defaults from the layout.
        if let Some(layout) = bands {
            let store = Self {
                values: values.into_boxed_slice(),
                specs,
                bands,
            };
            for ch in 0..layout.channels {
                for band in 0..layout.max_bands {
                    for (fi, field) in layout.fields.iter().enumerate() {
                        let id = band_id(layout, ch, band, fi);
                        store.set(id, field.kind.default_value());
                    }
                }
            }
            return store;
        }
        Self {
            values: values.into_boxed_slice(),
            specs,
            bands,
        }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn specs(&self) -> &'static [ParamSpec] {
        self.specs
    }

    pub fn bands(&self) -> Option<BandLayout> {
        self.bands
    }

    #[inline]
    pub fn get(&self, id: u16) -> ParamValue {
        let idx = id as usize;
        if idx >= self.values.len() {
            return ParamValue::F32(0.0);
        }
        ParamValue::F32(f32::from_bits(self.values[idx].load(Ordering::Relaxed)))
    }

    #[inline]
    pub fn get_f32(&self, id: u16) -> f32 {
        self.get(id).as_f32()
    }

    #[inline]
    pub fn get_bool(&self, id: u16) -> bool {
        self.get_f32(id) != 0.0
    }

    #[inline]
    pub fn get_enum(&self, id: u16) -> u16 {
        let v = self.get_f32(id);
        if v.is_finite() {
            v.max(0.0) as u16
        } else {
            0
        }
    }

    pub fn set(&self, id: u16, value: ParamValue) -> bool {
        let idx = id as usize;
        if idx >= self.values.len() {
            return false;
        }
        let coerced = self.coerce(id, value);
        self.values[idx].store(coerced.as_f32().to_bits(), Ordering::Relaxed);
        true
    }

    pub fn set_f32(&self, id: u16, value: f32) -> bool {
        self.set(id, ParamValue::F32(value))
    }

    fn coerce(&self, id: u16, value: ParamValue) -> ParamValue {
        if let Some(spec) = self.spec_for(id) {
            return spec.kind.coerce(value);
        }
        if let Some(layout) = self.bands {
            if let Some((_, _, _, field)) = self.decode_band(layout, id) {
                return field.kind.coerce(value);
            }
        }
        value
    }

    /// Value with the type declared by its spec (bool/int/enum/float).
    pub fn typed(&self, id: u16) -> ParamValue {
        let raw = self.get(id);
        if let Some(spec) = self.spec_for(id) {
            return match spec.kind {
                ParamKind::Bool { .. } => ParamValue::Bool(raw.as_f32() != 0.0),
                ParamKind::Int { .. } => ParamValue::Int(raw.as_f32().round() as i32),
                ParamKind::Enum { .. } => ParamValue::Enum(raw.as_u16()),
                ParamKind::Float { .. } => raw,
            };
        }
        if let Some(layout) = self.bands {
            if let Some((_, _, _, field)) = self.decode_band(layout, id) {
                return match field.kind {
                    ParamKind::Bool { .. } => ParamValue::Bool(raw.as_f32() != 0.0),
                    ParamKind::Int { .. } => ParamValue::Int(raw.as_f32().round() as i32),
                    ParamKind::Enum { .. } => ParamValue::Enum(raw.as_u16()),
                    ParamKind::Float { .. } => raw,
                };
            }
        }
        raw
    }

    pub fn spec_for(&self, id: u16) -> Option<&'static ParamSpec> {
        let idx = id as usize;
        if idx < COMMON_PARAMS.len() {
            return Some(&COMMON_PARAMS[idx]);
        }
        let off = idx.checked_sub(EFFECT_PARAM_BASE as usize)?;
        self.specs.get(off)
    }

    pub fn spec_by_key(&self, key: &str) -> Option<(u16, &'static ParamSpec)> {
        if let Some(i) = COMMON_PARAMS.iter().position(|s| s.key == key) {
            return Some((i as u16, &COMMON_PARAMS[i]));
        }
        self.specs
            .iter()
            .position(|s| s.key == key)
            .map(|i| (EFFECT_PARAM_BASE + i as u16, &self.specs[i]))
    }

    pub fn band_id(&self, ch: usize, band: usize, field: usize) -> Option<u16> {
        let layout = self.bands?;
        if ch >= layout.channels || band >= layout.max_bands || field >= layout.fields.len() {
            return None;
        }
        Some(band_id(layout, ch, band, field))
    }

    pub fn band_value(&self, ch: usize, band: usize, field: usize) -> f32 {
        match self.band_id(ch, band, field) {
            Some(id) => self.get_f32(id),
            None => 0.0,
        }
    }

    pub fn set_band(&self, ch: usize, band: usize, field: usize, v: ParamValue) -> bool {
        match self.band_id(ch, band, field) {
            Some(id) => self.set(id, v),
            None => false,
        }
    }

    fn decode_band(
        &self,
        layout: BandLayout,
        id: u16,
    ) -> Option<(usize, usize, usize, &'static BandField)> {
        let rel = (id as usize).checked_sub(BAND_BASE as usize)?;
        let per_ch = layout.max_bands * layout.fields.len();
        let ch = rel / per_ch;
        let rem = rel % per_ch;
        let band = rem / layout.fields.len();
        let fi = rem % layout.fields.len();
        if ch >= layout.channels || band >= layout.max_bands {
            return None;
        }
        Some((ch, band, fi, &layout.fields[fi]))
    }

    /// Serialize only the scalar parameters (common + effect specific).
    pub fn to_scalar_map(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        for (i, spec) in COMMON_PARAMS.iter().enumerate() {
            map.insert(spec.key.to_string(), value_to_json(self.typed(i as u16)));
        }
        for (i, spec) in self.specs.iter().enumerate() {
            map.insert(
                spec.key.to_string(),
                value_to_json(self.typed(EFFECT_PARAM_BASE + i as u16)),
            );
        }
        map
    }

    /// Serialize the whole store to a flat `key -> value` map. Band values are
    /// emitted as `left.band0.frequency` style keys.
    pub fn to_map(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        for (i, spec) in COMMON_PARAMS.iter().enumerate() {
            map.insert(spec.key.to_string(), value_to_json(self.typed(i as u16)));
        }
        for (i, spec) in self.specs.iter().enumerate() {
            map.insert(
                spec.key.to_string(),
                value_to_json(self.typed(EFFECT_PARAM_BASE + i as u16)),
            );
        }
        if let Some(layout) = self.bands {
            for ch in 0..layout.channels {
                let prefix = layout
                    .channel_labels
                    .get(ch)
                    .copied()
                    .unwrap_or(if ch == 0 { "left" } else { "right" });
                for band in 0..layout.max_bands {
                    for (fi, field) in layout.fields.iter().enumerate() {
                        let key = format!("{prefix}.band{band}.{}", field.key);
                        map.insert(
                            key,
                            value_to_json(self.typed(band_id(layout, ch, band, fi))),
                        );
                    }
                }
            }
        }
        map
    }
}

fn band_id(layout: BandLayout, ch: usize, band: usize, field: usize) -> u16 {
    BAND_BASE
        + (ch * layout.max_bands * layout.fields.len() + band * layout.fields.len() + field) as u16
}

pub fn value_to_json(v: ParamValue) -> serde_json::Value {
    match v {
        ParamValue::F32(f) => serde_json::Number::from_f64(f as f64)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        ParamValue::Bool(b) => serde_json::Value::Bool(b),
        ParamValue::Int(i) => serde_json::Value::Number(i.into()),
        ParamValue::Enum(e) => serde_json::Value::Number(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SPECS: [ParamSpec; 2] = [
        ParamSpec {
            key: "threshold",
            label: "Threshold",
            group: "Dynamics",
            kind: ParamKind::Float {
                min: -60.0,
                max: 0.0,
                default: -18.0,
                step: 0.1,
                unit: Unit::Db,
                log: false,
            },
            advanced: false,
        },
        ParamSpec {
            key: "mode",
            label: "Mode",
            group: "General",
            kind: ParamKind::Enum {
                default: 0,
                labels: &["Downward", "Upward"],
            },
            advanced: false,
        },
    ];

    #[test]
    fn defaults_and_clamping() {
        let store = ParamStore::new(&SPECS, None);
        assert_eq!(store.get_f32(EFFECT_PARAM_BASE), -18.0);
        assert!(store.set(EFFECT_PARAM_BASE, ParamValue::F32(-200.0)));
        assert_eq!(store.get_f32(EFFECT_PARAM_BASE), -60.0);
        assert!(store.set(EFFECT_PARAM_BASE + 1, ParamValue::Enum(9)));
        assert_eq!(store.get_enum(EFFECT_PARAM_BASE + 1), 1);
    }

    #[test]
    fn common_params_roundtrip() {
        let store = ParamStore::new(&SPECS, None);
        assert!(store.set(P_INPUT_GAIN, ParamValue::F32(-6.5)));
        assert_eq!(store.get_f32(P_INPUT_GAIN), -6.5);
        assert!(store.set(P_BYPASS, ParamValue::Bool(true)));
        assert!(store.get_bool(P_BYPASS));
    }
}
