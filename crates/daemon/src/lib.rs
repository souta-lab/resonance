//! Resonance daemon: process lifecycle, PipeWire bridge, audio threads.

#![allow(clippy::needless_range_loop)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use control_api::{
    AppState, DeviceInfo, DevicesInfo, HostSnapshot, PipewireInfo, StreamInfo, VirtualNodes,
};
use dsp_core::graph::{Engine, Pipeline};
use dsp_core::PipelineKind;
use pw_backend::{Bridge, Snapshot};

#[derive(Clone, Debug)]
pub struct Options {
    pub data_dir: Option<PathBuf>,
    pub web_dir: PathBuf,
    pub bind: String,
    pub port: u16,
    pub offline: bool,
    pub test_signal: bool,
    pub no_virtual_nodes: bool,
    pub sample_rate: u32,
    pub quantum: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            data_dir: None,
            web_dir: default_web_dir(),
            bind: "127.0.0.1".to_string(),
            port: 8737,
            offline: false,
            test_signal: false,
            no_virtual_nodes: false,
            sample_rate: 48_000,
            quantum: 128,
        }
    }
}

pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("resonance");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/share/resonance");
    }
    PathBuf::from("./resonance-data")
}

pub fn default_web_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut candidates = vec![
        cwd.join("web/dist"),
        cwd.join("../web/dist"),
        cwd.join("../../web/dist"),
    ];
    if let Ok(data) = std::env::var("XDG_DATA_HOME") {
        if !data.is_empty() {
            candidates.push(PathBuf::from(data).join("resonance/web"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(home).join(".local/share/resonance/web"));
    }
    candidates.push(PathBuf::from("/usr/share/resonance/web"));
    for candidate in &candidates {
        if candidate.join("index.html").is_file() {
            return candidate.clone();
        }
    }
    cwd.join("web/dist")
}

fn convert(snapshot: Snapshot) -> HostSnapshot {
    let devices = DevicesInfo {
        outputs: snapshot
            .devices
            .iter()
            .filter(|d| d.media_class == "Audio/Sink")
            .map(|d| DeviceInfo {
                id: d.id,
                name: d.name.clone(),
                description: d.description.clone(),
                nick: d.nick.clone(),
                is_default: d.is_default,
                state: d.state.clone(),
                profile: d.profile.clone(),
            })
            .collect(),
        inputs: snapshot
            .devices
            .iter()
            .filter(|d| d.media_class == "Audio/Source")
            .map(|d| DeviceInfo {
                id: d.id,
                name: d.name.clone(),
                description: d.description.clone(),
                nick: d.nick.clone(),
                is_default: d.is_default,
                state: d.state.clone(),
                profile: d.profile.clone(),
            })
            .collect(),
    };
    let streams = snapshot
        .streams
        .iter()
        .map(|s| StreamInfo {
            id: s.id,
            app: s.app.clone(),
            name: s.name.clone(),
            media: s.media.clone(),
            direction: s.direction.clone(),
            linked: s.linked,
            blocked: s.blocked,
            target: s.target.clone(),
        })
        .collect();
    HostSnapshot {
        pipewire: PipewireInfo {
            available: snapshot.available,
            version: snapshot.version,
            server: snapshot.server,
            virtual_nodes: VirtualNodes {
                output: pw_backend::OUTPUT_NODE.to_string(),
                input: pw_backend::INPUT_NODE.to_string(),
            },
        },
        devices,
        streams,
        mode: if snapshot.available {
            "pipewire".to_string()
        } else {
            "offline".to_string()
        },
    }
}

pub async fn run(options: Options) -> anyhow::Result<()> {
    let data_dir = options.data_dir.clone().unwrap_or_else(default_data_dir);
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating data directory {}", data_dir.display()))?;

    let state = AppState::new(
        data_dir.clone(),
        options.web_dir.clone(),
        options.sample_rate,
        options.quantum,
        2,
    );

    let snapshot = pw_backend::probe().await;
    let available = snapshot.available;
    control_api::update_host(&state, convert(snapshot));
    if available {
        tracing::info!("PipeWire is available");
    } else {
        tracing::warn!("PipeWire is not available; running in offline mode");
    }

    if !options.offline && !options.no_virtual_nodes && available {
        match pw_backend::create_virtual_nodes() {
            Ok(()) => tracing::info!(
                output = pw_backend::OUTPUT_NODE,
                input = pw_backend::INPUT_NODE,
                "virtual nodes created"
            ),
            Err(err) => tracing::warn!(%err, "could not create virtual nodes"),
        }
    }

    let output_pipeline = {
        let manager = state.inner.manager.read();
        manager.build_initial(PipelineKind::Output)?
    };
    let input_pipeline = {
        let manager = state.inner.manager.read();
        manager.build_initial(PipelineKind::Input)?
    };

    let stop = Arc::new(AtomicBool::new(false));
    let output_thread = spawn_engine(
        state.clone(),
        PipelineKind::Output,
        output_pipeline,
        options.clone(),
        stop.clone(),
    )?;
    let input_thread = spawn_engine(
        state.clone(),
        PipelineKind::Input,
        input_pipeline,
        options.clone(),
        stop.clone(),
    )?;
    control_api::mark_engine_ready(&state);

    // Periodically refresh devices and streams.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            loop {
                interval.tick().await;
                let snapshot = pw_backend::probe().await;
                control_api::update_host(&state, convert(snapshot));
            }
        });
    }

    // Autoload rules: apply a preset when a matching device appears.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut applied: Vec<String> = Vec::new();
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            loop {
                interval.tick().await;
                let rules = state.inner.autoload.read().clone();
                if rules.is_empty() {
                    continue;
                }
                let devices = state.inner.host.read().devices.clone();
                for rule in rules.iter().filter(|r| r.enabled) {
                    let key = format!("{}:{}", rule.device, rule.preset);
                    if applied.contains(&key) {
                        continue;
                    }
                    let present = devices
                        .outputs
                        .iter()
                        .chain(devices.inputs.iter())
                        .any(|d| d.name == rule.device);
                    if !present {
                        continue;
                    }
                    let revision = control_api::revision_of(&state, rule.pipeline);
                    match state.apply_preset(&rule.preset, rule.pipeline, revision, true) {
                        Ok(_) => {
                            tracing::info!(preset = %rule.preset, device = %rule.device, "autoload applied");
                            applied.push(key);
                        }
                        Err(err) => {
                            tracing::warn!(preset = %rule.preset, ?err, "autoload failed");
                        }
                    }
                }
            }
        });
    }

    let addr = format!("{}:{}", options.bind, options.port)
        .parse()
        .context("invalid bind address")?;

    let shutdown = {
        let stop = stop.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
            stop.store(true, Ordering::Relaxed);
        }
    };

    let server = control_api::serve(state.clone(), addr);
    tokio::select! {
        result = server => {
            if let Err(err) = result {
                tracing::error!(%err, "server stopped");
            }
        }
        _ = shutdown => {}
    }

    stop.store(true, Ordering::Relaxed);
    if !options.no_virtual_nodes {
        pw_backend::destroy_virtual_nodes();
    }
    let _ = output_thread.join();
    let _ = input_thread.join();
    Ok(())
}

fn spawn_engine(
    state: AppState,
    kind: PipelineKind,
    pipeline: Box<Pipeline>,
    options: Options,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    let swap = control_api::swap_of(&state, kind);
    let meters = control_api::meters_of(&state, kind);
    let frames = options.quantum;
    let channels = 2;
    let sample_rate = options.sample_rate;
    let name = format!("resonance-{}", kind.as_str());

    std::thread::Builder::new()
        .name(name)
        .spawn(move || {
            let mut engine = Engine::new(*pipeline, swap, meters);
            engine.set_realtime(options.test_signal);
            let mut buf = vec![vec![0.0f32; frames]; channels];

            let use_bridge = !options.offline && !options.test_signal;
            if use_bridge {
                let (capture_target, capture_sink, playback_target) = match kind {
                    PipelineKind::Output => (Some(pw_backend::OUTPUT_NODE), true, None),
                    PipelineKind::Input => (None, false, Some(pw_backend::INPUT_NODE)),
                };
                match Bridge::spawn(
                    capture_target,
                    capture_sink,
                    playback_target,
                    sample_rate,
                    channels,
                    frames,
                ) {
                    Ok(mut bridge) => {
                        tracing::info!(pipeline = kind.as_str(), "audio bridge running");
                        while !stop.load(Ordering::Relaxed) {
                            if !bridge.read_block(&mut buf) {
                                tracing::warn!(pipeline = kind.as_str(), "capture ended");
                                break;
                            }
                            engine.process(&mut buf, frames);
                            if !bridge.write_block(&buf) {
                                tracing::warn!(pipeline = kind.as_str(), "playback ended");
                                break;
                            }
                        }
                        return;
                    }
                    Err(err) => {
                        tracing::warn!(pipeline = kind.as_str(), %err, "bridge unavailable; using test signal");
                    }
                }
            }

            let period = Duration::from_secs_f32(frames as f32 / sample_rate as f32);
            let mut phase = 0.0f32;
            let mut t = 0.0f32;
            while !stop.load(Ordering::Relaxed) {
                if options.test_signal {
                    for i in 0..frames {
                        let a = (phase).sin() * 0.18;
                        let b = (phase * 1.5).sin() * 0.08;
                        let tremolo = 0.75 + 0.25 * (t * 0.7).sin();
                        let sample = (a + b) * tremolo;
                        buf[0][i] = sample;
                        buf[1][i] = sample * 0.92;
                        phase += 2.0 * std::f32::consts::PI * 220.0 / sample_rate as f32;
                        if phase > std::f32::consts::TAU {
                            phase -= std::f32::consts::TAU;
                        }
                        t += 1.0 / sample_rate as f32;
                    }
                } else {
                    for ch in buf.iter_mut() {
                        ch.fill(0.0);
                    }
                }
                engine.process(&mut buf, frames);
                std::thread::sleep(period);
            }
        })
        .context("spawning audio thread")
}
