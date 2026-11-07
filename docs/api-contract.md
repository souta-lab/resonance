# Resonance control API v1

Base path: `/api/v1`. The daemon serves the built SPA from `/` and the API from
`/api/v1`. All responses are JSON.

## Envelope

Success:

```json
{ "data": { } }
```

Error:

```json
{ "error": { "code": "invalid_param", "message": "ratio must be >= 1", "details": { "field": "ratio" } } }
```

Codes: `bad_request`, `unauthorized`, `forbidden_origin`, `not_found`,
`revision_conflict`, `invalid_param`, `invalid_preset`, `missing_asset`,
`pipeline_error`, `internal`.

## Authentication

* `GET /session` returns a short-lived token. It is only issued to same-origin
  requests (Origin / Sec-Fetch-Site checked).
* Mutating requests must send `X-Resonance-Token: <token>`.
* WebSocket connections pass `?token=<token>`.
* The server binds to `127.0.0.1` by default.

```json
{ "data": { "token": "…", "expires_in": 86400 } }
```

## Revisions

Each pipeline has a monotonically increasing `revision`. Mutations include the
revision they were based on. A mismatch returns HTTP 409:

```json
{ "error": { "code": "revision_conflict", "message": "pipeline changed", "details": { "revision": 7 } } }
```

## Types

### Status

```json
{
  "daemon": { "version": "0.1.0", "uptime_s": 123, "pid": 4242, "mode": "pipewire" },
  "pipewire": {
    "available": true,
    "version": "1.6.9",
    "server": "pipewire-0",
    "virtual_nodes": { "output": "resonance-output", "input": "resonance-input" }
  },
  "dsp": {
    "sample_rate": 48000,
    "quantum": 128,
    "latency_frames": 256,
    "xruns": 0,
    "cpu_percent": 1.4,
    "dsp_p99_us": 145
  },
  "compat": { "baseline": "EasyEffects 8.3.2", "lsp_required": false, "effects": 33 },
  "revision": 3
}
```

### Pipeline

```json
{
  "kind": "output",
  "bypass": false,
  "revision": 3,
  "latency_frames": 128,
  "effects": [
    {
      "id": "equalizer#0",
      "kind": "equalizer",
      "name": "Equalizer",
      "bypass": false,
      "index": 0,
      "compat": "exact",
      "latency_frames": 0,
      "gain_reduction_db": 0.0,
      "params": {
        "input-gain": 0.0,
        "output-gain": 0.0,
        "dry": 1.0,
        "wet": 1.0,
        "bypass": false,
        "num-bands": 32,
        "split-channels": false,
        "balance": 0.0,
        "pitch-left": 0.0
      },
      "bands": {
        "left": [
          { "type": 0, "frequency": 1000, "gain": 0, "q": 1, "width": 4, "slope": 0, "solo": false, "mute": false }
        ],
        "right": []
      }
    }
  ]
}
```

`bands` is present only for `equalizer` and `mid_side_equalizer`; the channel
keys are `left`/`right` or `mid`/`side`. `params` always contains the common
keys (`input-gain`, `output-gain`, `dry`, `wet`, `bypass`) plus the effect's
catalog keys.

### Catalog

```json
{
  "groups": [{ "id": "eq", "label": "EQ & filters" }],
  "effects": [
    {
      "kind": "equalizer",
      "name": "Equalizer",
      "group": "eq",
      "summary": "32-band parametric EQ …",
      "compat": "exact",
      "latency": "zero",
      "params": [
        {
          "key": "num-bands",
          "label": "Bands",
          "group": "Bands",
          "advanced": false,
          "kind": { "type": "int", "min": 1, "max": 32, "default": 32, "unit": "count" }
        },
        {
          "key": "balance",
          "label": "Balance",
          "group": "Bands",
          "advanced": false,
          "kind": { "type": "float", "min": -1, "max": 1, "default": 0, "step": 0.01, "unit": "none", "log": false }
        }
      ],
      "bands": {
        "max_bands": 32,
        "channels": 2,
        "channel_labels": ["left", "right"],
        "fields": [
          { "key": "type", "label": "Type", "kind": { "type": "enum", "default": 0, "labels": ["Bell", "Lo-shelf"] } }
        ]
      },
      "assets": []
    }
  ]
}
```

`kind.type` is one of `float`, `bool`, `int`, `enum`. Units: `none`, `db`, `hz`,
`ms`, `percent`, `ratio`, `seconds`, `octaves`, `degrees`, `count`, `semitones`,
`cents`, `bpm`, `db_per_oct`.

### Devices and streams

```json
{
  "outputs": [
    { "id": 42, "name": "alsa_output.pci-0000_00_1f.3.analog-stereo", "description": "Built-in Audio", "nick": "Speakers", "is_default": true, "state": "running", "profile": "analog-stereo" }
  ],
  "inputs": []
}
```

```json
[
  { "id": 55, "app": "Firefox", "name": "Playback", "media": "audio", "direction": "output", "linked": true, "blocked": false, "target": "resonance-output" }
]
```

### Presets

List item:

```json
{ "id": "bass-boost", "name": "Bass Boost", "kind": "output", "source": "local", "effects": 3, "missing_assets": [], "updated_at": "2026-11-07T12:00:00Z" }
```

Detail:

```json
{
  "id": "bass-boost",
  "name": "Bass Boost",
  "kind": "output",
  "blocklist": [],
  "plugins_order": ["equalizer#0", "limiter#0"],
  "instances": { "equalizer#0": { "bypass": false } },
  "warnings": [],
  "missing_assets": []
}
```

### Metrics

```json
{ "cpu_percent": 1.2, "dsp_p50_us": 80, "dsp_p99_us": 145, "xruns": 0, "latency_frames": 256, "sample_rate": 48000, "quantum": 128, "uptime_s": 123, "frames": 123456 }
```

## Endpoints

| Method | Path | Body / query | Returns |
|---|---|---|---|
| GET | `/status` | | Status |
| GET | `/session` | | token |
| GET | `/pipelines` | | Pipeline[] |
| GET | `/pipelines/{kind}/effects` | | Pipeline |
| POST | `/pipelines/{kind}/effects` | `{kind, index?, params?, bands?}` | `{effect, revision, warnings}` |
| PATCH | `/pipelines/{kind}/effects/{id}` | `{revision, params?, bands?, bypass?}` | `{effect, revision, warnings}` |
| DELETE | `/pipelines/{kind}/effects/{id}` | `?revision=` | `{revision}` |
| POST | `/pipelines/{kind}/effects/{id}/duplicate` | `{revision}` | Effect |
| PUT | `/pipelines/{kind}/order` | `{revision, order: string[]}` | Pipeline |
| PUT | `/pipelines/{kind}/bypass` | `{revision, bypass}` | Pipeline |
| GET | `/effects/catalog` | | Catalog |
| GET | `/devices` | | `{outputs, inputs}` |
| GET | `/streams` | | Stream[] |
| GET | `/presets` | | PresetListItem[] |
| POST | `/presets` | `{name, kind, plugins_order?, instances?, blocklist?}` | Preset |
| GET | `/presets/{id}` | | Preset |
| DELETE | `/presets/{id}` | | `{deleted: true}` |
| POST | `/presets/import` | `{json, name?}` | `{preset, warnings, missing_assets}` |
| POST | `/presets/{id}/apply` | `{pipeline, revision, allow_missing?}` | `{pipeline, warnings, missing_assets}` |
| GET | `/presets/{id}/export` | | EasyEffects JSON (attachment) |
| GET | `/metrics` | | Metrics |
| GET | `/config` | | Config |
| PATCH | `/config` | partial Config | Config |
| GET | `/autoload` | | AutoloadRule[] |
| PUT | `/autoload` | `{rules: AutoloadRule[]}` | AutoloadRule[] |
| POST | `/diagnostics/self-test` | | `{checks: [{name, ok, detail}]}` |
| GET | `/events` | WS, `?token=` | event stream |

`kind` is `input` or `output`.

## WebSocket events

```json
{ "type": "meter", "pipeline": "output", "timestamp_ms": 0, "peak_dbfs": [-8.2, -7.9], "true_peak_dbfs": [-7.7, -7.5], "rms_dbfs": [-18.0, -17.4], "lufs_m": -14.2, "lufs_s": -14.5, "gain_reduction_db": -3.1, "dsp_p99_us": 145, "xruns": 0 }
```

```json
{ "type": "spectrum", "pipeline": "output", "bins": [0.0], "sample_rate": 48000 }
```

```json
{ "type": "graph", "pipeline": "output", "revision": 4 }
```

```json
{ "type": "device", "action": "added", "device": { } }
```

```json
{ "type": "error", "code": "pipeline_error", "message": "…" }
```

`POST /pipelines/{kind}/effects` and `PATCH .../effects/{id}` return the updated
effect plus the new pipeline revision and any parameter warnings. `POST
/presets/{id}/apply` returns the new pipeline, the preset warnings and the
missing assets (empty when `allow_missing` was not needed).

Meters are only produced while at least one WebSocket client is subscribed.
The server drops stale frames instead of queueing them (backpressure by
coalescing).
