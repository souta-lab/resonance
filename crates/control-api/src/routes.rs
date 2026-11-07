//! HTTP and WebSocket routes.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use dsp_core::param::{ParamStore, P_BYPASS};
use dsp_core::{EffectKind, PipelineKind};
use preset_compat::{ExportOptions, Preset};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tokio::sync::broadcast;

use crate::error::{ok, ApiError, ApiResult};
use crate::state::{
    apply_bands, apply_params, parse_kind, save_autoload, save_config, slot_index, AppState,
    AutoloadRule, QueryMap, ServerEvent, SlotHandle,
};

pub fn api_router(state: AppState) -> Router {
    let reads = Router::new()
        .route("/status", get(status))
        .route("/session", get(session))
        .route("/pipelines", get(pipelines))
        .route("/pipelines/{kind}/effects", get(pipeline_effects))
        .route("/effects/catalog", get(catalog_route))
        .route("/devices", get(devices))
        .route("/streams", get(streams))
        .route("/presets", get(presets_list))
        .route("/presets/{id}", get(preset_get))
        .route("/presets/{id}/export", get(preset_export))
        .route("/metrics", get(metrics))
        .route("/config", get(config_get))
        .route("/autoload", get(autoload_get))
        .route("/events", get(events));

    let mutations = Router::new()
        .route("/pipelines/{kind}/effects", post(add_effect))
        .route("/pipelines/{kind}/effects/{id}", patch(patch_effect))
        .route("/pipelines/{kind}/effects/{id}", delete(delete_effect))
        .route(
            "/pipelines/{kind}/effects/{id}/duplicate",
            post(duplicate_effect),
        )
        .route("/pipelines/{kind}/order", put(reorder))
        .route("/pipelines/{kind}/bypass", put(set_bypass))
        .route("/presets", post(preset_create))
        .route("/presets/{id}", delete(preset_delete))
        .route("/presets/import", post(preset_import))
        .route("/presets/{id}/apply", post(preset_apply))
        .route("/config", patch(config_patch))
        .route("/autoload", put(autoload_put))
        .route("/diagnostics/self-test", post(self_test))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));

    reads.merge(mutations).with_state(state)
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        // Non-browser clients (curl, the CLI) do not send Origin.
        return true;
    };
    let Some(rest) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or(rest);
    let host_no_port = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    if matches!(host_no_port, "localhost" | "127.0.0.1" | "[::1]" | "::1") {
        return true;
    }
    if let Some(request_host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
        if host == request_host {
            return true;
        }
    }
    false
}

async fn require_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if !origin_allowed(&headers) {
        return Err(ApiError::forbidden_origin());
    }
    let token = headers
        .get("x-resonance-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if token != state.inner.token {
        return Err(ApiError::unauthorized());
    }
    Ok(next.run(request).await)
}

// ---------------------------------------------------------------------------
// Read handlers
// ---------------------------------------------------------------------------

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    ok(state.status_json())
}

async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    if !origin_allowed(&headers) {
        return Err(ApiError::forbidden_origin());
    }
    Ok(ok(json!({
        "token": state.inner.token,
        "expires_in": 86_400,
    })))
}

async fn pipelines(State(state): State<AppState>) -> impl IntoResponse {
    let manager = state.inner.manager.read();
    ok(json!([
        manager.output.pipeline_json(),
        manager.input.pipeline_json()
    ]))
}

async fn pipeline_effects(
    State(state): State<AppState>,
    Path(kind): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let kind = parse_kind(&kind)?;
    let manager = state.inner.manager.read();
    Ok(ok(manager.state(kind).pipeline_json()))
}

async fn catalog_route(State(state): State<AppState>) -> impl IntoResponse {
    ok(state.catalog_json())
}

async fn devices(State(state): State<AppState>) -> impl IntoResponse {
    let host = state.inner.host.read();
    ok(host.devices.clone())
}

async fn streams(State(state): State<AppState>) -> impl IntoResponse {
    let host = state.inner.host.read();
    ok(host.streams.clone())
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    ok(state.metrics_json())
}

async fn config_get(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.inner.config.read();
    ok(config.clone())
}

async fn autoload_get(State(state): State<AppState>) -> impl IntoResponse {
    let rules = state.inner.autoload.read();
    ok(rules.clone())
}

// ---------------------------------------------------------------------------
// Effect mutations
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AddEffectBody {
    kind: String,
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    params: Option<Map<String, Value>>,
    #[serde(default)]
    bands: Option<Map<String, Value>>,
}

async fn add_effect(
    State(state): State<AppState>,
    Path(kind): Path<String>,
    Json(body): Json<AddEffectBody>,
) -> ApiResult<impl IntoResponse> {
    let pipeline_kind = parse_kind(&kind)?;
    let effect_kind = EffectKind::parse(&body.kind)
        .ok_or_else(|| ApiError::bad_request(format!("unknown effect \"{}\"", body.kind)))?;
    if effect_kind.input_only() && pipeline_kind == PipelineKind::Output {
        return Err(ApiError::bad_request(format!(
            "{} only applies to the input pipeline",
            effect_kind.as_str()
        )));
    }

    let (sample_rate, max_frames, channels) = {
        let manager = state.inner.manager.read();
        (manager.sample_rate, manager.max_frames, manager.channels)
    };

    let mut manager = state.inner.manager.write();
    let pipeline = manager.state_mut(pipeline_kind);
    let id = pipeline.next_instance_id(effect_kind);
    let slot = SlotHandle::new(effect_kind, id.clone());
    let mut warnings = Vec::new();
    if let Some(params) = &body.params {
        warnings.extend(apply_params(&slot.params, params));
    }
    if let Some(bands) = &body.bands {
        warnings.extend(apply_bands(&slot.params, bands));
    }
    // Validate before mutating the live graph.
    dsp_effects::build(effect_kind, sample_rate, max_frames, channels, &slot.params)
        .map_err(|e| ApiError::pipeline_error(format!("{id} could not be prepared: {e}")))?;

    let index = body
        .index
        .unwrap_or(pipeline.slots.len())
        .min(pipeline.slots.len());
    pipeline.slots.insert(index, slot);
    pipeline.install(sample_rate, max_frames, channels)?;
    pipeline.revision += 1;
    let revision = pipeline.revision;
    let effect = pipeline.effect_json(index).unwrap_or(Value::Null);
    drop(manager);

    state.broadcast(ServerEvent::Graph {
        pipeline: pipeline_kind,
        revision,
    });
    Ok(ok(
        json!({ "effect": effect, "revision": revision, "warnings": warnings }),
    ))
}

#[derive(Deserialize)]
struct PatchEffectBody {
    revision: u64,
    #[serde(default)]
    params: Option<Map<String, Value>>,
    #[serde(default)]
    bands: Option<Map<String, Value>>,
    #[serde(default)]
    bypass: Option<bool>,
}

async fn patch_effect(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Json(body): Json<PatchEffectBody>,
) -> ApiResult<impl IntoResponse> {
    let pipeline_kind = parse_kind(&kind)?;
    let mut manager = state.inner.manager.write();
    let pipeline = manager.state_mut(pipeline_kind);
    if pipeline.revision != body.revision {
        return Err(ApiError::conflict(pipeline.revision));
    }
    let index = slot_index(pipeline, &id)?;
    let slot = &pipeline.slots[index];
    let mut warnings = Vec::new();
    if let Some(params) = &body.params {
        warnings.extend(apply_params(&slot.params, params));
    }
    if let Some(bands) = &body.bands {
        warnings.extend(apply_bands(&slot.params, bands));
    }
    if let Some(bypass) = body.bypass {
        slot.params
            .set(P_BYPASS, dsp_core::ParamValue::Bool(bypass));
    }
    pipeline.revision += 1;
    let revision = pipeline.revision;
    let effect = pipeline.effect_json(index).unwrap_or(Value::Null);
    drop(manager);

    state.broadcast(ServerEvent::Graph {
        pipeline: pipeline_kind,
        revision,
    });
    Ok(ok(
        json!({ "effect": effect, "revision": revision, "warnings": warnings }),
    ))
}

#[derive(Deserialize)]
struct RevisionQuery {
    #[serde(default)]
    revision: Option<u64>,
}

async fn delete_effect(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Query(query): Query<RevisionQuery>,
) -> ApiResult<impl IntoResponse> {
    let pipeline_kind = parse_kind(&kind)?;
    let (sample_rate, max_frames, channels) = {
        let manager = state.inner.manager.read();
        (manager.sample_rate, manager.max_frames, manager.channels)
    };
    let mut manager = state.inner.manager.write();
    let pipeline = manager.state_mut(pipeline_kind);
    if let Some(revision) = query.revision {
        if pipeline.revision != revision {
            return Err(ApiError::conflict(pipeline.revision));
        }
    }
    let index = slot_index(pipeline, &id)?;
    pipeline.slots.remove(index);
    pipeline.install(sample_rate, max_frames, channels)?;
    pipeline.revision += 1;
    let revision = pipeline.revision;
    drop(manager);
    state.broadcast(ServerEvent::Graph {
        pipeline: pipeline_kind,
        revision,
    });
    Ok(ok(json!({ "revision": revision })))
}

#[derive(Deserialize)]
struct RevisionBody {
    revision: u64,
}

async fn duplicate_effect(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Json(body): Json<RevisionBody>,
) -> ApiResult<impl IntoResponse> {
    let pipeline_kind = parse_kind(&kind)?;
    let (sample_rate, max_frames, channels) = {
        let manager = state.inner.manager.read();
        (manager.sample_rate, manager.max_frames, manager.channels)
    };
    let mut manager = state.inner.manager.write();
    let pipeline = manager.state_mut(pipeline_kind);
    if pipeline.revision != body.revision {
        return Err(ApiError::conflict(pipeline.revision));
    }
    let index = slot_index(pipeline, &id)?;
    let source = &pipeline.slots[index];
    let new_id = pipeline.next_instance_id(source.kind);
    let mut slot = SlotHandle::new(source.kind, new_id.clone());
    copy_store(&source.params, &slot.params);
    slot.extra = source.extra.clone();
    dsp_effects::build(source.kind, sample_rate, max_frames, channels, &slot.params)
        .map_err(|e| ApiError::pipeline_error(format!("{new_id} could not be prepared: {e}")))?;
    pipeline.slots.insert(index + 1, slot);
    pipeline.install(sample_rate, max_frames, channels)?;
    pipeline.revision += 1;
    let revision = pipeline.revision;
    let effect = pipeline.effect_json(index + 1).unwrap_or(Value::Null);
    drop(manager);
    state.broadcast(ServerEvent::Graph {
        pipeline: pipeline_kind,
        revision,
    });
    Ok(ok(json!({ "effect": effect, "revision": revision })))
}

fn copy_store(from: &ParamStore, to: &ParamStore) {
    for i in 0..from.len().min(to.len()) {
        to.set(i as u16, from.get(i as u16));
    }
}

#[derive(Deserialize)]
struct OrderBody {
    revision: u64,
    order: Vec<String>,
}

async fn reorder(
    State(state): State<AppState>,
    Path(kind): Path<String>,
    Json(body): Json<OrderBody>,
) -> ApiResult<impl IntoResponse> {
    let pipeline_kind = parse_kind(&kind)?;
    let (sample_rate, max_frames, channels) = {
        let manager = state.inner.manager.read();
        (manager.sample_rate, manager.max_frames, manager.channels)
    };
    let mut manager = state.inner.manager.write();
    let pipeline = manager.state_mut(pipeline_kind);
    if pipeline.revision != body.revision {
        return Err(ApiError::conflict(pipeline.revision));
    }
    if body.order.len() != pipeline.slots.len() {
        return Err(ApiError::bad_request(
            "the new order must contain every effect exactly once",
        ));
    }
    let mut reordered = Vec::with_capacity(pipeline.slots.len());
    for id in &body.order {
        let index = slot_index(pipeline, id)?;
        reordered.push(pipeline.slots[index].id.clone());
    }
    let mut slots = std::mem::take(&mut pipeline.slots);
    let mut ordered = Vec::with_capacity(slots.len());
    for id in reordered {
        if let Some(pos) = slots.iter().position(|s| s.id == id) {
            ordered.push(slots.remove(pos));
        }
    }
    pipeline.slots = ordered;
    pipeline.install(sample_rate, max_frames, channels)?;
    pipeline.revision += 1;
    let revision = pipeline.revision;
    let json = pipeline.pipeline_json();
    drop(manager);
    state.broadcast(ServerEvent::Graph {
        pipeline: pipeline_kind,
        revision,
    });
    Ok(ok(json))
}

#[derive(Deserialize)]
struct BypassBody {
    revision: u64,
    bypass: bool,
}

async fn set_bypass(
    State(state): State<AppState>,
    Path(kind): Path<String>,
    Json(body): Json<BypassBody>,
) -> ApiResult<impl IntoResponse> {
    let pipeline_kind = parse_kind(&kind)?;
    let (sample_rate, max_frames, channels) = {
        let manager = state.inner.manager.read();
        (manager.sample_rate, manager.max_frames, manager.channels)
    };
    let mut manager = state.inner.manager.write();
    let pipeline = manager.state_mut(pipeline_kind);
    if pipeline.revision != body.revision {
        return Err(ApiError::conflict(pipeline.revision));
    }
    pipeline.bypass = body.bypass;
    pipeline.install(sample_rate, max_frames, channels)?;
    pipeline.revision += 1;
    let revision = pipeline.revision;
    let json = pipeline.pipeline_json();
    drop(manager);
    state.broadcast(ServerEvent::Graph {
        pipeline: pipeline_kind,
        revision,
    });
    Ok(ok(json))
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

async fn presets_list(State(state): State<AppState>) -> impl IntoResponse {
    let presets = state.inner.presets.read();
    ok(presets.list_json())
}

async fn preset_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let presets = state.inner.presets.read();
    presets
        .preset_json(&id)
        .map(ok)
        .ok_or_else(|| ApiError::not_found(format!("preset \"{id}\" does not exist")))
}

#[derive(Deserialize)]
struct CreatePresetBody {
    name: String,
    kind: PipelineKind,
    #[serde(default)]
    plugins_order: Option<Vec<String>>,
    #[serde(default)]
    instances: Option<Map<String, Value>>,
    #[serde(default)]
    blocklist: Option<Vec<String>>,
}

async fn preset_create(
    State(state): State<AppState>,
    Json(body): Json<CreatePresetBody>,
) -> ApiResult<impl IntoResponse> {
    let mut preset = preset_compat::empty_preset(body.kind);
    preset.blocklist = body.blocklist.unwrap_or_default();
    if let Some(order) = body.plugins_order {
        preset.plugins_order = order;
    }
    if let Some(instances) = body.instances {
        for (id, value) in instances {
            let (name, index) = preset_compat::split_id(&id);
            let kind = EffectKind::parse(&name);
            let values = value.as_object().cloned().unwrap_or_default();
            preset.instances.insert(
                id.clone(),
                preset_compat::Instance {
                    id,
                    kind,
                    index,
                    values,
                },
            );
        }
    }
    let mut presets = state.inner.presets.write();
    let id = presets.unique_id(&body.name);
    presets.save(&id, &preset)?;
    presets.presets.insert(id.clone(), preset);
    let json = presets.preset_json(&id).unwrap_or(Value::Null);
    Ok(ok(json))
}

async fn preset_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let mut presets = state.inner.presets.write();
    if !presets.presets.contains_key(&id) {
        return Err(ApiError::not_found(format!(
            "preset \"{id}\" does not exist"
        )));
    }
    presets.delete(&id)?;
    Ok(ok(json!({ "deleted": true })))
}

#[derive(Deserialize)]
struct ImportPresetBody {
    json: String,
    #[serde(default)]
    name: Option<String>,
}

async fn preset_import(
    State(state): State<AppState>,
    Json(body): Json<ImportPresetBody>,
) -> ApiResult<impl IntoResponse> {
    let mut preset =
        preset_compat::parse(&body.json).map_err(|e| ApiError::invalid_preset(e.to_string()))?;
    preset.missing_assets = preset_compat::resolve_assets(&preset, &state.inner.asset_dirs);
    let name = body
        .name
        .clone()
        .unwrap_or_else(|| format!("Imported {}", preset.kind));
    let mut presets = state.inner.presets.write();
    let id = presets.unique_id(&name);
    presets.save(&id, &preset)?;
    presets.presets.insert(id.clone(), preset.clone());
    let json = presets.preset_json(&id).unwrap_or(Value::Null);
    Ok(ok(json!({
        "preset": json,
        "warnings": preset.warnings,
        "missing_assets": preset.missing_assets,
    })))
}

#[derive(Deserialize)]
struct ApplyPresetBody {
    pipeline: PipelineKind,
    revision: u64,
    #[serde(default)]
    allow_missing: bool,
}

async fn preset_apply(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ApplyPresetBody>,
) -> ApiResult<impl IntoResponse> {
    let result = state.apply_preset(&id, body.pipeline, body.revision, body.allow_missing)?;
    Ok(ok(result))
}

async fn preset_export(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<QueryMap>,
) -> ApiResult<Response> {
    let presets = state.inner.presets.read();
    let preset = presets
        .presets
        .get(&id)
        .ok_or_else(|| ApiError::not_found(format!("preset \"{id}\" does not exist")))?;
    let legacy = query
        .get("legacy")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);
    let text = preset_compat::to_json(
        preset,
        ExportOptions {
            legacy,
            sort_keys: true,
        },
    );
    let mut response = (StatusCode::OK, text).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "application/json; charset=utf-8".parse().unwrap(),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{id}.json\"")
            .parse()
            .unwrap(),
    );
    Ok(response)
}

// ---------------------------------------------------------------------------
// Config, autoload, diagnostics
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ConfigPatch {
    #[serde(default)]
    bind: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    meter_rate_hz: Option<u32>,
    #[serde(default)]
    log_level: Option<String>,
    #[serde(default)]
    autoload: Option<bool>,
    #[serde(default)]
    virtual_nodes: Option<bool>,
}

async fn config_patch(
    State(state): State<AppState>,
    Json(body): Json<ConfigPatch>,
) -> ApiResult<impl IntoResponse> {
    let mut config = state.inner.config.write();
    if let Some(v) = body.bind {
        config.bind = v;
    }
    if let Some(v) = body.port {
        config.port = v;
    }
    if let Some(v) = body.theme {
        config.theme = v;
    }
    if let Some(v) = body.meter_rate_hz {
        config.meter_rate_hz = v.clamp(1, 120);
    }
    if let Some(v) = body.log_level {
        config.log_level = v;
    }
    if let Some(v) = body.autoload {
        config.autoload = v;
    }
    if let Some(v) = body.virtual_nodes {
        config.virtual_nodes = v;
    }
    let snapshot = config.clone();
    drop(config);
    save_config(&state.inner.data_dir, &snapshot)?;
    Ok(ok(snapshot))
}

#[derive(Deserialize)]
struct AutoloadBody {
    rules: Vec<AutoloadRule>,
}

async fn autoload_put(
    State(state): State<AppState>,
    Json(body): Json<AutoloadBody>,
) -> ApiResult<impl IntoResponse> {
    save_autoload(&state.inner.data_dir, &body.rules)?;
    let mut rules = state.inner.autoload.write();
    *rules = body.rules;
    Ok(ok(rules.clone()))
}

async fn self_test(State(state): State<AppState>) -> impl IntoResponse {
    ok(state.self_test())
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

async fn events(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<QueryMap>,
) -> Response {
    let token = query.get("token").map(|s| s.as_str()).unwrap_or_default();
    if token != state.inner.token {
        return ApiError::unauthorized().into_response();
    }
    ws.on_upgrade(move |socket| ws_loop(socket, state))
}

async fn ws_loop(mut socket: WebSocket, state: AppState) {
    let mut rx = state.inner.events.subscribe();
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        let Ok(text) = serde_json::to_string(&event) else { continue };
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = socket.send(Message::Pong(payload)).await;
                    }
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

/// Spawn the meter pump. It only produces events while a client is subscribed.
pub fn spawn_meter_pump(state: AppState) {
    tokio::spawn(async move {
        let mut tick = 0u64;
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(33));
        loop {
            interval.tick().await;
            if state.inner.events.receiver_count() == 0 {
                continue;
            }
            tick += 1;
            let timestamp_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let (output, input, sample_rate) = {
                let manager = state.inner.manager.read();
                (
                    manager.output.meters.clone(),
                    manager.input.meters.clone(),
                    manager.sample_rate,
                )
            };
            for (kind, meters) in [(PipelineKind::Output, output), (PipelineKind::Input, input)] {
                let snapshot = meters.snapshot();
                let _ = state.inner.events.send(ServerEvent::Meter {
                    pipeline: kind,
                    timestamp_ms,
                    peak_dbfs: snapshot.peak_dbfs,
                    true_peak_dbfs: snapshot.true_peak_dbfs,
                    rms_dbfs: snapshot.rms_dbfs,
                    lufs_m: snapshot.lufs_m,
                    lufs_s: snapshot.lufs_s,
                    gain_reduction_db: snapshot.gain_reduction_db,
                    dsp_p99_us: snapshot.dsp_p99_us,
                    xruns: snapshot.xruns,
                });
                if tick.is_multiple_of(3) {
                    let _ = state.inner.events.send(ServerEvent::Spectrum {
                        pipeline: kind,
                        bins: meters.spectrum(),
                        sample_rate,
                    });
                }
            }
        }
    });
}

/// Update the host snapshot and notify clients about device changes.
pub fn update_host(state: &AppState, snapshot: crate::state::HostSnapshot) {
    let previous = {
        let mut host = state.inner.host.write();
        let previous = host.devices.clone();
        *host = snapshot.clone();
        previous
    };
    for device in snapshot
        .devices
        .outputs
        .iter()
        .chain(snapshot.devices.inputs.iter())
    {
        let old = previous
            .outputs
            .iter()
            .chain(previous.inputs.iter())
            .find(|d| d.id == device.id);
        let action = match old {
            None => "added",
            Some(old)
                if old.state != device.state
                    || old.name != device.name
                    || old.is_default != device.is_default =>
            {
                "changed"
            }
            Some(_) => continue,
        };
        state.broadcast(ServerEvent::Device {
            action: action.to_string(),
            device: device.clone(),
        });
    }
}

/// Mark the engine as ready (used by the status endpoint and diagnostics).
pub fn mark_engine_ready(state: &AppState) {
    state.inner.engine_ready.store(true, Ordering::Relaxed);
}

/// Convenience for the daemon: build a preset from the live pipeline.
pub fn capture_preset(state: &AppState, kind: PipelineKind, name: &str) -> Preset {
    let manager = state.inner.manager.read();
    manager.state(kind).capture_preset(name)
}

/// Convenience for the daemon: install the current state of a pipeline.
pub fn install_pipeline(state: &AppState, kind: PipelineKind) -> ApiResult<()> {
    let manager = state.inner.manager.read();
    manager.install(kind)
}

/// Convenience for the daemon: the current revision of a pipeline.
pub fn revision_of(state: &AppState, kind: PipelineKind) -> u64 {
    let manager = state.inner.manager.read();
    manager.state(kind).revision
}

/// Convenience for the daemon: number of effects in a pipeline.
pub fn effect_count(state: &AppState, kind: PipelineKind) -> usize {
    let manager = state.inner.manager.read();
    manager.state(kind).slots.len()
}

/// Convenience for the daemon: the swap handle for a pipeline.
pub fn swap_of(state: &AppState, kind: PipelineKind) -> Arc<dsp_core::GraphSwap> {
    let manager = state.inner.manager.read();
    manager.state(kind).swap.clone()
}

/// Convenience for the daemon: the meter bank for a pipeline.
pub fn meters_of(state: &AppState, kind: PipelineKind) -> Arc<dsp_core::MeterBank> {
    let manager = state.inner.manager.read();
    manager.state(kind).meters.clone()
}
