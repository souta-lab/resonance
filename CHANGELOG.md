# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Planned

- Native `pw_filter` backend with graph latency reporting
- Stream blocklist enforcement and per-application routing
- Acoustic comparison harness against EasyEffects
- Shared audio, microphone monitoring and a test-signal UI

## [0.1.0] - 2026-11-07

First public early-access release.

### Added

- **33 native effects** implemented in Rust: equalizers, filters, dynamics,
  space/time, tone shaping, voice/pitch, noise/echo and metering.
- **EasyEffects 8.3.2 preset compatibility**: modern and legacy instance ids,
  multiple instances, `blocklist`, band objects, enum labels, lossless
  preservation of unknown keys, sorted export with a trailing newline.
- **Real-time safe graph**: atomic parameters, `try_lock` graph hand-over,
  retired graphs dropped off the audio thread, click-free bypass and dry/wet.
- **Control API** under `/api/v1`: status, pipelines, effects, catalog,
  devices, streams, presets, metrics and a WebSocket event stream.
- **Svelte 5 control surface**: rack view, effect faceplates, EQ editor,
  meters, analyzer, presets, devices, diagnostics, light/dark themes and a
  mobile layout.
- **CLI**: `daemon`, `status`, `effects`, `preset import/export/apply`,
  `render` (offline WAV processing), `doctor` and `bench`.
- **PipeWire integration** via `pw-dump`, `pw-cli` and a `pw-cat` bridge,
  including `resonance-output` and `resonance-input` virtual sinks.
- **Packaging**: systemd user unit and an install script.
- **Tests**: 69 Rust tests plus `svelte-check` and a production web build.

### Known limitations

- The PipeWire path uses a `pw-cat` bridge, not a native `pw_filter`.
- Model-based effects (Deep Noise Remover, RNNoise/DPDFNet paths) are
  algorithmic substitutes and are labelled as such in the catalog.
- No acoustic comparison against EasyEffects has been published yet.
- Some upstream keys are preserved but not wired to DSP controls.
- Stream blocklist enforcement, shared audio and mic monitoring are not
  implemented.

[Unreleased]: https://github.com/s0u7a/resonance/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/s0u7a/resonance/releases/tag/v0.1.0
