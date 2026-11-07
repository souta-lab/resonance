# Benchmarks

## Method

`resonance bench` builds a representative chain and processes pink-ish noise
for N seconds at a fixed sample rate and quantum. It reports the per-block DSP
time distribution (p50/p95/p99/max), the quantum budget and the realtime
factor. It does not include the PipeWire bridge or the UI.

```bash
cargo build --release
./target/release/resonance bench --seconds 10 --sample-rate 48000 --quantum 128
```

The chain is Equalizer (32 bands, both channels) → Compressor → Limiter, with
default parameters. This is chain (A)+(B) from the specification, minus the
de-esser and gate.

For an EasyEffects comparison, run the same chain in EasyEffects with the UI
closed, measure with `perf stat`/`powertop`/`turbostat` on the same machine and
record the same percentiles. That comparison has **not** been run yet; the
numbers below are Resonance-only and must not be presented as a speed-up claim.

## Reference run

Machine: 6 logical cores, Linux 7.2.9, PipeWire 1.6.9, Rust 1.97.1 release
build with thin LTO. 48 kHz, 128-frame quantum (2.667 ms budget).

| Metric | Value |
|---|---:|
| p50 | 55.6 µs |
| p95 | 60.3 µs |
| p99 | 72.7 µs |
| max | 494.2 µs (first-block outlier) |
| quantum budget | 2666.7 µs |
| p99 share of budget | 2.7% |
| realtime factor | 46.7× |
| xruns (10 s) | 0 |

Run the command on your machine; the numbers are hardware-dependent. The
release profile uses `lto = "thin"`, `codegen-units = 1` and `panic = "abort"`.

The max value is dominated by the first block (FFT planning and cache warm-up
happen in `prepare`, but the first process call still touches cold memory).
Steady-state p99 is the number that matters for xrun risk.

## What is measured

* **DSP time** — wall time around `Pipeline::process`, which includes every
  effect, the meter update and the spectrum tap (every fourth block).
* **Quantum budget** — `quantum / sample_rate`.
* **Realtime factor** — audio seconds processed per wall second.

## What is not measured yet

* CPU time vs EasyEffects on an identical chain.
* Additional algorithmic latency per effect (the API reports it per instance;
  the limiter reports its lookahead, the convolver its partition size).
* Memory footprint and power draw.
* ML-effect profiles (the substitutes are cheap; the real models are not
  implemented).

## Reproducing

```bash
# 1. Build
cargo build --release

# 2. Run the benchmark
./target/release/resonance bench --seconds 30

# 3. Offline render for acoustic comparison
./target/release/resonance render fixtures/test-tone.wav /tmp/out.wav \
  --preset fixtures/presets/bass-boost.json
```

The render path uses the same `Pipeline` and effects as the real-time path, so
an offline render is a valid way to inspect the DSP without PipeWire.
