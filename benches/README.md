# Benchmarks

The DSP benchmark lives in the CLI so that it uses exactly the same code path
as the daemon:

```bash
cargo build --release
./target/release/resonance bench --seconds 10
```

It builds Equalizer → Compressor → Limiter, processes pink-ish noise at a fixed
sample rate and quantum, and reports p50/p95/p99/max DSP time, the quantum
budget and the realtime factor. See `docs/benchmarks.md` for the method and the
reference numbers.

A future `criterion` harness can live here once the acoustic comparison against
EasyEffects is automated; the current benchmark is intentionally dependency-free
so it can run on a minimal machine.
