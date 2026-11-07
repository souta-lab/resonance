# Design brief — Resonance control surface

## Product

Resonance is a PipeWire audio-effects rack: a Rust daemon does the DSP, a
browser is the control surface. The people using it are Linux audio users who
know what a compressor ratio is. They open the UI to change a chain, load a
preset, check why a stream is silent, or watch levels. It is a tool, not a
landing page.

## Direction: calibrated bench instrument

The reference world is lab and studio hardware: engraved faceplates, LED
ladders, graph paper, patch bays, a scope trace. The UI should feel like a
physical instrument that happens to be rendered in a browser. It must not look
like a SaaS dashboard, a shadcn demo, or a generic "audio app".

Concretely:

* Flat panels with 1px engraved rules and inset shadows, not floating cards.
* 2px corner radius everywhere. No pill buttons, no 16px cards.
* Segmented LED meters, not smooth gradient bars.
* Graph-paper plots with a phosphor trace.
* Numbers are monospaced and tabular. Units are small and quiet.
* One accent (signal amber) used for the active/selected state and the trace.
  A second, cooler trace colour (teal) is used only for measurement overlays.
* No purple, no gradients as decoration, no glassmorphism, no emoji, no
  scroll-reveal animations, no "hero" sections.

## Tokens

Light theme ("bench"):

| Token | Value |
|---|---|
| `--chassis` | `#E7E2D6` |
| `--panel` | `#F4F0E6` |
| `--panel-sunken` | `#DCD6C7` |
| `--ink` | `#191B1C` |
| `--ink-2` | `#4A4B47` |
| `--ink-3` | `#7A7A72` |
| `--rule` | `#C6BFAD` |
| `--rule-strong` | `#A79E88` |
| `--signal` | `#B25E00` |
| `--signal-soft` | `#E9C9A0` |
| `--trace` | `#2C6B66` |
| `--alert` | `#A3311F` |
| `--ok` | `#3F6B3A` |

Dark theme ("night scope"):

| Token | Value |
|---|---|
| `--chassis` | `#131516` |
| `--panel` | `#1A1D1E` |
| `--panel-sunken` | `#0E1011` |
| `--ink` | `#E8E2D4` |
| `--ink-2` | `#A8A69C` |
| `--ink-3` | `#6E6F69` |
| `--rule` | `#2E3334` |
| `--rule-strong` | `#454B4C` |
| `--signal` | `#E39A3B` |
| `--signal-soft` | `#5A4322` |
| `--trace` | `#6FB6AE` |
| `--alert` | `#D4614C` |
| `--ok` | `#7FA86F` |

Type: **IBM Plex Sans** for UI, **IBM Plex Mono** for numbers, units, keys and
the wordmark. Bundle both with `@fontsource/ibm-plex-sans` and
`@fontsource/ibm-plex-mono` (weights 400/500/600). Do not use Inter, Geist,
system-ui as the primary face, or a serif.

Scale: 11px (micro labels), 12px (secondary), 13px (body), 15px (panel titles),
18px (screen titles), 24px (large readouts), 32px (hero numbers only if needed).
Line height 1.4 for body, 1.1 for readouts. Letter-spacing 0 except the
wordmark (0.08em) and micro labels (0.04em).

Spacing: 4px base. 4/8/12/16/24/32. Panel padding 16px. Rail width 280px,
meter rail 300px, nav rail 64px, header 52px, status bar 28px.

## Layout

```
┌────┬──────────────┬──────────────────────────────┬──────────────┐
│nav │ signal chain │ effect faceplate             │ meter bridge │
│64  │ 280          │ 1fr                          │ 300          │
│    │              │                              │              │
│    │ 01 equalizer │  Equalizer        [bypass]   │  L ▮▮▮▮▮▮▮   │
│    │ 02 limiter   │  ┌────────────────────────┐  │  R ▮▮▮▮▮▮    │
│    │ + add effect │  │  EQ graph + band table │  │  peak -8.2   │
│    │              │  └────────────────────────┘  │  true -7.7   │
│    │              │  knobs / switches            │  LUFS -14.2  │
├────┴──────────────┴──────────────────────────────┴──────────────┤
│ analyzer drawer (collapsible): spectrum + EQ curve, 220px        │
├──────────────────────────────────────────────────────────────────┤
│ status bar: PipeWire 48 kHz · 128 · xruns 0 · DSP p99 145 µs     │
└──────────────────────────────────────────────────────────────────┘
```

* Nav rail: six destinations — Output, Input, Presets, Devices & Apps,
  Analyzer, Settings. Icon + 10px label under it. Active item gets an amber
  2px left edge and a sunken background.
* Signal chain: a vertical rack. Each slot is a 1U row with an engraved index
  (`01`), the effect name, a bypass toggle, a drag handle, and a thin GR bar.
  Selected slot is raised (panel + amber left edge). Drag to reorder; Alt+↑/↓
  also reorders. "Add effect" opens a searchable catalog popover grouped by
  category.
* Faceplate: header (name, compat badge, latency, bypass), then parameter
  groups. Floats are rotary knobs (drag vertically, Shift = fine, double-click
  = reset). Enums are segmented controls. Bools are physical-looking switches.
  Advanced params are behind a disclosure. EQ gets a graph with draggable band
  points and a band table.
* Meter bridge: two vertical LED ladders (L/R) with peak-hold ticks, numeric
  peak / true peak / RMS / LUFS-M / LUFS-S, a gain-reduction meter, and a small
  spectrum strip. Meters decay smoothly; no animation when the tab is hidden.
* Analyzer drawer: graph-paper plot, spectrum filled with a low-opacity teal,
  EQ curve in amber, dB grid, frequency ticks 20/50/100/200/500/1k/2k/5k/10k/20k.
* Status bar: connection LED, PipeWire state, sample rate, quantum, xruns,
  DSP p99, latency, daemon version.

## Screens

1. **Output / Input** — the rack described above. Empty state: "No effects in
   this chain." with an "Add effect" button and a hint that presets can be
   applied.
2. **Presets** — two columns: list (search, kind filter, source badge) and
   detail (plugins order, instance summary, missing assets, warnings). Actions:
   apply to Output/Input, export, duplicate, delete, import (file picker +
   paste JSON). Import errors show the exact validation message.
3. **Devices & Apps** — device cards (output/input) with default badge and
   state; stream table with app, direction, target, linked state, block toggle.
   Autoload rules table (device → preset).
4. **Analyzer** — large spectrum + EQ curve, peak/LUFS history strip, band
   table for the selected EQ.
5. **Settings & Diagnostics** — daemon info, bind address, theme, meter rate,
   self-test results, log level, "copy diagnostics" button.

## Motion

Only functional: 120ms panel cross-fade when switching effects, 80ms knob
response, meter decay, popover open/close. Respect
`prefers-reduced-motion: reduce` by removing transitions.

## Accessibility

* Visible focus ring: 2px `--signal` offset 2px.
* All controls reachable by keyboard. Knobs support arrow keys (↑/↓ = step,
  Shift = fine, Home/End = min/max).
* Reorder via Alt+↑/↓ with an aria-live announcement.
* Meters have `role="meter"` with aria-valuenow/min/max and a text label.
* Contrast: body text ≥ 7:1 on panel, secondary ≥ 4.5:1.
* Touch targets ≥ 32px on mobile; the layout collapses to a single column with
  a bottom tab bar under 900px.

## Copy

Plain, specific, no filler. "Add effect", "Apply to Output", "Export preset",
"No effects in this chain.", "PipeWire is not running. Start it with
`systemctl --user start pipewire`." Errors state what failed and what to do.
Never say "Oops", "Something went wrong", or use exclamation marks.

## Anti-slop checklist

Before finishing, verify: no purple/indigo, no gradient text, no glass, no
emoji icons, no rounded-everything, no centered hero, no three-card row, no
scroll reveals, no Inter/Geist, no "AI" copy. If a screen looks like it could
belong to any SaaS product, redo it.
