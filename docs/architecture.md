# Architecture

## Process model

```
┌──────────────────────────────────────────────────────────────┐
│ resonance daemon                                             │
│                                                              │
│  tokio runtime                                               │
│   ├─ control API (axum)          HTTP + WebSocket            │
│   ├─ meter pump                  33 ms, only when subscribed │
│   ├─ device poller               pw-dump every 2 s           │
│   └─ autoload watcher            rules → preset apply        │
│                                                              │
│  audio threads (one per pipeline, std::thread)               │
│   ├─ output: pw-cat capture → DSP → pw-cat playback          │
│   └─ input:  pw-cat capture → DSP → pw-cat playback          │
└──────────────────────────────────────────────────────────────┘
```

The control API and the audio threads share only lock-free structures:

* `Arc<ParamStore>` — one atomic `f32` per parameter.
* `Arc<MeterBank>` — atomics for peak, true peak, RMS, LUFS, DSP time, xruns.
* `Arc<GraphSwap>` — the hand-over point for whole graphs.

The audio thread never allocates, never takes a blocking lock, never logs and
never performs IO. The only syscall it makes is `clock_gettime` for the DSP
timing histogram (vDSO).

## Graph hand-over

Structural changes (add / remove / reorder / preset apply) are prepared on the
control thread:

1. Build a new `Pipeline` with freshly prepared effects.
2. `GraphSwap::install` stores it in a `Mutex<SwapSlot>`.
3. The audio thread calls `GraphSwap::take_incoming()` which uses `try_lock`.
   If the control thread happens to hold the lock, the audio thread keeps the
   previous graph for one block and tries again.
4. The replaced graph is pushed back through `GraphSwap::retire()` (also
   `try_lock`). If that fails it is kept in a single-slot `pending_retire` and
   retried next block. The control thread drains retired graphs and drops them
   off the audio thread.

Parameter changes do not rebuild the graph: they write atomics. Effects read
them every block and recompute coefficients only when a value actually changed.

## Parameter model

* Ids `0..16` are reserved: `input-gain`, `output-gain`, `dry`, `wet`, `bypass`.
* Effect parameters start at 16 and are resolved by key at `prepare` time, so
  the catalog order is not part of the ABI.
* Equalizer bands live at `BAND_BASE` (1024) and are laid out by `BandLayout`
  (`channel × band × field`).
* Values are stored as `f32` bits. Booleans are 0/1, enums are their index.
* `ParamStore::set` coerces to the declared range, so a bad API request cannot
  push a filter outside its domain.

## Bypass and dry/wet

Each slot keeps a smoothed wet/dry mix. Bypass ramps the mix to 0 over one
block, so toggling does not click. When the mix reaches 0 the effect is skipped
entirely and reset; re-enabling resets it again so stale delay/reverb tails do
not leak. The pipeline has the same mechanism for global bypass.

## Metering

`Pipeline::process` measures peak, a 4× interpolated true-peak estimate, RMS and
K-weighted loudness (BS.1770-4) after the chain, and writes them to atomics.
The spectrum tap runs a 2048-point FFT every fourth block and writes 96
log-spaced bins. The meter pump only broadcasts while at least one WebSocket
client is subscribed; slow clients are dropped by the broadcast channel instead
of queueing.

## PipeWire integration

The daemon does not link against libpipewire. It uses the PipeWire CLI tools:

* `pw-dump` for device and stream discovery (JSON).
* `pw-cli create-node adapter` to create `resonance-output` and
  `resonance-input` null sinks (`object.linger=true`).
* `pw-cat --record/--playback --raw --format f32` to move audio.

The output path captures the monitor of `resonance-output`, processes it and
plays it to the default device. The input path captures the default microphone,
processes it and plays it into `resonance-input`, whose monitor applications can
select as a microphone.

This bridge is deliberately simple: it is a real PipeWire path, but it adds the
`pw-cat` buffering latency (configured to 20 ms) on top of the DSP latency. A
native `pw_filter` backend is the next step (see `docs/roadmap.md`).

## Preset pipeline

`preset-compat` parses EasyEffects JSON into a lossless model:

* `plugins_order` is authoritative; instance ids are normalized to `kind#N`.
* Unknown keys stay in the instance map and are written back on export.
* Known keys are validated against the catalog; invalid values produce
  warnings, not silent drops.
* Band objects (`left.band0`) are mapped to the band layout; enum labels are
  mapped to indices and back.
* Export sorts keys, uses 4-space indentation and a trailing newline, matching
  the community preset validator.

Applying a preset builds every effect first. If any effect fails to prepare,
the previous graph is kept and the API returns an error. Missing IRs/models are
reported before applying; `allow_missing` is an explicit opt-in.

## Security

* The server binds to `127.0.0.1` by default.
* Mutations require `X-Resonance-Token`, issued by `GET /api/v1/session` to
  same-origin requests only.
* `Origin` is checked on mutations and on the session endpoint.
* WebSocket connections require the token in the query string.
* Asset names are rejected if they contain `/`, `\` or `..`; resolution is
  limited to the configured directories.
* Preset and parameter input is size/type checked before it reaches the DSP.

## Failure behaviour

* A broken preset never replaces a working graph.
* A failed effect `prepare` is reported with the instance id.
* If PipeWire disappears, the bridge thread exits and the daemon keeps serving
  the UI; the device poller reports `available: false`.
* If the web build is missing, the daemon serves a placeholder page that
  explains how to build it.
