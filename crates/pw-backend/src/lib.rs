//! PipeWire integration.
//!
//! Discovery uses `pw-dump` (JSON) so that we do not need to link against
//! libpipewire. Virtual devices are created with `pw-cli create-node adapter`
//! and audio is bridged with `pw-cat` raw f32 streams. This keeps the daemon
//! buildable on systems without PipeWire development headers while still using
//! the real server at runtime.

use std::io::{Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const OUTPUT_NODE: &str = "resonance-output";
pub const INPUT_NODE: &str = "resonance-input";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Device {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub nick: String,
    pub is_default: bool,
    pub state: String,
    pub profile: String,
    pub media_class: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Stream {
    pub id: u32,
    pub app: String,
    pub name: String,
    pub media: String,
    pub direction: String,
    pub linked: bool,
    pub blocked: bool,
    pub target: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub available: bool,
    pub version: String,
    pub server: String,
    pub devices: Vec<Device>,
    pub streams: Vec<Stream>,
}

fn prop<'a>(props: &'a Value, key: &str) -> Option<&'a str> {
    props.get(key).and_then(|v| v.as_str())
}

fn node_state(info: &Value) -> String {
    info.get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Run `pw-dump` and parse the result.
pub async fn probe() -> Snapshot {
    let version = pipewire_version().await;
    let output = tokio::time::timeout(
        Duration::from_secs(4),
        tokio::process::Command::new("pw-dump").output(),
    )
    .await;

    let Ok(Ok(output)) = output else {
        return Snapshot {
            available: false,
            version,
            server: String::new(),
            devices: Vec::new(),
            streams: Vec::new(),
        };
    };
    if !output.status.success() {
        return Snapshot {
            available: false,
            version,
            server: String::new(),
            devices: Vec::new(),
            streams: Vec::new(),
        };
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Snapshot {
            available: false,
            version,
            server: String::new(),
            devices: Vec::new(),
            streams: Vec::new(),
        };
    };
    let mut snapshot = parse_dump(&value);
    snapshot.version = version;
    snapshot
}

async fn pipewire_version() -> String {
    let output = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::process::Command::new("pipewire")
            .arg("--version")
            .output(),
    )
    .await;
    if let Ok(Ok(output)) = output {
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = text.lines().find(|l| l.contains("libpipewire")) {
            if let Some(v) = line.split_whitespace().last() {
                return v.to_string();
            }
        }
    }
    String::new()
}

pub fn parse_dump(value: &Value) -> Snapshot {
    let mut devices = Vec::new();
    let mut streams = Vec::new();
    let mut default_sink = String::new();
    let mut default_source = String::new();
    let mut server = String::new();

    let Some(items) = value.as_array() else {
        return Snapshot::default();
    };

    for item in items {
        let ty = item
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let info = item.get("info").unwrap_or(&Value::Null);
        match ty {
            "PipeWire:Interface:Core" => {
                if let Some(props) = info.get("props") {
                    if let Some(socket) = prop(props, "pipewire.sec.socket") {
                        server = socket.to_string();
                    }
                }
            }
            "PipeWire:Interface:Metadata" => {
                let name = info
                    .get("metadata")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if name != "default" {
                    continue;
                }
                if let Some(entries) = info.get("entries").and_then(|v| v.as_array()) {
                    for entry in entries {
                        let key = entry
                            .get("key")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        let value = entry.get("value").cloned().unwrap_or(Value::Null);
                        let node_name = value
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        match key {
                            "default.audio.sink" => default_sink = node_name,
                            "default.audio.source" => default_source = node_name,
                            _ => {}
                        }
                    }
                }
            }
            "PipeWire:Interface:Node" => {
                let props = info.get("props").unwrap_or(&Value::Null);
                let media_class = prop(props, "media.class").unwrap_or_default();
                let node_name = prop(props, "node.name").unwrap_or_default();
                let description = prop(props, "node.description")
                    .or_else(|| prop(props, "node.nick"))
                    .unwrap_or(node_name);
                let nick = prop(props, "node.nick").unwrap_or(description);
                let id = item.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let state = node_state(info);

                if media_class == "Audio/Sink" || media_class == "Audio/Source" {
                    let is_default = if media_class == "Audio/Sink" {
                        !default_sink.is_empty() && node_name == default_sink
                    } else {
                        !default_source.is_empty() && node_name == default_source
                    };
                    devices.push(Device {
                        id,
                        name: node_name.to_string(),
                        description: description.to_string(),
                        nick: nick.to_string(),
                        is_default,
                        state,
                        profile: prop(props, "device.profile.name")
                            .unwrap_or_default()
                            .to_string(),
                        media_class: media_class.to_string(),
                    });
                } else if media_class.starts_with("Stream/") {
                    let direction = if media_class.contains("Output") {
                        "output"
                    } else {
                        "input"
                    };
                    let app = prop(props, "application.name")
                        .or_else(|| prop(props, "application.process.binary"))
                        .or_else(|| prop(props, "node.name"))
                        .unwrap_or("Unknown");
                    let name = prop(props, "media.name")
                        .or_else(|| prop(props, "node.description"))
                        .unwrap_or_default();
                    streams.push(Stream {
                        id,
                        app: app.to_string(),
                        name: name.to_string(),
                        media: prop(props, "media.type").unwrap_or("audio").to_string(),
                        direction: direction.to_string(),
                        linked: state == "running",
                        blocked: false,
                        target: prop(props, "node.target").unwrap_or_default().to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    if server.is_empty() {
        server = "pipewire-0".to_string();
    }

    Snapshot {
        available: true,
        version: String::new(),
        server,
        devices,
        streams,
    }
}

fn run_pw_cli(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("pw-cli")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
}

/// Create the virtual sink nodes. Existing nodes with the same name are
/// destroyed first so that a restart is idempotent.
pub fn create_virtual_nodes() -> std::io::Result<()> {
    let _ = run_pw_cli(&["destroy", OUTPUT_NODE]);
    let _ = run_pw_cli(&["destroy", INPUT_NODE]);
    for node in [OUTPUT_NODE, INPUT_NODE] {
        let spec = format!(
            "{{ factory.name=support.null-audio-sink node.name={node} node.description=\"Resonance {node}\" media.class=Audio/Sink object.linger=true audio.position=[FL,FR] }}"
        );
        let output = run_pw_cli(&["create-node", "adapter", &spec])?;
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "pw-cli create-node failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
    }
    Ok(())
}

pub fn destroy_virtual_nodes() {
    let _ = run_pw_cli(&["destroy", OUTPUT_NODE]);
    let _ = run_pw_cli(&["destroy", INPUT_NODE]);
}

/// A raw f32 audio bridge built on `pw-cat`.
pub struct Bridge {
    capture: Child,
    playback: Child,
    capture_out: ChildStdout,
    playback_in: ChildStdin,
    channels: usize,
    frames: usize,
    raw: Vec<u8>,
    interleaved: Vec<u8>,
}

impl Bridge {
    pub fn spawn(
        capture_target: Option<&str>,
        capture_sink: bool,
        playback_target: Option<&str>,
        rate: u32,
        channels: usize,
        frames: usize,
    ) -> std::io::Result<Self> {
        let mut capture_cmd = Command::new("pw-cat");
        capture_cmd
            .arg("--record")
            .arg("--raw")
            .arg("--format")
            .arg("f32")
            .arg("--rate")
            .arg(rate.to_string())
            .arg("--channels")
            .arg(channels.to_string())
            .arg("--latency")
            .arg("20ms")
            .arg("-");
        if let Some(target) = capture_target {
            capture_cmd.arg("--target").arg(target);
        }
        if capture_sink {
            // Capture the monitor of a sink rather than a source.
            capture_cmd.arg("-P").arg("stream.capture.sink=true");
        }
        let mut capture = capture_cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let mut playback_cmd = Command::new("pw-cat");
        playback_cmd
            .arg("--playback")
            .arg("--raw")
            .arg("--format")
            .arg("f32")
            .arg("--rate")
            .arg(rate.to_string())
            .arg("--channels")
            .arg(channels.to_string())
            .arg("--latency")
            .arg("20ms")
            .arg("-");
        if let Some(target) = playback_target {
            playback_cmd.arg("--target").arg(target);
        }
        let mut playback = playback_cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        let capture_out = capture
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("pw-cat capture has no stdout"))?;
        let playback_in = playback
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("pw-cat playback has no stdin"))?;

        Ok(Self {
            capture,
            playback,
            capture_out,
            playback_in,
            channels,
            frames,
            raw: vec![0u8; frames * channels * 4],
            interleaved: vec![0u8; frames * channels * 4],
        })
    }

    /// Read one block. Returns false on EOF/error.
    pub fn read_block(&mut self, buf: &mut [Vec<f32>]) -> bool {
        if self.capture_out.read_exact(&mut self.raw).is_err() {
            return false;
        }
        for frame in 0..self.frames {
            for ch in 0..self.channels.min(buf.len()) {
                let offset = (frame * self.channels + ch) * 4;
                let bytes = [
                    self.raw[offset],
                    self.raw[offset + 1],
                    self.raw[offset + 2],
                    self.raw[offset + 3],
                ];
                buf[ch][frame] = f32::from_le_bytes(bytes);
            }
        }
        true
    }

    pub fn write_block(&mut self, buf: &[Vec<f32>]) -> bool {
        for frame in 0..self.frames {
            for ch in 0..self.channels {
                let sample = buf
                    .get(ch)
                    .and_then(|c| c.get(frame))
                    .copied()
                    .unwrap_or(0.0);
                let offset = (frame * self.channels + ch) * 4;
                self.interleaved[offset..offset + 4].copy_from_slice(&sample.to_le_bytes());
            }
        }
        self.playback_in.write_all(&self.interleaved).is_ok()
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.capture.try_wait(), Ok(None)) && matches!(self.playback.try_wait(), Ok(None))
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.capture.kill();
        let _ = self.playback.kill();
        let _ = self.capture.wait();
        let _ = self.playback.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_devices_and_defaults() {
        let dump = json!([
            {
                "id": 0,
                "type": "PipeWire:Interface:Core",
                "info": { "props": { "pipewire.sec.socket": "pipewire-0" } }
            },
            {
                "id": 1,
                "type": "PipeWire:Interface:Metadata",
                "info": {
                    "metadata": "default",
                    "entries": [
                        { "key": "default.audio.sink", "value": { "name": "speakers" } }
                    ]
                }
            },
            {
                "id": 42,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "state": "running",
                    "props": {
                        "media.class": "Audio/Sink",
                        "node.name": "speakers",
                        "node.description": "Built-in Audio"
                    }
                }
            },
            {
                "id": 55,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "state": "running",
                    "props": {
                        "media.class": "Stream/Output/Audio",
                        "node.name": "firefox",
                        "application.name": "Firefox",
                        "media.name": "Playback"
                    }
                }
            }
        ]);
        let snapshot = parse_dump(&dump);
        assert!(snapshot.available);
        assert_eq!(snapshot.server, "pipewire-0");
        assert_eq!(snapshot.devices.len(), 1);
        assert!(snapshot.devices[0].is_default);
        assert_eq!(snapshot.streams.len(), 1);
        assert_eq!(snapshot.streams[0].app, "Firefox");
        assert_eq!(snapshot.streams[0].direction, "output");
    }
}
