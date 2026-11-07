# Verification log

Commands run on the development machine (Debian forky/sid, PipeWire 1.6.9,
Rust 1.97.1, Node 25.9). Results are from the final tree.

## Build

```text
$ cargo build --release -p resonance-cli
Finished `release` profile [optimized] target(s) in 59.68s

$ cd web && npm run build
✓ built in 586ms
```

## Tests

```text
$ cargo test --workspace
dsp-core        14 passed
dsp-effects     38 passed
preset-compat   10 passed
preset-compat (fixtures) 5 passed
pw-backend       1 passed
```

Coverage highlights:

* biquad magnitude at the centre frequency and at the cutoff;
* envelope follower and gain computer ratio;
* BS.1770 loudness of a −20 dBFS 1 kHz sine (≈ −23 LUFS);
* delay-line integer and fractional reads;
* preset parse/round-trip, legacy ids, unknown keys, unknown plugins,
  path traversal, missing IR, sorted export;
* PipeWire `pw-dump` parsing (devices, defaults, streams);
* per-effect DSP sanity (delay timing, crusher quantisation, pitch detection,
  noise attenuation, echo cancellation, reverb decay, convolver delta IR).

## API smoke test

```text
GET  /api/v1/status                 200, 33 effects, PipeWire 1.6.9
GET  /api/v1/pipelines              200, output + input
POST /api/v1/pipelines/output/effects {"kind":"equalizer"}   200
PATCH .../effects/compressor%230     {"threshold":-12.5}      200
POST /api/v1/presets/bass-boost/apply                        200
GET  /api/v1/metrics                200, p99 17.9 µs, 0 xruns
```

Security checks:

```text
POST without token                  401
POST with Origin: https://evil      403
PATCH with stale revision           409 revision_conflict
apply preset with missing IR        422 missing_asset + details
```

## PipeWire path

```text
$ resonance daemon
INFO virtual nodes created output="resonance-output" input="resonance-input"
INFO audio bridge running pipeline="output"
INFO audio bridge running pipeline="input"

$ pw-play --target resonance-output fixtures/test-tone.wav
# WebSocket meter events:
peak [-9.0, -6.0] dBFS   true peak [-9.0, -6.0]   LUFS-M -12.3
```

After applying `bass-boost` (EQ input gain −4 dB, limiter lookahead 5 ms):

```text
peak [-12.8, -11.3] dBFS   LUFS-M -16.3
pipeline latency 240 frames (limiter lookahead)
```

Convolver with a generated 1 s IR:

```text
apply convolver-test -> latency 1024 frames, no error
```

## Offline render

```text
$ resonance render fixtures/test-tone.wav /tmp/out.wav \
    --preset fixtures/presets/bass-boost.json
Rendered 96000 frames (2.00 s) at 48000 Hz, 2 ch
  effects 2 · latency 240 frames · peak -11.33 dBFS
```

## Benchmark (release)

```text
$ resonance bench --seconds 10
Chain: EQ + Compressor + Limiter · 48000 Hz · 128 frames · 10 s
  p50 55.6 µs · p95 60.3 µs · p99 72.7 µs · max 494.2 µs
  quantum budget 2666.7 µs · p99 uses 2.7% · realtime factor 46.7x
```

## Doctor

```text
$ resonance doctor
[ok] pw-dump / pw-cat / pw-cli
[ok] LSP Plugins: not installed (not required)
[ok] effect catalog: 33 effects
[ok] web build
[ok] data directory
[ok] daemon 0.1.0 pipewire
```

## WebUI

```text
$ npx svelte-check --tsconfig ./tsconfig.json
svelte-check found 0 errors and 0 warnings

$ npm run build
✓ built in 1.35s
```

Screenshots in `docs/screenshots/`: output chain, presets, devices, analyzer,
settings, night scope, mobile (390×844). The UI was driven against the live
daemon (not only mock mode) for the captures.

## Not verified

* Acoustic comparison against EasyEffects. No reference renders were produced
  on this machine, so no compatibility claim beyond "same algorithm class" is
  made.
* Long-running (60 min) xrun test.
* Bluetooth/USB hot-plug and suspend/resume.
* Screen-reader pass on the WebUI.
* The `pw-cat` bridge under heavy multi-stream load.
