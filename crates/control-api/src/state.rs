//! Application state: pipelines, presets, host snapshot, events.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use dsp_core::graph::{GraphSwap, Pipeline, PipelineKind, Slot};
use dsp_core::meter::{MeterBank, SlotMeters};
use dsp_core::param::{ParamStore, ParamValue, P_BYPASS};
use dsp_core::EffectKind;
use dsp_effects::catalog;
use indexmap::IndexMap;
use parking_lot::RwLock;
use preset_compat::{AssetDirs, Instance, Preset, Warning};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tokio::sync::broadcast;

use crate::error::{ApiError, ApiResult};

// ---------------------------------------------------------------------------
// Host snapshot (filled by the daemon from pw-backend)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize)]
pub struct VirtualNodes {
    pub output: String,
    pub input: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PipewireInfo {
    pub available: bool,
    pub version: String,
    pub server: String,
    pub virtual_nodes: VirtualNodes,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DeviceInfo {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub nick: String,
    pub is_default: bool,
    pub state: String,
    pub profile: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DevicesInfo {
    pub outputs: Vec<DeviceInfo>,
    pub inputs: Vec<DeviceInfo>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct StreamInfo {
    pub id: u32,
    pub app: String,
    pub name: String,
    pub media: String,
    pub direction: String,
    pub linked: bool,
    pub blocked: bool,
    pub target: String,
}

#[derive(Clone, Debug, Default)]
pub struct HostSnapshot {
    pub pipewire: PipewireInfo,
    pub devices: DevicesInfo,
    pub streams: Vec<StreamInfo>,
    pub mode: String,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub bind: String,
    pub port: u16,
    pub theme: String,
    pub meter_rate_hz: u32,
    pub log_level: String,
    pub autoload: bool,
    pub virtual_nodes: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".to_string(),
            port: 8737,
            theme: "bench".to_string(),
            meter_rate_hz: 30,
            log_level: "info".to_string(),
            autoload: true,
            virtual_nodes: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutoloadRule {
    pub id: String,
    pub device: String,
    pub route: String,
    pub pipeline: PipelineKind,
    pub preset: String,
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    Meter {
        pipeline: PipelineKind,
        timestamp_ms: u64,
        peak_dbfs: [f32; 2],
        true_peak_dbfs: [f32; 2],
        rms_dbfs: [f32; 2],
        lufs_m: f32,
        lufs_s: f32,
        gain_reduction_db: f32,
        dsp_p99_us: f32,
        xruns: u32,
    },
    Spectrum {
        pipeline: PipelineKind,
        bins: Vec<f32>,
        sample_rate: u32,
    },
    Graph {
        pipeline: PipelineKind,
        revision: u64,
    },
    Device {
        action: String,
        device: DeviceInfo,
    },
    Error {
        code: String,
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Pipeline state
// ---------------------------------------------------------------------------

pub struct SlotHandle {
    pub id: String,
    pub kind: EffectKind,
    pub params: Arc<ParamStore>,
    pub meters: Arc<SlotMeters>,
    /// Raw preset values, including unknown keys, for lossless export.
    pub extra: Map<String, Value>,
    /// Resolved external assets (asset key, path) loaded into the effect.
    pub assets: Vec<(String, std::path::PathBuf)>,
}

impl SlotHandle {
    pub fn new(kind: EffectKind, id: String) -> Self {
        let desc = catalog::descriptor(kind);
        Self {
            id,
            kind,
            params: Arc::new(ParamStore::new(desc.params, desc.bands)),
            meters: Arc::new(SlotMeters::default()),
            extra: Map::new(),
            assets: Vec::new(),
        }
    }
}

pub struct PipelineState {
    pub kind: PipelineKind,
    pub bypass: bool,
    pub revision: u64,
    pub slots: Vec<SlotHandle>,
    pub swap: Arc<GraphSwap>,
    pub meters: Arc<MeterBank>,
    pub blocklist: Vec<String>,
}

impl PipelineState {
    pub fn new(kind: PipelineKind) -> Self {
        Self {
            kind,
            bypass: false,
            revision: 1,
            slots: Vec::new(),
            swap: GraphSwap::new(),
            meters: Arc::new(MeterBank::default()),
            blocklist: Vec::new(),
        }
    }

    pub fn next_instance_id(&self, kind: EffectKind) -> String {
        let base = kind.as_str();
        let mut index = 0u32;
        loop {
            let id = format!("{base}#{index}");
            if !self.slots.iter().any(|s| s.id == id) {
                return id;
            }
            index += 1;
        }
    }

    pub fn latency_frames(&self) -> u32 {
        self.slots
            .iter()
            .map(|s| s.meters.latency_frames.load(Ordering::Relaxed))
            .sum()
    }

    pub fn rebuild(
        &self,
        sample_rate: u32,
        max_frames: usize,
        channels: usize,
    ) -> ApiResult<Box<Pipeline>> {
        let mut pipeline = Pipeline::new(self.kind, sample_rate, max_frames, channels);
        pipeline.bypass = self.bypass;
        for slot in &self.slots {
            let mut effect =
                dsp_effects::build(slot.kind, sample_rate, max_frames, channels, &slot.params)
                    .map_err(|e| {
                        ApiError::pipeline_error(format!("{} could not be prepared: {e}", slot.id))
                    })?;
            for (key, path) in &slot.assets {
                effect.load_asset(key, path).map_err(|e| {
                    ApiError::pipeline_error(format!(
                        "{} could not load {key} from {}: {e}",
                        slot.id,
                        path.display()
                    ))
                })?;
            }
            slot.meters
                .latency_frames
                .store(effect.latency_frames() as u32, Ordering::Relaxed);
            pipeline.add_slot(Slot::new(
                slot.id.clone(),
                slot.kind,
                slot.params.clone(),
                effect,
                slot.meters.clone(),
            ));
        }
        Ok(Box::new(pipeline))
    }

    pub fn install(&self, sample_rate: u32, max_frames: usize, channels: usize) -> ApiResult<()> {
        let pipeline = self.rebuild(sample_rate, max_frames, channels)?;
        let retired = self.swap.install(pipeline);
        drop(retired);
        Ok(())
    }

    pub fn effect_json(&self, index: usize) -> Option<Value> {
        let slot = self.slots.get(index)?;
        let desc = catalog::descriptor(slot.kind);
        let params = slot.params.to_scalar_map();
        // Keep the common keys first for readability.
        const COMMON: [&str; 5] = ["input-gain", "output-gain", "dry", "wet", "bypass"];
        let mut ordered = Map::new();
        for key in COMMON {
            if let Some(v) = params.get(key) {
                ordered.insert(key.to_string(), v.clone());
            }
        }
        for (k, v) in params {
            if !COMMON.contains(&k.as_str()) {
                ordered.insert(k, v);
            }
        }
        let bands = desc.bands.map(|layout| {
            let mut out = Map::new();
            for (ch, label) in layout.channel_labels.iter().enumerate() {
                let mut arr = Vec::with_capacity(layout.max_bands);
                for band in 0..layout.max_bands {
                    let mut obj = Map::new();
                    for (fi, field) in layout.fields.iter().enumerate() {
                        let Some(id) = slot.params.band_id(ch, band, fi) else {
                            continue;
                        };
                        obj.insert(
                            field.key.to_string(),
                            dsp_core::param::value_to_json(slot.params.typed(id)),
                        );
                    }
                    arr.push(Value::Object(obj));
                }
                out.insert((*label).to_string(), Value::Array(arr));
            }
            Value::Object(out)
        });
        Some(json!({
            "id": slot.id,
            "kind": slot.kind.as_str(),
            "name": desc.name,
            "bypass": slot.params.get_bool(P_BYPASS),
            "index": index,
            "compat": desc.compat,
            "latency_frames": slot.meters.latency_frames.load(Ordering::Relaxed),
            "gain_reduction_db": slot.meters.gain_reduction_db.get(),
            "params": ordered,
            "bands": bands,
        }))
    }

    pub fn pipeline_json(&self) -> Value {
        let effects: Vec<Value> = (0..self.slots.len())
            .filter_map(|i| self.effect_json(i))
            .collect();
        json!({
            "kind": self.kind,
            "bypass": self.bypass,
            "revision": self.revision,
            "latency_frames": self.latency_frames(),
            "effects": effects,
        })
    }

    pub fn capture_preset(&self, name: &str) -> Preset {
        let mut instances = IndexMap::new();
        let mut order = Vec::new();
        for slot in &self.slots {
            let values = preset_compat::capture_instance(slot.kind, &slot.params, &slot.extra);
            order.push(slot.id.clone());
            instances.insert(
                slot.id.clone(),
                Instance {
                    id: slot.id.clone(),
                    kind: Some(slot.kind),
                    index: 0,
                    values,
                },
            );
        }
        let mut preset =
            preset_compat::from_instances(self.kind, self.blocklist.clone(), order, instances);
        preset.warnings.clear();
        let _ = name;
        preset
    }
}

pub struct PipelineManager {
    pub sample_rate: u32,
    pub max_frames: usize,
    pub channels: usize,
    pub output: PipelineState,
    pub input: PipelineState,
}

impl PipelineManager {
    pub fn new(sample_rate: u32, max_frames: usize, channels: usize) -> Self {
        Self {
            sample_rate,
            max_frames,
            channels,
            output: PipelineState::new(PipelineKind::Output),
            input: PipelineState::new(PipelineKind::Input),
        }
    }

    pub fn state(&self, kind: PipelineKind) -> &PipelineState {
        match kind {
            PipelineKind::Output => &self.output,
            PipelineKind::Input => &self.input,
        }
    }

    pub fn state_mut(&mut self, kind: PipelineKind) -> &mut PipelineState {
        match kind {
            PipelineKind::Output => &mut self.output,
            PipelineKind::Input => &mut self.input,
        }
    }

    pub fn install(&self, kind: PipelineKind) -> ApiResult<()> {
        self.state(kind)
            .install(self.sample_rate, self.max_frames, self.channels)
    }

    pub fn build_initial(&self, kind: PipelineKind) -> ApiResult<Box<Pipeline>> {
        self.state(kind)
            .rebuild(self.sample_rate, self.max_frames, self.channels)
    }
}

// ---------------------------------------------------------------------------
// Preset store
// ---------------------------------------------------------------------------

pub struct PresetStore {
    pub dir: PathBuf,
    pub presets: IndexMap<String, Preset>,
}

impl PresetStore {
    pub fn load(dir: PathBuf) -> Self {
        let mut presets = IndexMap::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                match preset_compat::parse(&text) {
                    Ok(preset) => {
                        let id = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("preset")
                            .to_string();
                        presets.insert(id, preset);
                    }
                    Err(err) => {
                        tracing::warn!(path = %path.display(), %err, "skipping invalid preset");
                    }
                }
            }
        }
        Self { dir, presets }
    }

    pub fn slug(name: &str) -> String {
        let mut out = String::new();
        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
            } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !out.ends_with('-') {
                out.push('-');
            }
        }
        let trimmed = out.trim_matches('-').to_string();
        if trimmed.is_empty() {
            "preset".to_string()
        } else {
            trimmed
        }
    }

    pub fn unique_id(&self, name: &str) -> String {
        let base = Self::slug(name);
        if !self.presets.contains_key(&base) {
            return base;
        }
        for i in 2.. {
            let candidate = format!("{base}-{i}");
            if !self.presets.contains_key(&candidate) {
                return candidate;
            }
        }
        unreachable!()
    }

    pub fn save(&self, id: &str, preset: &Preset) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let text = preset_compat::to_json(preset, preset_compat::ExportOptions::default());
        std::fs::write(self.dir.join(format!("{id}.json")), text)
    }

    pub fn delete(&mut self, id: &str) -> std::io::Result<()> {
        self.presets.shift_remove(id);
        let path = self.dir.join(format!("{id}.json"));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn list_json(&self) -> Vec<Value> {
        self.presets
            .iter()
            .map(|(id, p)| {
                json!({
                    "id": id,
                    "name": display_name(id),
                    "kind": p.kind,
                    "source": "local",
                    "effects": p.plugins_order.len(),
                    "missing_assets": p.missing_assets,
                    "updated_at": "",
                })
            })
            .collect()
    }

    pub fn preset_json(&self, id: &str) -> Option<Value> {
        let p = self.presets.get(id)?;
        Some(json!({
            "id": id,
            "name": display_name(id),
            "kind": p.kind,
            "blocklist": p.blocklist,
            "plugins_order": p.plugins_order,
            "instances": p.instances.iter().map(|(k, v)| (k.clone(), Value::Object(v.values.clone()))).collect::<Map<_, _>>(),
            "warnings": p.warnings,
            "missing_assets": p.missing_assets,
        }))
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct Inner {
    pub started: Instant,
    pub config: RwLock<Config>,
    pub manager: RwLock<PipelineManager>,
    pub presets: RwLock<PresetStore>,
    pub host: RwLock<HostSnapshot>,
    pub autoload: RwLock<Vec<AutoloadRule>>,
    pub events: broadcast::Sender<ServerEvent>,
    pub token: String,
    pub web_dir: PathBuf,
    pub data_dir: PathBuf,
    pub asset_dirs: AssetDirs,
    pub engine_ready: AtomicBool,
    pub revision: AtomicU64,
}

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Inner>,
}

impl AppState {
    pub fn new(
        data_dir: PathBuf,
        web_dir: PathBuf,
        sample_rate: u32,
        max_frames: usize,
        channels: usize,
    ) -> Self {
        let (events, _) = broadcast::channel(64);
        let presets_dir = data_dir.join("presets");
        let asset_dirs = AssetDirs {
            irs: vec![
                data_dir.join("irs"),
                PathBuf::from("/usr/share/easyeffects/irs"),
            ],
            models: vec![data_dir.join("models")],
        };
        let mut preset_store = PresetStore::load(presets_dir);
        for preset in preset_store.presets.values_mut() {
            preset.missing_assets = preset_compat::resolve_assets(preset, &asset_dirs);
        }
        let config = load_config(&data_dir);
        let autoload = load_autoload(&data_dir);
        Self {
            inner: Arc::new(Inner {
                started: Instant::now(),
                config: RwLock::new(config),
                manager: RwLock::new(PipelineManager::new(sample_rate, max_frames, channels)),
                presets: RwLock::new(preset_store),
                host: RwLock::new(HostSnapshot::default()),
                autoload: RwLock::new(autoload),
                events,
                token: generate_token(),
                web_dir,
                data_dir,
                asset_dirs,
                engine_ready: AtomicBool::new(false),
                revision: AtomicU64::new(1),
            }),
        }
    }

    pub fn broadcast(&self, event: ServerEvent) {
        let _ = self.inner.events.send(event);
    }

    pub fn bump(&self, kind: PipelineKind) -> u64 {
        let mut manager = self.inner.manager.write();
        let state = manager.state_mut(kind);
        state.revision += 1;
        let revision = state.revision;
        drop(manager);
        self.broadcast(ServerEvent::Graph {
            pipeline: kind,
            revision,
        });
        revision
    }

    pub fn status_json(&self) -> Value {
        let manager = self.inner.manager.read();
        let host = self.inner.host.read();
        let config = self.inner.config.read();
        let out = &manager.output;
        let snap = out.meters.snapshot();
        json!({
            "daemon": {
                "version": env!("CARGO_PKG_VERSION"),
                "uptime_s": self.inner.started.elapsed().as_secs(),
                "pid": std::process::id(),
                "mode": if host.mode.is_empty() { "offline" } else { &host.mode },
            },
            "pipewire": host.pipewire,
            "dsp": {
                "sample_rate": snap.sample_rate,
                "quantum": snap.quantum,
                "latency_frames": snap.latency_frames,
                "xruns": snap.xruns,
                "cpu_percent": cpu_percent(&snap, manager.sample_rate),
                "dsp_p99_us": snap.dsp_p99_us,
            },
            "compat": {
                "baseline": "EasyEffects 8.3.2",
                "lsp_required": false,
                "effects": EffectKind::ALL.len(),
            },
            "config": {
                "bind": config.bind,
                "port": config.port,
                "theme": config.theme,
            },
            "revision": out.revision,
        })
    }

    pub fn metrics_json(&self) -> Value {
        let manager = self.inner.manager.read();
        let snap = manager.output.meters.snapshot();
        json!({
            "cpu_percent": cpu_percent(&snap, manager.sample_rate),
            "dsp_p50_us": snap.dsp_p50_us,
            "dsp_p99_us": snap.dsp_p99_us,
            "xruns": snap.xruns,
            "latency_frames": snap.latency_frames,
            "sample_rate": snap.sample_rate,
            "quantum": snap.quantum,
            "uptime_s": self.inner.started.elapsed().as_secs(),
            "frames": snap.frames,
        })
    }

    pub fn catalog_json(&self) -> Value {
        let groups: Vec<Value> = [
            dsp_effects::EffectGroup::Eq,
            dsp_effects::EffectGroup::Dynamics,
            dsp_effects::EffectGroup::Space,
            dsp_effects::EffectGroup::Tone,
            dsp_effects::EffectGroup::Voice,
            dsp_effects::EffectGroup::Noise,
            dsp_effects::EffectGroup::Meter,
        ]
        .into_iter()
        .map(|g| json!({ "id": g.as_str(), "label": g.label() }))
        .collect();
        let effects: Vec<&dsp_effects::EffectDescriptor> = dsp_effects::all_descriptors().collect();
        json!({ "groups": groups, "effects": effects })
    }

    pub fn apply_preset(
        &self,
        id: &str,
        kind: PipelineKind,
        revision: u64,
        allow_missing: bool,
    ) -> ApiResult<Value> {
        let preset = {
            let presets = self.inner.presets.read();
            presets
                .presets
                .get(id)
                .cloned()
                .ok_or_else(|| ApiError::not_found(format!("preset \"{id}\" does not exist")))?
        };

        let missing = preset_compat::resolve_assets(&preset, &self.inner.asset_dirs);
        if !missing.is_empty() && !allow_missing {
            return Err(ApiError::missing_asset(
                "The preset references assets that are not installed.",
                json!({ "missing_assets": missing }),
            ));
        }

        let mut manager = self.inner.manager.write();
        let (sample_rate, max_frames, channels) =
            (manager.sample_rate, manager.max_frames, manager.channels);
        let state = manager.state_mut(kind);
        if state.revision != revision {
            return Err(ApiError::conflict(state.revision));
        }

        let mut warnings: Vec<Warning> = preset.warnings.clone();
        if preset.kind != kind {
            warnings.push(Warning {
                code: "pipeline_mismatch".to_string(),
                message: format!(
                    "this preset was written for the {} pipeline but was applied to {}",
                    preset.kind, kind
                ),
                instance: None,
            });
        }
        let mut slots = Vec::new();
        for instance_id in &preset.plugins_order {
            let Some(instance) = preset.instances.get(instance_id) else {
                continue;
            };
            let Some(effect_kind) = instance.kind else {
                warnings.push(Warning {
                    code: "unknown_plugin".to_string(),
                    message: format!("{instance_id} is not a known effect and was not loaded"),
                    instance: Some(instance_id.clone()),
                });
                continue;
            };
            let desc = catalog::descriptor(effect_kind);
            let params = Arc::new(ParamStore::new(desc.params, desc.bands));
            let apply_warnings = preset_compat::apply_instance(instance, &params);
            warnings.extend(apply_warnings);
            let mut effect =
                dsp_effects::build(effect_kind, sample_rate, max_frames, channels, &params)
                    .map_err(|e| {
                        ApiError::pipeline_error(format!(
                            "{instance_id} could not be prepared: {e}"
                        ))
                    })?;
            let mut assets = Vec::new();
            for asset in desc.assets {
                let Some(name) = instance.string(asset.key) else {
                    continue;
                };
                let Some(path) = preset_compat::resolve_asset_path(
                    &self.inner.asset_dirs,
                    asset.kind,
                    name,
                    asset.extensions,
                ) else {
                    continue;
                };
                effect.load_asset(asset.key, &path).map_err(|e| {
                    ApiError::pipeline_error(format!(
                        "{instance_id} could not load {}: {e}",
                        path.display()
                    ))
                })?;
                assets.push((asset.key.to_string(), path));
            }
            let meters = Arc::new(SlotMeters::default());
            meters
                .latency_frames
                .store(effect.latency_frames() as u32, Ordering::Relaxed);
            let mut extra = instance.values.clone();
            for key in ["bypass", "input-gain", "output-gain", "dry", "wet"] {
                extra.remove(key);
            }
            for spec in desc.params {
                extra.remove(spec.key);
            }
            slots.push(SlotHandle {
                id: instance_id.clone(),
                kind: effect_kind,
                params,
                meters,
                extra,
                assets,
            });
        }

        let previous_slots = std::mem::replace(&mut state.slots, slots);
        let previous_blocklist = std::mem::replace(&mut state.blocklist, preset.blocklist.clone());
        if let Err(err) = state.install(sample_rate, max_frames, channels) {
            state.slots = previous_slots;
            state.blocklist = previous_blocklist;
            return Err(err);
        }
        state.revision += 1;
        let revision = state.revision;
        let pipeline = state.pipeline_json();
        drop(manager);

        self.broadcast(ServerEvent::Graph {
            pipeline: kind,
            revision,
        });
        Ok(json!({
            "pipeline": pipeline,
            "warnings": warnings,
            "missing_assets": missing,
        }))
    }

    pub fn self_test(&self) -> Value {
        let host = self.inner.host.read();
        let manager = self.inner.manager.read();
        let presets = self.inner.presets.read();
        let mut checks = Vec::new();
        checks.push(json!({
            "name": "PipeWire connection",
            "ok": host.pipewire.available,
            "detail": if host.pipewire.available {
                format!("{} {}", host.pipewire.server, host.pipewire.version)
            } else {
                "PipeWire is not reachable; running in offline mode".to_string()
            },
        }));
        checks.push(json!({
            "name": "DSP graph",
            "ok": true,
            "detail": format!(
                "{} output effects, {} input effects",
                manager.output.slots.len(),
                manager.input.slots.len()
            ),
        }));
        checks.push(json!({
            "name": "Preset directory",
            "ok": presets.dir.exists() || std::fs::create_dir_all(&presets.dir).is_ok(),
            "detail": presets.dir.display().to_string(),
        }));
        checks.push(json!({
            "name": "LSP Plugins",
            "ok": true,
            "detail": "not required and not loaded",
        }));
        checks.push(json!({
            "name": "Asset store",
            "ok": true,
            "detail": format!(
                "{} IR directories, {} model directories",
                self.inner.asset_dirs.irs.len(),
                self.inner.asset_dirs.models.len()
            ),
        }));
        json!({ "checks": checks })
    }
}

/// Turn a preset file stem into a readable name: `bass-boost` → `Bass Boost`.
fn display_name(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| match part {
            "ir" => "IR".to_string(),
            "eq" => "EQ".to_string(),
            "nr" => "NR".to_string(),
            _ => {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn cpu_percent(snap: &dsp_core::MeterSnapshot, sample_rate: u32) -> f32 {
    if sample_rate == 0 || snap.quantum == 0 {
        return 0.0;
    }
    let period_us = snap.quantum as f32 / sample_rate as f32 * 1e6;
    if period_us <= 0.0 {
        0.0
    } else {
        (snap.dsp_p50_us / period_us * 100.0).clamp(0.0, 100.0)
    }
}

fn generate_token() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 24];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut bytes);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("config.json")
}

fn load_config(data_dir: &Path) -> Config {
    std::fs::read_to_string(config_path(data_dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_config(data_dir: &Path, config: &Config) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let text = serde_json::to_string_pretty(config)?;
    std::fs::write(config_path(data_dir), text)
}

fn autoload_path(data_dir: &Path) -> PathBuf {
    data_dir.join("autoload.json")
}

fn load_autoload(data_dir: &Path) -> Vec<AutoloadRule> {
    std::fs::read_to_string(autoload_path(data_dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_autoload(data_dir: &Path, rules: &[AutoloadRule]) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let text = serde_json::to_string_pretty(rules)?;
    std::fs::write(autoload_path(data_dir), text)
}

/// Apply a JSON parameter map to a store, returning warnings.
pub fn apply_params(store: &ParamStore, params: &Map<String, Value>) -> Vec<String> {
    let mut warnings = Vec::new();
    for (key, value) in params {
        let Some((id, spec)) = store.spec_by_key(key) else {
            warnings.push(format!("unknown parameter \"{key}\""));
            continue;
        };
        match preset_compat::json_to_param(&spec.kind, value) {
            Some(v) => {
                store.set(id, v);
            }
            None => warnings.push(format!("\"{key}\" has an invalid value")),
        }
    }
    warnings
}

/// Apply a `{ channel: [band, ...] }` map to a store.
pub fn apply_bands(store: &ParamStore, bands: &Map<String, Value>) -> Vec<String> {
    let mut warnings = Vec::new();
    let Some(layout) = store.bands() else {
        return vec!["this effect has no bands".to_string()];
    };
    for (channel, value) in bands {
        let Some(ch) = layout.channel_labels.iter().position(|l| l == channel) else {
            warnings.push(format!("unknown band channel \"{channel}\""));
            continue;
        };
        let Some(items) = value.as_array() else {
            warnings.push(format!("\"{channel}\" should be an array of bands"));
            continue;
        };
        for (band, item) in items.iter().enumerate() {
            if band >= layout.max_bands {
                break;
            }
            let Some(obj) = item.as_object() else {
                continue;
            };
            for (field, field_value) in obj {
                let Some(fi) = layout.field_index(field) else {
                    continue;
                };
                let Some(spec) = layout.fields.get(fi) else {
                    continue;
                };
                let parsed = if field == "type" {
                    field_value
                        .as_u64()
                        .map(|v| ParamValue::Enum(v as u16))
                        .or_else(|| {
                            field_value
                                .as_str()
                                .and_then(band_type_index)
                                .map(ParamValue::Enum)
                        })
                } else {
                    preset_compat::json_to_param(&spec.kind, field_value)
                };
                match parsed {
                    Some(v) => {
                        store.set_band(ch, band, fi, v);
                    }
                    None => warnings.push(format!("\"{channel}[{band}].{field}\" is invalid")),
                }
            }
        }
    }
    warnings
}

fn band_type_index(s: &str) -> Option<u16> {
    const LABELS: [&str; 8] = [
        "Bell",
        "Lo-shelf",
        "Hi-shelf",
        "Lo-pass",
        "Hi-pass",
        "Notch",
        "Band-pass",
        "All-pass",
    ];
    let norm = s.to_ascii_lowercase().replace([' ', '_'], "-");
    LABELS
        .iter()
        .position(|l| l.to_ascii_lowercase().replace([' ', '_'], "-") == norm)
        .map(|i| i as u16)
}

/// Parse a pipeline kind path segment.
pub fn parse_kind(kind: &str) -> ApiResult<PipelineKind> {
    PipelineKind::parse(kind)
        .ok_or_else(|| ApiError::bad_request(format!("unknown pipeline \"{kind}\"")))
}

/// Find a slot index by instance id.
pub fn slot_index(state: &PipelineState, id: &str) -> ApiResult<usize> {
    state
        .slots
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| ApiError::not_found(format!("effect \"{id}\" is not in this pipeline")))
}

/// Build a `HashMap` of query parameters (used by the WS handler).
pub type QueryMap = HashMap<String, String>;
