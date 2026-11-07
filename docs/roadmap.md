# Roadmap and known gaps

Status against the milestones in the specification.

| Milestone | Status | Notes |
|---|---|---|
| M0 Research | partial | Catalog, preset format and compatibility levels are implemented. The full upstream parameter inventory (every GSettings key per plugin) is not yet transcribed. |
| M1 Audio MVP | done | PipeWire in/out, Rust graph, EQ + limiter, minimal UI. The PipeWire path is a `pw-cat` bridge, not a native `pw_filter`. |
| M2 Compatibility | done | JSON import/export, legacy ids, unknown-key preservation, asset resolution, validation warnings. |
| M3 Basic DSP | done | EQ, dynamics, filter, stereo tools, delay, crossfeed, crusher, meter. |
| M4 Advanced DSP | done with substitutes | Convolver, reverb, pitch, autotune, harmonizer, noise reduction, echo canceller. Model-based effects are algorithmic stand-ins. |
| M5 Full integration | partial | UI, presets, devices, autoload, diagnostics. Missing: native filter backend, stream blocklist enforcement, shared audio, mic monitoring, test-signal UI. |
| M6 v1.0 | not started | Acoustic validation, EasyEffects benchmark comparison, packaging, migration tests. |

## Concrete gaps

### Audio path

* Replace the `pw-cat` bridge with a native `pw_filter` node. The bridge adds
  the `pw-cat` buffer latency (20 ms configured) and cannot report the true
  graph latency.
* Per-application routing and blocklist enforcement. The API exposes streams
  and a `blocked` flag, but the daemon does not yet move or block links.
* Device profile/route matching for autoload. Rules currently match on
  `node.name` only.
* Shared audio, microphone monitoring and the test-signal generator are only
  available through the CLI (`--test-signal`), not the UI.

### DSP

* Neural models: Deep Noise Remover, RNNoise/DPDFNet paths. The catalog marks
  these as `substitute` and the UI shows the badge.
* Limiter oversampling and dithering are accepted as parameters but the audio
  path uses 4× interpolated peak detection rather than a true oversampled
  limiter. `alr` is not implemented.
* Compressor external sidechain routing is not implemented; the internal
  detector with HPF/LPF is.
* Equalizer `mode` (IIR/FIR/FFT) is preserved but only the IIR path runs.
* Pitch/autotune quality is functional but not comparable to a phase vocoder
  or a neural pitch tracker.

### Compatibility

* Full key coverage per effect. Unknown keys survive round-trips, but some are
  not mapped to controls yet (see `docs/compatibility-matrix.md`).
* Acoustic comparison against EasyEffects: no automated harness yet. The
  `render` command and the fixture WAV are the starting point.
* Community preset corpus with license review.

### Product

* Accessibility audit (keyboard paths exist; screen-reader labels need a pass).
* Packaging: Flatpak, deb/rpm, and a proper install layout for the web assets.
* Benchmark report against EasyEffects on the same machine.

## Next three steps

1. Native `pw_filter` backend with graph latency reporting.
2. Acoustic test harness: impulse/sweep/pink-noise renders compared against
   EasyEffects for the `exact` effects, with per-effect tolerances.
3. Stream routing: move/block links for the blocklist and per-app selection.
