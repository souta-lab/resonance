//! `resonance` command line interface.

#![allow(clippy::needless_range_loop)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use dsp_core::graph::{Pipeline, PipelineKind, Slot};
use dsp_core::meter::{MeterBank, SlotMeters};
use dsp_core::param::ParamStore;
use dsp_core::EffectKind;
use dsp_effects::catalog;
use serde_json::{json, Value};

#[derive(Parser)]
#[command(
    name = "resonance",
    version,
    about = "PipeWire audio effects rack with EasyEffects preset compatibility"
)]
struct Cli {
    /// Base URL of a running daemon.
    #[arg(long, global = true, default_value = "http://127.0.0.1:8737")]
    url: String,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the daemon (default).
    Daemon(DaemonArgs),
    /// Print daemon and DSP status.
    Status,
    /// List the effect catalog.
    Effects,
    /// Manage presets.
    Preset {
        #[command(subcommand)]
        cmd: PresetCmd,
    },
    /// Render a WAV file through a preset.
    Render(RenderArgs),
    /// Check the environment.
    Doctor,
    /// Run a DSP benchmark.
    Bench(BenchArgs),
}

#[derive(Args, Clone)]
struct DaemonArgs {
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long)]
    web_dir: Option<PathBuf>,
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
    #[arg(long, default_value_t = 8737)]
    port: u16,
    /// Do not touch PipeWire; run the DSP with silence.
    #[arg(long)]
    offline: bool,
    /// Generate a test tone instead of capturing audio.
    #[arg(long)]
    test_signal: bool,
    /// Do not create the virtual sink nodes.
    #[arg(long)]
    no_virtual_nodes: bool,
    #[arg(long, default_value_t = 48_000)]
    sample_rate: u32,
    #[arg(long, default_value_t = 128)]
    quantum: usize,
}

#[derive(Subcommand)]
enum PresetCmd {
    /// List presets known to the daemon.
    List,
    /// Show one preset as JSON.
    Show { id: String },
    /// Import an EasyEffects JSON file.
    Import {
        file: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Export a preset to a file.
    Export {
        id: String,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        legacy: bool,
    },
    /// Apply a preset to a pipeline.
    Apply {
        id: String,
        #[arg(long, default_value = "output")]
        pipeline: String,
    },
    /// Delete a preset.
    Delete { id: String },
}

#[derive(Args)]
struct RenderArgs {
    input: PathBuf,
    output: PathBuf,
    #[arg(long)]
    preset: PathBuf,
    #[arg(long, default_value = "output")]
    kind: String,
    #[arg(long, default_value_t = 512)]
    block: usize,
}

#[derive(Args)]
struct BenchArgs {
    #[arg(long, default_value_t = 10)]
    seconds: u32,
    #[arg(long, default_value_t = 48_000)]
    sample_rate: u32,
    #[arg(long, default_value_t = 128)]
    quantum: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Daemon(DaemonArgs {
        data_dir: None,
        web_dir: None,
        bind: "127.0.0.1".to_string(),
        port: 8737,
        offline: false,
        test_signal: false,
        no_virtual_nodes: false,
        sample_rate: 48_000,
        quantum: 128,
    })) {
        Command::Daemon(args) => run_daemon(args),
        Command::Status => status(&cli.url),
        Command::Effects => effects(),
        Command::Preset { cmd } => preset(&cli.url, cmd),
        Command::Render(args) => render(args),
        Command::Doctor => doctor(&cli.url),
        Command::Bench(args) => bench(args),
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

fn run_daemon(args: DaemonArgs) -> Result<()> {
    init_tracing();
    let options = daemon::Options {
        data_dir: args.data_dir,
        web_dir: args.web_dir.unwrap_or_else(daemon::default_web_dir),
        bind: args.bind,
        port: args.port,
        offline: args.offline,
        test_signal: args.test_signal,
        no_virtual_nodes: args.no_virtual_nodes,
        sample_rate: args.sample_rate,
        quantum: args.quantum,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(daemon::run(options))
}

// ---------------------------------------------------------------------------
// Minimal HTTP client (localhost only)
// ---------------------------------------------------------------------------

fn http(
    base: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
    token: Option<&str>,
) -> Result<(u16, String)> {
    let base = base.trim_end_matches('/');
    let rest = base
        .strip_prefix("http://")
        .ok_or_else(|| anyhow!("only http:// URLs are supported"))?;
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().context("invalid port")?),
        None => (rest, 80),
    };
    let mut stream =
        TcpStream::connect((host, port)).with_context(|| format!("connecting to {host}:{port}"))?;
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\nAccept: application/json\r\n"
    );
    if let Some(token) = token {
        request.push_str(&format!("X-Resonance-Token: {token}\r\n"));
    }
    if let Some(body) = body {
        request.push_str("Content-Type: application/json\r\n");
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }
    stream.write_all(request.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or((response.as_str(), ""));
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    Ok((status, body.to_string()))
}

fn token(base: &str) -> Result<String> {
    let (status, body) = http(base, "GET", "/api/v1/session", None, None)?;
    if status != 200 {
        bail!("could not obtain a session token (HTTP {status})");
    }
    let value: Value = serde_json::from_str(&body)?;
    value
        .get("data")
        .and_then(|d| d.get("token"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("session response did not contain a token"))
}

fn api_get(base: &str, path: &str) -> Result<Value> {
    let (status, body) = http(base, "GET", path, None, None)?;
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    if status != 200 {
        let message = value
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("request failed");
        bail!("{message} (HTTP {status})");
    }
    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}

fn api_post(base: &str, path: &str, body: &Value) -> Result<Value> {
    let token = token(base)?;
    let text = serde_json::to_string(body)?;
    let (status, response) = http(base, "POST", path, Some(&text), Some(&token))?;
    let value: Value = serde_json::from_str(&response).unwrap_or(Value::Null);
    if status != 200 {
        let message = value
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("request failed");
        bail!("{message} (HTTP {status})");
    }
    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn status(base: &str) -> Result<()> {
    let data = api_get(base, "/api/v1/status")?;
    let daemon = &data["daemon"];
    let pw = &data["pipewire"];
    let dsp = &data["dsp"];
    println!("Resonance {}", daemon["version"].as_str().unwrap_or("?"));
    println!(
        "  mode        {}",
        daemon["mode"].as_str().unwrap_or("unknown")
    );
    println!(
        "  pipewire    {} {}",
        pw["server"].as_str().unwrap_or("-"),
        pw["version"].as_str().unwrap_or("")
    );
    println!(
        "  dsp         {} Hz, quantum {}, latency {} frames",
        dsp["sample_rate"], dsp["quantum"], dsp["latency_frames"]
    );
    println!(
        "  load        {:.1}% cpu, p99 {:.0} µs, {} xruns",
        dsp["cpu_percent"].as_f64().unwrap_or(0.0),
        dsp["dsp_p99_us"].as_f64().unwrap_or(0.0),
        dsp["xruns"]
    );
    println!(
        "  compat      {} (LSP required: {})",
        data["compat"]["baseline"].as_str().unwrap_or("?"),
        data["compat"]["lsp_required"]
    );
    Ok(())
}

fn effects() -> Result<()> {
    let mut groups: Vec<&str> = Vec::new();
    for desc in dsp_effects::all_descriptors() {
        let group = desc.group.as_str();
        if !groups.contains(&group) {
            groups.push(group);
            println!("\n{}", desc.group.label());
        }
        println!(
            "  {:<22} {:<12} {}",
            desc.kind.as_str(),
            desc.compat.as_str(),
            desc.name
        );
    }
    println!("\n{} effects", EffectKind::ALL.len());
    Ok(())
}

fn preset(base: &str, cmd: PresetCmd) -> Result<()> {
    match cmd {
        PresetCmd::List => {
            let data = api_get(base, "/api/v1/presets")?;
            let items = data.as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                println!("No presets.");
                return Ok(());
            }
            for item in items {
                println!(
                    "{:<28} {:<8} {} effects",
                    item["id"].as_str().unwrap_or("?"),
                    item["kind"].as_str().unwrap_or("?"),
                    item["effects"]
                );
            }
        }
        PresetCmd::Show { id } => {
            let data = api_get(base, &format!("/api/v1/presets/{id}"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        PresetCmd::Import { file, name } => {
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("reading {}", file.display()))?;
            let mut body = json!({ "json": text });
            if let Some(name) = name {
                body["name"] = Value::String(name);
            }
            let data = api_post(base, "/api/v1/presets/import", &body)?;
            println!(
                "Imported {} ({} effects)",
                data["preset"]["id"].as_str().unwrap_or("?"),
                data["preset"]["plugins_order"]
                    .as_array()
                    .map(|a| a.len())
                    .unwrap_or(0)
            );
            if let Some(warnings) = data["warnings"].as_array() {
                for warning in warnings {
                    println!("  warning: {}", warning["message"].as_str().unwrap_or(""));
                }
            }
            if let Some(missing) = data["missing_assets"].as_array() {
                for asset in missing {
                    println!(
                        "  missing: {} ({})",
                        asset["name"].as_str().unwrap_or(""),
                        asset["kind"].as_str().unwrap_or("")
                    );
                }
            }
        }
        PresetCmd::Export { id, out, legacy } => {
            let path = format!(
                "/api/v1/presets/{id}/export{}",
                if legacy { "?legacy=1" } else { "" }
            );
            let (status, body) = http(base, "GET", &path, None, None)?;
            if status != 200 {
                bail!("export failed (HTTP {status})");
            }
            match out {
                Some(path) => {
                    std::fs::write(&path, body)?;
                    println!("Wrote {}", path.display());
                }
                None => println!("{body}"),
            }
        }
        PresetCmd::Apply { id, pipeline } => {
            let kind = PipelineKind::parse(&pipeline)
                .ok_or_else(|| anyhow!("pipeline must be input or output"))?;
            let current = api_get(base, &format!("/api/v1/pipelines/{pipeline}/effects"))?;
            let revision = current["revision"].as_u64().unwrap_or(0);
            let data = api_post(
                base,
                &format!("/api/v1/presets/{id}/apply"),
                &json!({ "pipeline": kind, "revision": revision, "allow_missing": true }),
            )?;
            println!(
                "Applied {id} to {pipeline} ({} effects)",
                data["pipeline"]["effects"]
                    .as_array()
                    .map(|a| a.len())
                    .unwrap_or(0)
            );
            if let Some(warnings) = data["warnings"].as_array() {
                for warning in warnings {
                    println!("  warning: {}", warning["message"].as_str().unwrap_or(""));
                }
            }
        }
        PresetCmd::Delete { id } => {
            let token = token(base)?;
            let (status, body) = http(
                base,
                "DELETE",
                &format!("/api/v1/presets/{id}"),
                None,
                Some(&token),
            )?;
            if status != 200 {
                bail!("delete failed: {body}");
            }
            println!("Deleted {id}");
        }
    }
    Ok(())
}

fn render(args: RenderArgs) -> Result<()> {
    let kind =
        PipelineKind::parse(&args.kind).ok_or_else(|| anyhow!("--kind must be input or output"))?;
    let text = std::fs::read_to_string(&args.preset)
        .with_context(|| format!("reading {}", args.preset.display()))?;
    let preset = preset_compat::parse(&text).context("parsing preset")?;

    let mut reader = hound::WavReader::open(&args.input)
        .with_context(|| format!("opening {}", args.input.display()))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let sample_rate = spec.sample_rate;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    let frames = samples.len() / channels.max(1);
    let mut input = vec![vec![0.0f32; frames]; channels];
    for (i, sample) in samples.iter().enumerate() {
        let frame = i / channels;
        let ch = i % channels;
        if frame < frames {
            input[ch][frame] = *sample;
        }
    }

    let block = args.block.max(16);
    let mut pipeline = Pipeline::new(kind, sample_rate, block, channels);
    for instance_id in &preset.plugins_order {
        let Some(instance) = preset.instances.get(instance_id) else {
            continue;
        };
        let Some(effect_kind) = instance.kind else {
            eprintln!("skipping unknown effect {instance_id}");
            continue;
        };
        let desc = catalog::descriptor(effect_kind);
        let params = Arc::new(ParamStore::new(desc.params, desc.bands));
        for warning in preset_compat::apply_instance(instance, &params) {
            eprintln!("warning: {}", warning.message);
        }
        let effect = dsp_effects::build(effect_kind, sample_rate, block, channels, &params)
            .with_context(|| format!("preparing {instance_id}"))?;
        pipeline.add_slot(Slot::new(
            instance_id.clone(),
            effect_kind,
            params,
            effect,
            Arc::new(SlotMeters::default()),
        ));
    }

    let meters = MeterBank::default();
    let mut output = vec![vec![0.0f32; frames]; channels];
    let mut pos = 0;
    let mut peak = 0.0f32;
    while pos < frames {
        let n = block.min(frames - pos);
        for ch in 0..channels {
            output[ch][pos..pos + n].copy_from_slice(&input[ch][pos..pos + n]);
        }
        let mut block_buf: Vec<Vec<f32>> =
            output.iter().map(|ch| ch[pos..pos + n].to_vec()).collect();
        pipeline.process(&mut block_buf, n, &meters);
        for ch in 0..channels {
            output[ch][pos..pos + n].copy_from_slice(&block_buf[ch]);
            for &v in &block_buf[ch] {
                peak = peak.max(v.abs());
            }
        }
        pos += n;
    }

    let mut writer = hound::WavWriter::create(
        &args.output,
        hound::WavSpec {
            channels: channels as u16,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    for frame in 0..frames {
        for ch in 0..channels {
            writer.write_sample(output[ch][frame])?;
        }
    }
    writer.finalize()?;

    println!(
        "Rendered {} frames ({:.2} s) at {} Hz, {} ch",
        frames,
        frames as f64 / sample_rate as f64,
        sample_rate,
        channels
    );
    println!(
        "  effects {} · latency {} frames · peak {:.2} dBFS",
        pipeline.slots.len(),
        pipeline.latency_frames(),
        20.0 * peak.max(1e-9).log10()
    );
    println!("  wrote {}", args.output.display());
    Ok(())
}

fn doctor(base: &str) -> Result<()> {
    println!("Resonance doctor\n");
    let mut ok = true;
    let mut check = |name: &str, passed: bool, detail: String| {
        println!(
            "  [{}] {:<24} {}",
            if passed { "ok" } else { "!!" },
            name,
            detail
        );
        ok &= passed;
    };

    check(
        "pw-dump",
        which("pw-dump").is_some(),
        which("pw-dump").unwrap_or_else(|| "not found".to_string()),
    );
    check(
        "pw-cat",
        which("pw-cat").is_some(),
        which("pw-cat").unwrap_or_else(|| "not found".to_string()),
    );
    check(
        "pw-cli",
        which("pw-cli").is_some(),
        which("pw-cli").unwrap_or_else(|| "not found".to_string()),
    );
    let lsp = which("lsp-plugins-lv2").is_some()
        || std::path::Path::new("/usr/lib/lv2/lsp-plugins.lv2").exists();
    check(
        "LSP Plugins",
        true,
        if lsp {
            "installed but not used".to_string()
        } else {
            "not installed (not required)".to_string()
        },
    );
    check(
        "effect catalog",
        EffectKind::ALL.len() == 33,
        format!("{} effects", EffectKind::ALL.len()),
    );
    let web = daemon::default_web_dir();
    check(
        "web build",
        web.join("index.html").is_file(),
        web.display().to_string(),
    );
    let data = daemon::default_data_dir();
    check(
        "data directory",
        data.exists() || std::fs::create_dir_all(&data).is_ok(),
        data.display().to_string(),
    );

    match api_get(base, "/api/v1/status") {
        Ok(status) => check(
            "daemon",
            true,
            format!(
                "{} {}",
                status["daemon"]["version"].as_str().unwrap_or("?"),
                status["daemon"]["mode"].as_str().unwrap_or("?")
            ),
        ),
        Err(err) => check("daemon", false, format!("not reachable: {err}")),
    }

    println!(
        "\n{}",
        if ok {
            "All checks passed."
        } else {
            "Some checks failed."
        }
    );
    Ok(())
}

fn which(binary: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
    }
    None
}

fn bench(args: BenchArgs) -> Result<()> {
    let sample_rate = args.sample_rate;
    let quantum = args.quantum;
    let channels = 2;
    let mut pipeline = Pipeline::new(PipelineKind::Output, sample_rate, quantum, channels);
    for kind in [
        EffectKind::Equalizer,
        EffectKind::Compressor,
        EffectKind::Limiter,
    ] {
        let desc = catalog::descriptor(kind);
        let params = Arc::new(ParamStore::new(desc.params, desc.bands));
        let effect = dsp_effects::build(kind, sample_rate, quantum, channels, &params)?;
        pipeline.add_slot(Slot::new(
            kind.as_str(),
            kind,
            params,
            effect,
            Arc::new(SlotMeters::default()),
        ));
    }
    let meters = MeterBank::default();
    let mut buf = vec![vec![0.0f32; quantum]; channels];
    let mut noise = 0x1234_5678u32;
    let blocks = (args.seconds as usize * sample_rate as usize) / quantum;
    let mut times = Vec::with_capacity(blocks);
    let start = Instant::now();
    for _ in 0..blocks {
        for ch in 0..channels {
            for sample in buf[ch].iter_mut() {
                noise ^= noise << 13;
                noise ^= noise >> 17;
                noise ^= noise << 5;
                *sample = ((noise as f32 / u32::MAX as f32) - 0.5) * 0.4;
            }
        }
        let t = Instant::now();
        pipeline.process(&mut buf, quantum, &meters);
        times.push(t.elapsed().as_secs_f64() * 1e6);
    }
    let elapsed = start.elapsed().as_secs_f64();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p = |q: f64| times[((times.len() as f64 - 1.0) * q) as usize];
    let period_us = quantum as f64 / sample_rate as f64 * 1e6;
    println!(
        "Chain: EQ + Compressor + Limiter · {} Hz · {} frames · {} s",
        sample_rate, quantum, args.seconds
    );
    println!(
        "  p50 {:.1} µs · p95 {:.1} µs · p99 {:.1} µs · max {:.1} µs",
        p(0.50),
        p(0.95),
        p(0.99),
        times.last().copied().unwrap_or(0.0)
    );
    println!(
        "  quantum budget {:.1} µs · p99 uses {:.1}% · realtime factor {:.1}x",
        period_us,
        p(0.99) / period_us * 100.0,
        (args.seconds as f64) / elapsed
    );
    Ok(())
}
