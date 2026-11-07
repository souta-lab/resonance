# Contributing to Resonance

Thanks for taking the time to contribute. This project is an early-access
reimplementation of the EasyEffects pipeline in Rust, so correctness and
honest compatibility reporting matter more than feature count.

## Code of conduct

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Development setup

```bash
git clone https://github.com/s0u7a/resonance.git
cd resonance

# Rust
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# Web
cd web
npm install --legacy-peer-deps
npm run dev          # http://localhost:5173, proxies /api to :8737
npx svelte-check --tsconfig ./tsconfig.json
npm run build
```

Run the daemon in another terminal:

```bash
./target/debug/resonance daemon --test-signal
```

`--test-signal` avoids touching PipeWire and gives the meters something to
show.

## Project layout

```
crates/dsp-core/        parameters, graph, meters, biquads, FFT helpers
crates/dsp-effects/     catalog + effect implementations
crates/preset-compat/   EasyEffects JSON parse/validate/export
crates/control-api/     axum HTTP/WS, state, static hosting
crates/pw-backend/      pw-dump discovery, virtual nodes, pw-cat bridge
crates/daemon/          process lifecycle, audio threads
crates/cli/             the `resonance` binary
web/                    Svelte 5 + TypeScript control surface
```

## Adding an effect

1. Add the variant to `EffectKind` in `crates/dsp-core/src/kind.rs`.
2. Add a descriptor to `crates/dsp-effects/src/catalog.rs` (name, group,
   summary, compatibility level, parameter specs, band layout, assets).
3. Implement `AudioEffect` in the appropriate module. Resolve parameter ids in
   `prepare` with `params.spec_by_key("key")`; never do string lookups in
   `process`.
4. Register it in `dsp_effects::create`.
5. Add unit tests for the DSP behaviour and a preset fixture if the effect has
   interesting serialization.
6. Update `docs/compatibility-matrix.md` (it is generated from the catalog).

Rules for `process`:

- No allocation, locks, logging or IO.
- Handle mono and stereo.
- Guard against NaN and denormals.
- Report algorithmic latency from `latency_frames`.

## Preset compatibility rules

- Unknown keys must survive `import → export → import`.
- Known keys are validated against the catalog; invalid values produce
  warnings, not silent drops.
- Enum values are stored as EasyEffects labels and converted to indices at load
  time.
- Export sorts keys, uses 4-space indentation and a trailing newline.
- Never silently skip a missing IR or model. Report it.

## Commit style

Conventional-ish, imperative, scoped when useful:

```
dsp: fix limiter lookahead latency reporting
web: keep the band table inside the viewport on mobile
preset: preserve unknown keys on legacy export
```

## Pull requests

- Keep changes focused; one concern per PR.
- Add or update tests.
- Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, and the web checks.
- Describe what changed and why, and call out compatibility or latency changes.
- If you change the DSP, include a short before/after measurement or a render.

## Reporting bugs

Use the issue template and include:

- Distribution and PipeWire version (`pipewire --version`)
- `resonance doctor` output
- The preset JSON (redact personal paths)
- Whether the daemon runs with `--test-signal`, the `pw-cat` bridge, or offline

## License

By contributing you agree that your contributions are licensed under
GPL-3.0-or-later.
