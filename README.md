# Resonance

A PipeWire audio-effects rack written in Rust, with a browser control surface
and EasyEffects 8.3.2 preset compatibility. No LSP Plugins, no LV2 host, no
Qt — the DSP is native Rust and the UI is a static SPA served by the daemon.

> Status: working prototype (M1–M4 of the spec, partial M5). The DSP, preset
> round-trip, HTTP/WS API, CLI and WebUI are functional. The PipeWire path uses
> a `pw-cat` bridge rather than a native `pw_filter`; model-based effects use
> documented algorithmic substitutes. See [docs/roadmap.md](docs/roadmap.md)
> for the honest gap list.

![Output chain](docs/screenshots/output.png)

<details>
<summary>More screens</summary>

| | |
|---|---|
| ![Presets](docs/screenshots/presets.png) | ![Devices](docs/screenshots/devices.png) |
| ![Analyzer](docs/screenshots/analyzer.png) | ![Night scope](docs/screenshots/output-night.png) |

</details>

## Quick start

```bash
# 1. Build the daemon and CLI
cargo build --release

# 2. Build the control surface
cd web && npm install && npm run build && cd ..

# 3. Run
./target/release/resonance daemon
# open http://127.0.0.1:8737
```

The daemon creates two virtual sinks, `resonance-output` and
`resonance-input`, and bridges them to your default devices. Point an
application at `resonance-output` to process its playback; select
`resonance-input` (monitor) as a microphone to process a capture path.

Useful flags:

```bash
resonance daemon --test-signal        # no PipeWire, generate a tone
resonance daemon --offline            # silence, API only
resonance daemon --no-virtual-nodes   # do not create the sinks
resonance daemon --port 9000
```

## What works

* **33 effects** in the catalog, all constructible without LSP Plugins.
  Equalizer, Mid-Side EQ, Filter, Compressor, Limiter, Gate, Expander,
  Multiband Compressor/Gate, De-esser, Auto Gain, Delay, Crossfeed, Stereo
  Tools, Crusher, Level Meter and others are native implementations; the
  model-based effects are algorithmic substitutes (see the matrix).
* **EasyEffects preset round-trip**: modern and legacy instance ids, multiple
  instances, `blocklist`, unknown keys preserved, enum labels mapped, band
  objects mapped, sorted export with a trailing newline.
* **Real-time graph**: lock-free parameter updates, graph hand-over with
  `try_lock`, retired graphs dropped off the audio thread, click-free bypass
  and dry/wet ramps.
* **Control API**: `/api/v1` HTTP + WebSocket, session token, origin checks,
  revision-based conflict detection, meter/spectrum events only while
  subscribed.
* **WebUI**: rack, effect faceplates, EQ editor, presets, devices, analyzer,
  diagnostics. Keyboard reorder, responsive layout, light/dark themes.
* **CLI**: daemon, status, effects, preset import/export/apply, offline WAV
  render, doctor, benchmark.

## What is not there yet

* Native `pw_filter` backend (the `pw-cat` bridge adds ~20 ms of buffering).
* Neural noise suppression / DPDFNet / RNNoise models. `Deep Noise Remover`
  runs a spectral gate and says so in the catalog.
* Per-effect acoustic validation against EasyEffects. The compatibility matrix
  records the claimed level, not a measured one.
* Full parameter coverage for every upstream key. Unknown keys are preserved
  losslessly, but some are not yet mapped to DSP controls.
* Packaging beyond the systemd unit and install script in `packaging/`.

## CLI

```bash
resonance status
resonance effects
resonance preset list
resonance preset import fixtures/presets/bass-boost.json
resonance preset apply bass-boost --pipeline output
resonance preset export bass-boost --out /tmp/bass-boost.json
resonance render in.wav out.wav --preset fixtures/presets/bass-boost.json
resonance bench --seconds 10
resonance doctor
```

`render` is the fastest way to check a preset without touching PipeWire: it
loads a WAV, builds the chain, processes it offline and writes a 32-bit float
WAV.

## Repository layout

```
crates/
  dsp-core/        parameters, graph, meters, biquads, FFT helpers
  dsp-effects/     catalog + all effect implementations
  preset-compat/   EasyEffects JSON parse/validate/export
  control-api/     axum HTTP/WS, state, static hosting
  pw-backend/      pw-dump discovery, virtual nodes, pw-cat bridge
  daemon/          process lifecycle, audio threads
  cli/             `resonance` binary
web/               Svelte 5 + TypeScript + Vite control surface
fixtures/          licensed test presets and a test tone
docs/              architecture, API contract, design brief, matrix, roadmap
packaging/         systemd unit and install script
```

## Tests

```bash
cargo test --workspace
cd web && npx svelte-check --tsconfig ./tsconfig.json && npm run build
```

The Rust suite covers biquad responses, envelope/gain computers, loudness,
delay lines, preset parsing/round-trip/legacy/unknown keys/missing assets,
PipeWire dump parsing, and per-effect DSP sanity tests.

## License

GPL-3.0-or-later, matching EasyEffects. The implementation is written from
public DSP literature and the EasyEffects preset format; no EasyEffects or LSP
source code is copied. IRs and models are not bundled.
