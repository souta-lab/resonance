// Types mirroring docs/api-contract.md.

export type PipelineKind = 'input' | 'output';

export type CompatLevel = 'exact' | 'approximate' | 'substitute' | 'meter';

export type Unit =
  | 'none'
  | 'db'
  | 'hz'
  | 'ms'
  | 'percent'
  | 'ratio'
  | 'seconds'
  | 'octaves'
  | 'degrees'
  | 'count'
  | 'semitones'
  | 'cents'
  | 'bpm'
  | 'db_per_oct';

export type ParamKind =
  | { type: 'float'; min: number; max: number; default: number; step: number; unit: Unit; log: boolean }
  | { type: 'bool'; default: boolean }
  | { type: 'int'; min: number; max: number; default: number; unit: Unit }
  | { type: 'enum'; default: number; labels: string[] };

export interface ParamSpec {
  key: string;
  label: string;
  group: string;
  advanced: boolean;
  kind: ParamKind;
}

export interface BandField {
  key: string;
  label: string;
  kind: ParamKind;
}

export interface BandLayout {
  max_bands: number;
  channels: number;
  channel_labels: string[];
  fields: BandField[];
}

export interface AssetSpec {
  key: string;
  label: string;
  kind: 'impulse_response' | 'model';
  extensions: string[];
}

export interface EffectDescriptor {
  kind: string;
  name: string;
  group: string;
  summary: string;
  compat: CompatLevel;
  latency: 'zero' | 'block' | 'adaptive';
  params: ParamSpec[];
  bands: BandLayout | null;
  assets: AssetSpec[];
}

export interface Catalog {
  groups: { id: string; label: string }[];
  effects: EffectDescriptor[];
}

export type ParamValue = number | boolean;

export interface Band {
  type: number;
  frequency: number;
  gain: number;
  q: number;
  width: number;
  slope: number;
  solo: boolean;
  mute: boolean;
}

export interface EffectInstance {
  id: string;
  kind: string;
  name: string;
  bypass: boolean;
  index: number;
  compat: CompatLevel;
  latency_frames: number;
  gain_reduction_db: number;
  params: Record<string, ParamValue>;
  bands?: Record<string, Band[]>;
}

export interface Pipeline {
  kind: PipelineKind;
  bypass: boolean;
  revision: number;
  latency_frames: number;
  effects: EffectInstance[];
}

export interface Status {
  daemon: { version: string; uptime_s: number; pid: number; mode: 'pipewire' | 'offline' };
  pipewire: {
    available: boolean;
    version: string;
    server: string;
    virtual_nodes: { output: string; input: string };
  };
  dsp: {
    sample_rate: number;
    quantum: number;
    latency_frames: number;
    xruns: number;
    cpu_percent: number;
    dsp_p99_us: number;
  };
  compat: { baseline: string; lsp_required: boolean; effects: number };
  revision: number;
}

export interface Device {
  id: number;
  name: string;
  description: string;
  nick: string;
  is_default: boolean;
  state: string;
  profile: string;
}

export interface Devices {
  outputs: Device[];
  inputs: Device[];
}

export interface Stream {
  id: number;
  app: string;
  name: string;
  media: string;
  direction: 'input' | 'output';
  linked: boolean;
  blocked: boolean;
  target: string;
}

export interface MissingAsset {
  instance: string;
  key: string;
  name: string;
  kind: string;
}

export interface PresetListItem {
  id: string;
  name: string;
  kind: PipelineKind;
  source: 'local' | 'community' | 'imported';
  effects: number;
  missing_assets: MissingAsset[];
  updated_at: string;
}

export interface Preset {
  id: string;
  name: string;
  kind: PipelineKind;
  blocklist: string[];
  plugins_order: string[];
  instances: Record<string, Record<string, unknown>>;
  warnings: string[];
  missing_assets: MissingAsset[];
}

export interface Metrics {
  cpu_percent: number;
  dsp_p50_us: number;
  dsp_p99_us: number;
  xruns: number;
  latency_frames: number;
  sample_rate: number;
  quantum: number;
  uptime_s: number;
  frames: number;
}

export interface MeterEvent {
  type: 'meter';
  pipeline: PipelineKind;
  timestamp_ms: number;
  peak_dbfs: [number, number];
  true_peak_dbfs: [number, number];
  rms_dbfs: [number, number];
  lufs_m: number;
  lufs_s: number;
  gain_reduction_db: number;
  dsp_p99_us: number;
  xruns: number;
}

export interface SpectrumEvent {
  type: 'spectrum';
  pipeline: PipelineKind;
  bins: number[];
  sample_rate: number;
}

export interface GraphEvent {
  type: 'graph';
  pipeline: PipelineKind;
  revision: number;
}

export interface DeviceEvent {
  type: 'device';
  action: 'added' | 'removed' | 'changed';
  device: Device;
}

export interface ErrorEvent {
  type: 'error';
  code: string;
  message: string;
}

export type ServerEvent = MeterEvent | SpectrumEvent | GraphEvent | DeviceEvent | ErrorEvent;

export interface AutoloadRule {
  id: string;
  device: string;
  route: string;
  pipeline: PipelineKind;
  preset: string;
  enabled: boolean;
}

export interface Config {
  bind: string;
  port: number;
  theme: 'bench' | 'scope';
  meter_rate_hz: number;
  log_level: string;
  autoload: boolean;
  virtual_nodes: boolean;
}

export interface SelfTestCheck {
  name: string;
  ok: boolean;
  detail: string;
}
