**Native Rust DSP for PipeWire. Your EasyEffects presets just work.**

This is the first public build of Resonance: a small Rust daemon that owns a
PipeWire effects graph, 33 native effects, and a browser control surface served
by the daemon itself. No LSP Plugins, no LV2 host, no Qt.

## ✨ Highlights

- **33 effects, all native Rust** — EQ, dynamics, space/time, tone, voice/pitch,
  noise/echo and metering.
- **EasyEffects 8.3.2 preset compatibility** — modern and legacy ids, multiple
  instances, `blocklist`, band objects, unknown keys preserved losslessly.
- **Real-time safe graph** — atomic parameters, non-blocking graph hand-over,
  click-free bypass and dry/wet.
- **Browser control surface** — rack view, EQ editor, LED meters, spectrum,
  presets, devices, diagnostics, light/dark themes, mobile layout.
- **Headless daemon + CLI** — `render` processes WAV files offline; `bench`
  reports DSP percentiles.
- **Local-first security** — loopback bind, session token, `Origin` checks.

## 📦 What is in this release

| Asset | Contents |
|---|---|
| `resonance-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` | Binary, built web UI, docs, systemd unit, install script |
| `SHA256SUMS` | Checksums for the archive |

## 🚀 Install

```bash
tar xzf resonance-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
cd resonance-v0.1.0-x86_64-unknown-linux-gnu
./install.sh
systemctl --user enable --now resonance.service
```

Or run it directly:

```bash
./resonance daemon
# open http://127.0.0.1:8737
```

Try it without touching PipeWire:

```bash
./resonance daemon --test-signal
```

## 📊 Performance

Equalizer → Compressor → Limiter, 48 kHz, 128-frame quantum, release build:

| Metric | Result |
|---|---:|
| p50 | 54.5 µs |
| p95 | 57.3 µs |
| p99 | 68.0 µs |
| p99 share of the 2.667 ms budget | 2.6 % |
| Realtime factor | 47.8× |

## ⚠️ Known limitations

- The PipeWire path uses a `pw-cat` bridge, not a native `pw_filter`; expect
  roughly 20 ms of extra buffering.
- Deep Noise Remover and the neural noise paths are algorithmic substitutes,
  labelled `substitute` in the catalog.
- No acoustic comparison against EasyEffects has been published yet.
- Some upstream keys are preserved but not wired to DSP controls.
- Stream blocklist enforcement, shared audio and mic monitoring are not
  implemented.

## 🙏 Thanks

To the EasyEffects, PipeWire, rustfft, axum, tokio, Svelte and Vite projects.
Resonance is written from public DSP literature and the EasyEffects preset
format; no EasyEffects or LSP source code is copied.

**Full changelog:** [CHANGELOG.md](https://github.com/souta-lab/resonance/blob/master/CHANGELOG.md)
