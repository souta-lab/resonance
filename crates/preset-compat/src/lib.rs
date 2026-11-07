//! EasyEffects 8.3.2 preset compatibility.
//!
//! Goals:
//! * Parse both the modern (`equalizer#0`) and legacy (`equalizer`) instance
//!   notation.
//! * Preserve every unknown key so that a load → save round-trip is lossless.
//! * Validate known keys against the effect catalog, reporting warnings instead
//!   of silently dropping data.
//! * Export a file that EasyEffects can load again (sorted keys, 4-space
//!   indent, trailing newline).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dsp_core::param::{ParamKind, ParamSpec, ParamStore, ParamValue};
use dsp_core::{EffectKind, PipelineKind};
use dsp_effects::catalog;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, thiserror::Error)]
pub enum PresetError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("preset must contain exactly one of \"input\" or \"output\"")]
    MissingPipeline,
    #[error("\"plugins_order\" is missing or is not an array of strings")]
    InvalidOrder,
    #[error("instance \"{0}\" is not a JSON object")]
    InvalidInstance(String),
    #[error("asset path is not allowed: {0}")]
    UnsafeAsset(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Modern,
    Legacy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Warning {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl Warning {
    fn new(code: &str, message: impl Into<String>, instance: Option<&str>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            instance: instance.map(|s| s.to_string()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MissingAsset {
    pub instance: String,
    pub key: String,
    pub name: String,
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct Instance {
    /// Normalized id, always `kind#index`.
    pub id: String,
    pub kind: Option<EffectKind>,
    pub index: u32,
    /// Raw EasyEffects values, including unknown keys.
    pub values: Map<String, Value>,
}

impl Instance {
    pub fn raw(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn string(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(|v| v.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct Preset {
    pub kind: PipelineKind,
    pub blocklist: Vec<String>,
    pub plugins_order: Vec<String>,
    pub instances: IndexMap<String, Instance>,
    pub warnings: Vec<Warning>,
    pub missing_assets: Vec<MissingAsset>,
    pub source_format: SourceFormat,
}

#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    /// Emit the legacy notation (no `#0` suffix) when possible.
    pub legacy: bool,
    /// Sort object keys alphabetically (community preset validator style).
    pub sort_keys: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            legacy: false,
            sort_keys: true,
        }
    }
}

/// Split `equalizer#2` into (`equalizer`, 2). Bare names become index 0.
pub fn split_id(id: &str) -> (String, u32) {
    match id.rsplit_once('#') {
        Some((name, idx)) if !name.is_empty() => {
            let index = idx.parse::<u32>().unwrap_or(0);
            (name.to_string(), index)
        }
        _ => (id.to_string(), 0),
    }
}

pub fn normalize_id(id: &str) -> String {
    let (name, index) = split_id(id);
    format!("{name}#{index}")
}

fn effect_kind(name: &str) -> Option<EffectKind> {
    EffectKind::parse(name)
}

pub fn parse(json: &str) -> Result<Preset, PresetError> {
    let value: Value = serde_json::from_str(json)?;
    parse_value(value)
}

pub fn parse_value(value: Value) -> Result<Preset, PresetError> {
    let root = value.as_object().ok_or(PresetError::MissingPipeline)?;

    let has_input = root.contains_key("input");
    let has_output = root.contains_key("output");
    let (kind, body) = match (has_input, has_output) {
        (true, false) => (PipelineKind::Input, root.get("input")),
        (false, true) => (PipelineKind::Output, root.get("output")),
        _ => return Err(PresetError::MissingPipeline),
    };
    let body = body
        .and_then(|v| v.as_object())
        .ok_or(PresetError::MissingPipeline)?;

    let mut warnings = Vec::new();

    let blocklist = body
        .get("blocklist")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let order_value = body.get("plugins_order");
    let mut source_format = SourceFormat::Modern;
    let raw_order: Vec<String> = match order_value {
        Some(Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item.as_str() {
                    Some(s) => out.push(s.to_string()),
                    None => warnings.push(Warning::new(
                        "invalid_order_entry",
                        "plugins_order contains a non-string entry; it was ignored",
                        None,
                    )),
                }
            }
            out
        }
        Some(_) => return Err(PresetError::InvalidOrder),
        None => {
            // Legacy files without plugins_order: derive from instance keys.
            warnings.push(Warning::new(
                "missing_plugins_order",
                "plugins_order was missing; the order was derived from the object keys",
                None,
            ));
            body.keys()
                .filter(|k| *k != "blocklist" && *k != "plugins_order")
                .filter(|k| effect_kind(&split_id(k).0).is_some())
                .cloned()
                .collect()
        }
    };

    let mut plugins_order = Vec::with_capacity(raw_order.len());
    let mut instances: IndexMap<String, Instance> = IndexMap::new();

    for raw_id in raw_order {
        let (name, index) = split_id(&raw_id);
        if !raw_id.contains('#') {
            source_format = SourceFormat::Legacy;
        }
        let id = format!("{name}#{index}");
        if plugins_order.contains(&id) {
            warnings.push(Warning::new(
                "duplicate_instance",
                format!("duplicate instance id \"{id}\" was ignored"),
                Some(&id),
            ));
            continue;
        }
        let kind = effect_kind(&name);
        if kind.is_none() {
            warnings.push(Warning::new(
                "unknown_plugin",
                format!("unknown effect type \"{name}\"; the instance is preserved but cannot be processed"),
                Some(&id),
            ));
        }
        let values = match body.get(&raw_id).or_else(|| body.get(&id)) {
            Some(Value::Object(map)) => map.clone(),
            Some(_) => return Err(PresetError::InvalidInstance(raw_id.clone())),
            None => {
                warnings.push(Warning::new(
                    "missing_instance",
                    format!("plugins_order references \"{raw_id}\" but no settings object exists"),
                    Some(&id),
                ));
                Map::new()
            }
        };
        plugins_order.push(id.clone());
        instances.insert(
            id.clone(),
            Instance {
                id,
                kind,
                index,
                values,
            },
        );
    }

    // Instances present in the file but not referenced by plugins_order are
    // preserved as warnings so that a round-trip does not lose them.
    for (key, value) in body.iter() {
        if key == "blocklist" || key == "plugins_order" {
            continue;
        }
        let (name, index) = split_id(key);
        if effect_kind(&name).is_none() {
            continue;
        }
        let id = format!("{name}#{index}");
        if !instances.contains_key(&id) {
            warnings.push(Warning::new(
                "orphan_instance",
                format!(
                    "\"{key}\" is not listed in plugins_order; it was preserved but not loaded"
                ),
                Some(&id),
            ));
            if let Value::Object(map) = value {
                instances.insert(
                    id.clone(),
                    Instance {
                        id,
                        kind: effect_kind(&name),
                        index,
                        values: map.clone(),
                    },
                );
            }
        }
    }

    let mut preset = Preset {
        kind,
        blocklist,
        plugins_order,
        instances,
        warnings,
        missing_assets: Vec::new(),
        source_format,
    };
    preset.warnings.extend(validate(&preset));
    Ok(preset)
}

/// Validate known keys against the catalog. Unknown keys are not warnings.
pub fn validate(preset: &Preset) -> Vec<Warning> {
    let mut warnings = Vec::new();
    for id in &preset.plugins_order {
        let Some(instance) = preset.instances.get(id) else {
            continue;
        };
        let Some(kind) = instance.kind else {
            continue;
        };
        let desc = catalog::descriptor(kind);
        for (key, value) in instance.values.iter() {
            if key == "bypass"
                || key == "input-gain"
                || key == "output-gain"
                || key == "dry"
                || key == "wet"
            {
                if !value.is_number() && !value.is_boolean() {
                    warnings.push(Warning::new(
                        "invalid_value",
                        format!("\"{key}\" should be a number or boolean"),
                        Some(id),
                    ));
                }
                continue;
            }
            if let Some(spec) = desc.params.iter().find(|p| p.key == key) {
                if json_to_param(&spec.kind, value).is_none() {
                    warnings.push(Warning::new(
                        "invalid_value",
                        format!("\"{key}\" has an unexpected type or enum label"),
                        Some(id),
                    ));
                }
                continue;
            }
            if is_band_key(key) {
                if !value.is_object() {
                    warnings.push(Warning::new(
                        "invalid_band",
                        format!("\"{key}\" should be an object"),
                        Some(id),
                    ));
                }
                continue;
            }
            // Unknown key: preserved silently by design.
        }
    }
    warnings
}

fn is_band_key(key: &str) -> bool {
    let Some((channel, rest)) = key.split_once('.') else {
        return false;
    };
    if !matches!(channel, "left" | "right" | "mid" | "side") {
        return false;
    }
    rest.starts_with("band")
}

fn channel_index(channel: &str) -> Option<usize> {
    match channel {
        "left" | "mid" => Some(0),
        "right" | "side" => Some(1),
        _ => None,
    }
}

fn band_index(key: &str) -> Option<usize> {
    key.strip_prefix("band")?.parse::<usize>().ok()
}

pub fn json_to_param(kind: &ParamKind, value: &Value) -> Option<ParamValue> {
    match kind {
        ParamKind::Bool { .. } => value.as_bool().map(ParamValue::Bool),
        ParamKind::Int { .. } => value.as_i64().map(|v| ParamValue::Int(v as i32)),
        ParamKind::Float { .. } => value.as_f64().map(|v| ParamValue::F32(v as f32)),
        ParamKind::Enum { labels, .. } => {
            if let Some(s) = value.as_str() {
                labels
                    .iter()
                    .position(|l| l.eq_ignore_ascii_case(s))
                    .map(|i| ParamValue::Enum(i as u16))
            } else {
                value.as_u64().map(|v| ParamValue::Enum(v as u16))
            }
        }
    }
}

pub fn param_to_json(kind: &ParamKind, value: ParamValue) -> Value {
    match kind {
        ParamKind::Bool { .. } => Value::Bool(value.as_bool()),
        ParamKind::Int { .. } => Value::Number(value.as_i32().into()),
        ParamKind::Float { .. } => serde_json::Number::from_f64(value.as_f32() as f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ParamKind::Enum { labels, .. } => {
            let idx = value.as_u16() as usize;
            labels
                .get(idx)
                .map(|l| Value::String((*l).to_string()))
                .unwrap_or_else(|| Value::Number(idx.into()))
        }
    }
}

const BAND_TYPE_LABELS: [&str; 8] = [
    "Bell",
    "Lo-shelf",
    "Hi-shelf",
    "Lo-pass",
    "Hi-pass",
    "Notch",
    "Band-pass",
    "All-pass",
];

fn band_type_index(s: &str) -> Option<u16> {
    let norm = s.to_ascii_lowercase().replace([' ', '_'], "-");
    BAND_TYPE_LABELS
        .iter()
        .position(|l| l.to_ascii_lowercase().replace([' ', '_'], "-") == norm)
        .map(|i| i as u16)
}

/// Apply an instance's values to a parameter store. Returns warnings for values
/// that could not be applied; unknown keys are ignored here but stay in the
/// preset.
pub fn apply_instance(instance: &Instance, store: &ParamStore) -> Vec<Warning> {
    let mut warnings = Vec::new();
    let Some(kind) = instance.kind else {
        return warnings;
    };
    let desc = catalog::descriptor(kind);

    for (key, value) in instance.values.iter() {
        if let Some((id, spec)) = store.spec_by_key(key) {
            match json_to_param(&spec.kind, value) {
                Some(v) => {
                    store.set(id, v);
                }
                None => warnings.push(Warning::new(
                    "invalid_value",
                    format!("\"{key}\" could not be applied"),
                    Some(&instance.id),
                )),
            }
            continue;
        }
        if is_band_key(key) {
            let Some((channel, band_part)) = key.split_once('.') else {
                continue;
            };
            let Some(ch) = channel_index(channel) else {
                continue;
            };
            let Some(band) = band_index(band_part) else {
                continue;
            };
            let Some(layout) = desc.bands else {
                continue;
            };
            let Value::Object(fields) = value else {
                warnings.push(Warning::new(
                    "invalid_band",
                    format!("\"{key}\" should be an object"),
                    Some(&instance.id),
                ));
                continue;
            };
            for (field, field_value) in fields.iter() {
                let Some(fi) = layout.field_index(field) else {
                    continue;
                };
                let Some(field_spec) = layout.fields.get(fi) else {
                    continue;
                };
                let parsed = if field == "type" {
                    field_value
                        .as_str()
                        .and_then(band_type_index)
                        .map(ParamValue::Enum)
                        .or_else(|| field_value.as_u64().map(|v| ParamValue::Enum(v as u16)))
                } else {
                    json_to_param(&field_spec.kind, field_value)
                };
                match parsed {
                    Some(v) => {
                        store.set_band(ch, band, fi, v);
                    }
                    None => warnings.push(Warning::new(
                        "invalid_band_value",
                        format!("\"{key}.{field}\" could not be applied"),
                        Some(&instance.id),
                    )),
                }
            }
        }
    }
    warnings
}

/// Build the EasyEffects instance object for an effect from its live state.
pub fn capture_instance(
    kind: EffectKind,
    store: &ParamStore,
    extra: &Map<String, Value>,
) -> Map<String, Value> {
    let desc = catalog::descriptor(kind);
    let mut out = Map::new();

    for spec in dsp_core::param::COMMON_PARAMS.iter() {
        if let Some((id, _)) = store.spec_by_key(spec.key) {
            out.insert(
                spec.key.to_string(),
                param_to_json(&spec.kind, store.get(id)),
            );
        }
    }
    for spec in desc.params.iter() {
        if let Some((id, _)) = store.spec_by_key(spec.key) {
            out.insert(
                spec.key.to_string(),
                param_to_json(&spec.kind, store.get(id)),
            );
        }
    }
    if let Some(layout) = desc.bands {
        for (ch, label) in layout.channel_labels.iter().enumerate() {
            for band in 0..layout.max_bands {
                let mut obj = Map::new();
                for (fi, field) in layout.fields.iter().enumerate() {
                    let Some(id) = store.band_id(ch, band, fi) else {
                        continue;
                    };
                    let value = store.get(id);
                    let json = if field.key == "type" {
                        BAND_TYPE_LABELS
                            .get(value.as_u16() as usize)
                            .map(|l| Value::String((*l).to_string()))
                            .unwrap_or(Value::Null)
                    } else {
                        param_to_json(&field.kind, value)
                    };
                    obj.insert(field.key.to_string(), json);
                }
                out.insert(format!("{label}.band{band}"), Value::Object(obj));
            }
        }
    }
    // Preserve unknown keys from the source preset.
    for (k, v) in extra.iter() {
        out.entry(k.clone()).or_insert_with(|| v.clone());
    }
    out
}

/// Serialize a preset back to EasyEffects JSON.
pub fn to_json(preset: &Preset, options: ExportOptions) -> String {
    let mut body = Map::new();
    body.insert(
        "blocklist".to_string(),
        Value::Array(
            preset
                .blocklist
                .iter()
                .map(|s| Value::String(s.clone()))
                .collect(),
        ),
    );

    let legacy = options.legacy && preset.source_format == SourceFormat::Legacy;
    let order: Vec<Value> = preset
        .plugins_order
        .iter()
        .map(|id| {
            if legacy {
                let (name, index) = split_id(id);
                if index == 0 {
                    Value::String(name)
                } else {
                    Value::String(id.clone())
                }
            } else {
                Value::String(id.clone())
            }
        })
        .collect();
    body.insert("plugins_order".to_string(), Value::Array(order));

    for id in &preset.plugins_order {
        let Some(instance) = preset.instances.get(id) else {
            continue;
        };
        let key = if legacy {
            let (name, index) = split_id(id);
            if index == 0 {
                name
            } else {
                id.clone()
            }
        } else {
            id.clone()
        };
        body.insert(key, Value::Object(instance.values.clone()));
    }

    let mut root = Map::new();
    root.insert(preset.kind.as_str().to_string(), Value::Object(body));

    let value = if options.sort_keys {
        sort_value(Value::Object(root))
    } else {
        Value::Object(root)
    };
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string());
    text.push('\n');
    text
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted: BTreeMap<String, Value> = BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k, sort_value(v));
            }
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_value).collect()),
        other => other,
    }
}

/// Directories used to resolve IRs and models.
#[derive(Clone, Debug, Default)]
pub struct AssetDirs {
    pub irs: Vec<PathBuf>,
    pub models: Vec<PathBuf>,
}

fn safe_name(name: &str) -> Option<&str> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return None;
    }
    Some(name)
}

fn find_asset(dirs: &[PathBuf], name: &str, extensions: &[&str]) -> Option<PathBuf> {
    let safe = safe_name(name)?;
    for dir in dirs {
        let direct = dir.join(safe);
        if direct.is_file() {
            return Some(direct);
        }
        for ext in extensions {
            let candidate = dir.join(format!("{safe}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Resolve a single asset name against the configured directories.
pub fn resolve_asset_path(
    dirs: &AssetDirs,
    kind: dsp_effects::AssetKind,
    name: &str,
    extensions: &[&str],
) -> Option<PathBuf> {
    let dirs_for = match kind {
        dsp_effects::AssetKind::ImpulseResponse => &dirs.irs,
        dsp_effects::AssetKind::Model => &dirs.models,
    };
    find_asset(dirs_for, name, extensions)
}

/// Resolve IR/model references and report anything missing. Never silently
/// skips: a missing asset is always reported.
pub fn resolve_assets(preset: &Preset, dirs: &AssetDirs) -> Vec<MissingAsset> {
    let mut missing = Vec::new();
    for id in &preset.plugins_order {
        let Some(instance) = preset.instances.get(id) else {
            continue;
        };
        let Some(kind) = instance.kind else {
            continue;
        };
        let desc = catalog::descriptor(kind);
        for asset in desc.assets {
            let name = instance
                .string(asset.key)
                .or_else(|| instance.string("kernel-path"))
                .map(|s| {
                    Path::new(s)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(s)
                        .to_string()
                });
            let Some(name) = name else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let dirs_for = match asset.kind {
                dsp_effects::AssetKind::ImpulseResponse => &dirs.irs,
                dsp_effects::AssetKind::Model => &dirs.models,
            };
            if find_asset(dirs_for, &name, asset.extensions).is_none() {
                missing.push(MissingAsset {
                    instance: id.clone(),
                    key: asset.key.to_string(),
                    name,
                    kind: match asset.kind {
                        dsp_effects::AssetKind::ImpulseResponse => "impulse_response".to_string(),
                        dsp_effects::AssetKind::Model => "model".to_string(),
                    },
                });
            }
        }
    }
    missing
}

/// Build a preset from a live pipeline description.
pub fn from_instances(
    kind: PipelineKind,
    blocklist: Vec<String>,
    order: Vec<String>,
    instances: IndexMap<String, Instance>,
) -> Preset {
    let mut preset = Preset {
        kind,
        blocklist,
        plugins_order: order,
        instances,
        warnings: Vec::new(),
        missing_assets: Vec::new(),
        source_format: SourceFormat::Modern,
    };
    preset.warnings = validate(&preset);
    preset
}

/// A minimal, valid preset used by tests and the "new preset" action.
pub fn empty_preset(kind: PipelineKind) -> Preset {
    Preset {
        kind,
        blocklist: Vec::new(),
        plugins_order: Vec::new(),
        instances: IndexMap::new(),
        warnings: Vec::new(),
        missing_assets: Vec::new(),
        source_format: SourceFormat::Modern,
    }
}

/// Helper for the daemon: the catalog spec for a key, if any.
pub fn spec_for(kind: EffectKind, key: &str) -> Option<&'static ParamSpec> {
    catalog::descriptor(kind)
        .params
        .iter()
        .find(|p| p.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "output": {
            "blocklist": ["Firefox"],
            "plugins_order": ["equalizer#0", "compressor#0"],
            "equalizer#0": {
                "bypass": false,
                "input-gain": -2.5,
                "num-bands": 10,
                "left.band0": { "type": "Bell", "frequency": 100, "gain": 3.5, "q": 1.2, "width": 4, "slope": 0, "solo": false, "mute": false },
                "future-key": { "nested": [1, 2, 3] }
            },
            "compressor#0": { "bypass": true, "threshold": -20.0, "ratio": 3.0, "mode": "Downward" }
        }
    }"#;

    #[test]
    fn parses_modern_preset() {
        let p = parse(SAMPLE).unwrap();
        assert_eq!(p.kind, PipelineKind::Output);
        assert_eq!(p.plugins_order, vec!["equalizer#0", "compressor#0"]);
        assert_eq!(p.blocklist, vec!["Firefox"]);
        assert_eq!(p.instances.len(), 2);
        assert_eq!(p.source_format, SourceFormat::Modern);
    }

    #[test]
    fn preserves_unknown_keys_on_roundtrip() {
        let p = parse(SAMPLE).unwrap();
        let text = to_json(&p, ExportOptions::default());
        let again = parse(&text).unwrap();
        let eq = again.instances.get("equalizer#0").unwrap();
        assert_eq!(
            eq.values.get("future-key").unwrap(),
            &serde_json::json!({ "nested": [1, 2, 3] })
        );
        assert_eq!(
            eq.values.get("input-gain").unwrap(),
            &serde_json::json!(-2.5)
        );
    }

    #[test]
    fn normalizes_legacy_ids() {
        let legacy = r#"{
            "output": {
                "plugins_order": ["equalizer", "limiter"],
                "equalizer": { "bypass": false },
                "limiter": { "bypass": false }
            }
        }"#;
        let p = parse(legacy).unwrap();
        assert_eq!(p.plugins_order, vec!["equalizer#0", "limiter#0"]);
        assert_eq!(p.source_format, SourceFormat::Legacy);
        let text = to_json(
            &p,
            ExportOptions {
                legacy: true,
                sort_keys: true,
            },
        );
        assert!(text.contains("\"equalizer\""));
        assert!(!text.contains("equalizer#0"));
    }

    #[test]
    fn rejects_missing_pipeline() {
        assert!(matches!(
            parse(r#"{"foo": {}}"#),
            Err(PresetError::MissingPipeline)
        ));
    }

    #[test]
    fn reports_unknown_plugin() {
        let p = parse(r#"{"output":{"plugins_order":["mystery#0"],"mystery#0":{"bypass":false}}}"#)
            .unwrap();
        assert!(p.warnings.iter().any(|w| w.code == "unknown_plugin"));
        assert_eq!(p.instances.len(), 1);
    }

    #[test]
    fn applies_values_and_bands() {
        let p = parse(SAMPLE).unwrap();
        let eq = p.instances.get("equalizer#0").unwrap();
        let desc = catalog::descriptor(EffectKind::Equalizer);
        let store = ParamStore::new(desc.params, desc.bands);
        let warnings = apply_instance(eq, &store);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            store.get_f32(store.spec_by_key("input-gain").unwrap().0),
            -2.5
        );
        assert_eq!(
            store.get_f32(store.spec_by_key("num-bands").unwrap().0),
            10.0
        );
        assert_eq!(store.band_value(0, 0, 1), 100.0);
        assert_eq!(store.band_value(0, 0, 2), 3.5);
        assert_eq!(store.band_value(0, 0, 0), 0.0);
    }

    #[test]
    fn capture_roundtrip_keeps_band_values() {
        let p = parse(SAMPLE).unwrap();
        let eq = p.instances.get("equalizer#0").unwrap();
        let desc = catalog::descriptor(EffectKind::Equalizer);
        let store = ParamStore::new(desc.params, desc.bands);
        apply_instance(eq, &store);
        let captured = capture_instance(EffectKind::Equalizer, &store, &Map::new());
        let band = captured.get("left.band0").unwrap();
        assert_eq!(band.get("frequency").unwrap(), &serde_json::json!(100.0));
        assert_eq!(band.get("type").unwrap(), &serde_json::json!("Bell"));
    }

    #[test]
    fn rejects_path_traversal() {
        let preset = parse(
            r#"{"output":{"plugins_order":["convolver#0"],"convolver#0":{"kernel-name":"../../etc/passwd"}}}"#,
        )
        .unwrap();
        let missing = resolve_assets(&preset, &AssetDirs::default());
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].name, "passwd");
    }

    #[test]
    fn missing_ir_is_reported() {
        let preset = parse(
            r#"{"output":{"plugins_order":["convolver#0"],"convolver#0":{"kernel-name":"studio-hall"}}}"#,
        )
        .unwrap();
        let missing = resolve_assets(&preset, &AssetDirs::default());
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].key, "kernel-name");
    }

    #[test]
    fn export_is_sorted_and_newline_terminated() {
        let p = parse(SAMPLE).unwrap();
        let text = to_json(&p, ExportOptions::default());
        assert!(text.ends_with('\n'));
        let v: Value = serde_json::from_str(&text).unwrap();
        let body = v.get("output").unwrap().as_object().unwrap();
        let keys: Vec<&String> = body.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}
