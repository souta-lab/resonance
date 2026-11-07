// In-memory mock of the control API, used for UI development with ?mock=1.

import type {
  AutoloadRule,
  ParamSpec,
  Band,
  Catalog,
  Config,
  Devices,
  EffectDescriptor,
  EffectInstance,
  Metrics,
  Pipeline,
  PipelineKind,
  Preset,
  PresetListItem,
  SelfTestCheck,
  ServerEvent,
  Status,
  Stream,
} from './types';

const now = () => new Date().toISOString();

const EQ_FIELDS = [
  { key: 'type', label: 'Type', kind: { type: 'enum', default: 0, labels: ['Bell', 'Lo-shelf', 'Hi-shelf', 'Lo-pass', 'Hi-pass', 'Notch', 'Band-pass', 'All-pass'] } },
  { key: 'frequency', label: 'Frequency', kind: { type: 'float', min: 10, max: 24000, default: 1000, step: 1, unit: 'hz', log: true } },
  { key: 'gain', label: 'Gain', kind: { type: 'float', min: -36, max: 36, default: 0, step: 0.1, unit: 'db', log: false } },
  { key: 'q', label: 'Q', kind: { type: 'float', min: 0.1, max: 10, default: 1, step: 0.01, unit: 'none', log: true } },
  { key: 'width', label: 'Width', kind: { type: 'float', min: 0, max: 10, default: 4, step: 0.1, unit: 'octaves', log: false } },
  { key: 'slope', label: 'Slope', kind: { type: 'float', min: 0, max: 10, default: 0, step: 0.1, unit: 'none', log: false } },
  { key: 'solo', label: 'Solo', kind: { type: 'bool', default: false } },
  { key: 'mute', label: 'Mute', kind: { type: 'bool', default: false } },
] as const;

function f(key: string, label: string, group: string, min: number, max: number, def: number, step: number, unit: string, log = false): ParamSpec {
  return { key, label, group, advanced: false, kind: { type: 'float', min, max, default: def, step, unit: unit as ParamSpec['kind'] extends { unit: infer U } ? U : never, log } };
}
function b(key: string, label: string, group: string, def: boolean): ParamSpec {
  return { key, label, group, advanced: false, kind: { type: 'bool', default: def } };
}
function e(key: string, label: string, group: string, def: number, labels: string[]): ParamSpec {
  return { key, label, group, advanced: false, kind: { type: 'enum', default: def, labels } };
}
function i(key: string, label: string, group: string, min: number, max: number, def: number, unit: string): ParamSpec {
  return { key, label, group, advanced: false, kind: { type: 'int', min, max, default: def, unit: unit as never } };
}

const EFFECTS: EffectDescriptor[] = [
  {
    kind: 'equalizer',
    name: 'Equalizer',
    group: 'eq',
    summary: '32-band parametric EQ with per-channel bands and pitch-scaled curves.',
    compat: 'exact',
    latency: 'zero',
    params: [
      i('num-bands', 'Bands', 'Bands', 1, 32, 32, 'count'),
      b('split-channels', 'Split channels', 'Bands', false),
      f('balance', 'Balance', 'Bands', -1, 1, 0, 0.01, 'none'),
      f('pitch-left', 'Pitch left', 'Bands', -12, 12, 0, 0.01, 'semitones'),
    ],
    bands: { max_bands: 32, channels: 2, channel_labels: ['left', 'right'], fields: EQ_FIELDS as never },
    assets: [],
  },
  {
    kind: 'compressor',
    name: 'Compressor',
    group: 'dynamics',
    summary: 'Feed-forward compressor with knee, sidechain filters and lookahead.',
    compat: 'approximate',
    latency: 'zero',
    params: [
      e('mode', 'Mode', 'Dynamics', 0, ['Downward', 'Upward', 'Boosting']),
      f('threshold', 'Threshold', 'Dynamics', -60, 0, -18, 0.1, 'db'),
      f('ratio', 'Ratio', 'Dynamics', 1, 20, 4, 0.1, 'ratio'),
      f('knee', 'Knee', 'Dynamics', 0, 12, 6, 0.1, 'db'),
      f('attack', 'Attack', 'Dynamics', 0.1, 500, 20, 0.1, 'ms', true),
      f('release', 'Release', 'Dynamics', 1, 2000, 250, 1, 'ms', true),
      f('makeup', 'Makeup', 'Dynamics', 0, 24, 0, 0.1, 'db'),
      f('lookahead', 'Lookahead', 'Dynamics', 0, 20, 0, 0.1, 'ms'),
    ],
    bands: null,
    assets: [],
  },
  {
    kind: 'limiter',
    name: 'Limiter',
    group: 'dynamics',
    summary: 'Lookahead brickwall limiter with optional oversampling and dithering.',
    compat: 'approximate',
    latency: 'adaptive',
    params: [
      f('threshold', 'Threshold', 'Limiter', -60, 0, -3, 0.1, 'db'),
      f('lookahead', 'Lookahead', 'Limiter', 0, 20, 5, 0.1, 'ms'),
      f('attack', 'Attack', 'Limiter', 0.1, 100, 5, 0.1, 'ms'),
      f('release', 'Release', 'Limiter', 1, 1000, 50, 1, 'ms'),
      e('oversampling', 'Oversampling', 'Limiter', 0, ['None', 'Half x2', 'Full x2', 'Half x4', 'Full x4']),
    ],
    bands: null,
    assets: [],
  },
  {
    kind: 'filter',
    name: 'Filter',
    group: 'eq',
    summary: 'Resonant low/high/band-pass, notch and shelf filter up to 48 dB/oct.',
    compat: 'exact',
    latency: 'zero',
    params: [
      e('type', 'Type', 'Filter', 0, ['Low-pass', 'High-pass', 'Band-pass', 'Notch', 'Low-shelf', 'High-shelf']),
      f('frequency', 'Frequency', 'Filter', 20, 20000, 1000, 1, 'hz', true),
      f('resonance', 'Resonance', 'Filter', 0.1, 10, 0.7, 0.01, 'none'),
      e('mode', 'Slope', 'Filter', 1, ['12 dB/oct', '24 dB/oct', '48 dB/oct']),
    ],
    bands: null,
    assets: [],
  },
  {
    kind: 'delay',
    name: 'Delay',
    group: 'space',
    summary: 'Stereo delay with feedback and ping-pong routing.',
    compat: 'exact',
    latency: 'zero',
    params: [
      f('time-l', 'Time left', 'Delay', 1, 2000, 250, 1, 'ms'),
      f('time-r', 'Time right', 'Delay', 1, 2000, 250, 1, 'ms'),
      f('feedback', 'Feedback', 'Delay', 0, 100, 30, 0.1, 'percent'),
      b('ping-pong', 'Ping-pong', 'Delay', false),
    ],
    bands: null,
    assets: [],
  },
  {
    kind: 'reverberation',
    name: 'Reverberation',
    group: 'space',
    summary: 'Feedback delay network reverb with early reflections and damping.',
    compat: 'approximate',
    latency: 'zero',
    params: [
      f('room-size', 'Room size', 'Reverb', 0, 100, 50, 0.1, 'percent'),
      f('damping', 'Damping', 'Reverb', 0, 100, 50, 0.1, 'percent'),
      f('width', 'Width', 'Reverb', 0, 100, 100, 0.1, 'percent'),
      f('pre-delay', 'Pre-delay', 'Reverb', 0, 200, 20, 0.1, 'ms'),
      f('decay-time', 'Decay time', 'Reverb', 0.1, 10, 1.5, 0.01, 'seconds'),
    ],
    bands: null,
    assets: [],
  },
  {
    kind: 'stereo_tools',
    name: 'Stereo Tools',
    group: 'space',
    summary: 'Mid/side balance, width and level control.',
    compat: 'exact',
    latency: 'zero',
    params: [
      f('balance', 'Balance', 'Stereo', -1, 1, 0, 0.01, 'none'),
      f('width', 'Width', 'Stereo', 0, 2, 1, 0.01, 'none'),
      f('mid-level', 'Mid level', 'Stereo', -24, 24, 0, 0.1, 'db'),
      f('side-level', 'Side level', 'Stereo', -24, 24, 0, 0.1, 'db'),
      b('phase', 'Invert phase', 'Stereo', false),
    ],
    bands: null,
    assets: [],
  },
  {
    kind: 'crusher',
    name: 'Crusher',
    group: 'tone',
    summary: 'Bit-depth and sample-rate reduction.',
    compat: 'exact',
    latency: 'zero',
    params: [
      i('bit-depth', 'Bit depth', 'Crusher', 2, 32, 16, 'count'),
      f('sample-rate', 'Sample rate', 'Crusher', 1000, 192000, 48000, 1, 'hz', true),
      f('mix', 'Mix', 'Crusher', 0, 100, 100, 0.1, 'percent'),
    ],
    bands: null,
    assets: [],
  },
  {
    kind: 'noise_reduction',
    name: 'Noise Reduction',
    group: 'noise',
    summary: 'STFT spectral subtraction with a tracked noise floor.',
    compat: 'substitute',
    latency: 'block',
    params: [
      f('amount', 'Amount', 'Noise', 0, 100, 50, 0.1, 'percent'),
      f('noise-floor', 'Noise floor', 'Noise', -100, -20, -60, 0.1, 'db'),
      f('release', 'Release', 'Noise', 1, 1000, 100, 1, 'ms'),
      e('model', 'Algorithm', 'Noise', 0, ['Spectral subtraction', 'Wiener']),
    ],
    bands: null,
    assets: [],
  },
  {
    kind: 'level_meter',
    name: 'Level Meter',
    group: 'meter',
    summary: 'Peak, true peak, RMS and loudness measurement.',
    compat: 'meter',
    latency: 'zero',
    params: [
      f('peak-hold', 'Peak hold', 'Meter', 0, 5000, 1000, 10, 'ms'),
      e('scale', 'Scale', 'Meter', 0, ['dBFS', 'LUFS']),
    ],
    bands: null,
    assets: [],
  },
];

const GROUPS = [
  { id: 'eq', label: 'EQ & filters' },
  { id: 'dynamics', label: 'Dynamics' },
  { id: 'space', label: 'Space & time' },
  { id: 'tone', label: 'Tone shaping' },
  { id: 'voice', label: 'Voice & pitch' },
  { id: 'noise', label: 'Noise & echo' },
  { id: 'meter', label: 'Measurement' },
];

function defaultParams(desc: EffectDescriptor): Record<string, number | boolean> {
  const out: Record<string, number | boolean> = {
    'input-gain': 0,
    'output-gain': 0,
    dry: 1,
    wet: 1,
    bypass: false,
  };
  for (const p of desc.params) out[p.key] = p.kind.default;
  return out;
}

function defaultBands(desc: EffectDescriptor): Record<string, Band[]> {
  const out: Record<string, Band[]> = {};
  if (!desc.bands) return out;
  const curve = [4.5, 3.5, 1.5, 0, -1, -1.5, -0.5, 1, 2, 1.5];
  for (const ch of desc.bands.channel_labels) {
    out[ch] = Array.from({ length: 10 }, (_, idx) => ({
      type: 0,
      frequency: 31.25 * Math.pow(2, idx),
      gain: curve[idx] ?? 0,
      q: 1.41,
      width: 4,
      slope: 0,
      solo: false,
      mute: false,
    }));
  }
  return out;
}

function makeInstance(kind: string, index: number): EffectInstance {
  const desc = EFFECTS.find((d) => d.kind === kind) ?? EFFECTS[0];
  return {
    id: `${kind}#${index}`,
    kind,
    name: desc.name,
    bypass: false,
    index,
    compat: desc.compat,
    latency_frames: 0,
    gain_reduction_db: kind === 'limiter' ? -2.4 : 0,
    params: defaultParams(desc),
    bands: desc.bands ? defaultBands(desc) : undefined,
  };
}

const state: {
  output: Pipeline;
  input: Pipeline;
  presets: Preset[];
  config: Config;
  autoload: AutoloadRule[];
  streams: Stream[];
} = {
  output: {
    kind: 'output',
    bypass: false,
    revision: 4,
    latency_frames: 0,
    effects: [makeInstance('equalizer', 0), makeInstance('limiter', 0)],
  },
  input: {
    kind: 'input',
    bypass: false,
    revision: 1,
    latency_frames: 0,
    effects: [],
  },
  presets: [
    {
      id: 'bass-boost',
      name: 'Bass Boost',
      kind: 'output',
      blocklist: [],
      plugins_order: ['equalizer#0', 'limiter#0'],
      instances: {
        'equalizer#0': { bypass: false, 'input-gain': -3, 'num-bands': 10 },
        'limiter#0': { bypass: false, threshold: -1 },
      },
      warnings: [],
      missing_assets: [],
    },
    {
      id: 'podcast-voice',
      name: 'Podcast Voice',
      kind: 'input',
      blocklist: ['Firefox'],
      plugins_order: ['noise_reduction#0', 'compressor#0', 'limiter#0'],
      instances: {
        'noise_reduction#0': { bypass: false, amount: 65 },
        'compressor#0': { bypass: false, threshold: -22, ratio: 3.5 },
        'limiter#0': { bypass: false, threshold: -1 },
      },
      warnings: [],
      missing_assets: [],
    },
    {
      id: 'wide-headphones',
      name: 'Wide Headphones',
      kind: 'output',
      blocklist: [],
      plugins_order: ['stereo_tools#0', 'reverberation#0'],
      instances: {
        'stereo_tools#0': { bypass: false, width: 1.35 },
        'reverberation#0': { bypass: false, 'room-size': 32, 'decay-time': 0.9 },
      },
      warnings: [],
      missing_assets: [],
    },
  ],
  config: {
    bind: '127.0.0.1',
    port: 8737,
    theme: 'bench',
    meter_rate_hz: 30,
    log_level: 'info',
    autoload: true,
    virtual_nodes: true,
  },
  autoload: [
    { id: 'rule-1', device: 'alsa_output.usb-Focusrite_Scarlett_2i2', route: 'analog-stereo', pipeline: 'output', preset: 'bass-boost', enabled: true },
  ],
  streams: [
    { id: 55, app: 'Firefox', name: 'Playback', media: 'audio', direction: 'output', linked: true, blocked: false, target: 'resonance-output' },
    { id: 56, app: 'Spotify', name: 'Playback', media: 'audio', direction: 'output', linked: true, blocked: false, target: 'resonance-output' },
    { id: 57, app: 'Discord', name: 'Capture', media: 'audio', direction: 'input', linked: false, blocked: false, target: 'resonance-input' },
  ],
};

function pipelineOf(kind: PipelineKind): Pipeline {
  return kind === 'input' ? state.input : state.output;
}

function listItem(p: Preset): PresetListItem {
  return {
    id: p.id,
    name: p.name,
    kind: p.kind,
    source: 'local',
    effects: p.plugins_order.length,
    missing_assets: p.missing_assets,
    updated_at: now(),
  };
}

const wait = (ms = 40) => new Promise((r) => setTimeout(r, ms));

export const mockApi = {
  async status(): Promise<Status> {
    await wait();
    return {
      daemon: { version: '0.1.0', uptime_s: 3725, pid: 4242, mode: 'pipewire' },
      pipewire: {
        available: true,
        version: '1.6.9',
        server: 'pipewire-0',
        virtual_nodes: { output: 'resonance-output', input: 'resonance-input' },
      },
      dsp: { sample_rate: 48000, quantum: 128, latency_frames: 128, xruns: 0, cpu_percent: 1.4, dsp_p99_us: 145 },
      compat: { baseline: 'EasyEffects 8.3.2', lsp_required: false, effects: 33 },
      revision: state.output.revision,
    };
  },

  async pipelines(): Promise<Pipeline[]> {
    await wait();
    return [state.output, state.input];
  },

  async pipeline(kind: PipelineKind): Promise<Pipeline> {
    await wait();
    return pipelineOf(kind);
  },

  async catalog(): Promise<Catalog> {
    await wait();
    return { groups: GROUPS, effects: EFFECTS };
  },

  async addEffect(kind: PipelineKind, effect: string, index?: number, params?: Record<string, unknown>): Promise<EffectInstance> {
    await wait();
    const p = pipelineOf(kind);
    const count = p.effects.filter((e) => e.kind === effect).length;
    const inst = makeInstance(effect, count);
    if (params) Object.assign(inst.params, params);
    const at = index ?? p.effects.length;
    p.effects.splice(at, 0, inst);
    p.effects.forEach((e, idx) => (e.index = idx));
    p.revision += 1;
    return inst;
  },

  async patchEffect(
    kind: PipelineKind,
    id: string,
    patch: { params?: Record<string, unknown>; bands?: Record<string, unknown>; bypass?: boolean },
  ): Promise<EffectInstance> {
    await wait();
    const p = pipelineOf(kind);
    const inst = p.effects.find((e) => e.id === id);
    if (!inst) throw new Error('not found');
    if (patch.params) Object.assign(inst.params, patch.params);
    if (patch.bands) Object.assign(inst.bands ?? {}, patch.bands);
    if (typeof patch.bypass === 'boolean') inst.bypass = patch.bypass;
    p.revision += 1;
    return inst;
  },

  async removeEffect(kind: PipelineKind, id: string): Promise<{ revision: number }> {
    await wait();
    const p = pipelineOf(kind);
    p.effects = p.effects.filter((e) => e.id !== id);
    p.effects.forEach((e, idx) => (e.index = idx));
    p.revision += 1;
    return { revision: p.revision };
  },

  async duplicateEffect(kind: PipelineKind, id: string): Promise<EffectInstance> {
    await wait();
    const p = pipelineOf(kind);
    const src = p.effects.find((e) => e.id === id);
    if (!src) throw new Error('not found');
    const count = p.effects.filter((e) => e.kind === src.kind).length;
    const copy: EffectInstance = {
      ...src,
      id: `${src.kind}#${count}`,
      params: { ...src.params },
      bands: src.bands ? JSON.parse(JSON.stringify(src.bands)) : undefined,
    };
    p.effects.splice(src.index + 1, 0, copy);
    p.effects.forEach((e, idx) => (e.index = idx));
    p.revision += 1;
    return copy;
  },

  async reorder(kind: PipelineKind, order: string[]): Promise<Pipeline> {
    await wait();
    const p = pipelineOf(kind);
    const map = new Map(p.effects.map((e) => [e.id, e]));
    p.effects = order.map((id) => map.get(id)).filter((e): e is EffectInstance => !!e);
    p.effects.forEach((e, idx) => (e.index = idx));
    p.revision += 1;
    return p;
  },

  async setBypass(kind: PipelineKind, bypass: boolean): Promise<Pipeline> {
    await wait();
    const p = pipelineOf(kind);
    p.bypass = bypass;
    p.revision += 1;
    return p;
  },

  async devices(): Promise<Devices> {
    await wait();
    return {
      outputs: [
        { id: 42, name: 'alsa_output.pci-0000_00_1f.3.analog-stereo', description: 'Built-in Audio', nick: 'Speakers', is_default: true, state: 'running', profile: 'analog-stereo' },
        { id: 61, name: 'alsa_output.usb-Focusrite_Scarlett_2i2', description: 'Scarlett 2i2', nick: 'Scarlett', is_default: false, state: 'suspended', profile: 'analog-stereo' },
      ],
      inputs: [
        { id: 43, name: 'alsa_input.pci-0000_00_1f.3.analog-stereo', description: 'Built-in Microphone', nick: 'Mic', is_default: true, state: 'running', profile: 'analog-stereo' },
      ],
    };
  },

  async streams(): Promise<Stream[]> {
    await wait();
    return state.streams;
  },

  async setStreamBlocked(id: number, blocked: boolean): Promise<Stream> {
    await wait();
    const stream = state.streams.find((s) => s.id === id);
    if (!stream) throw new Error('stream not found');
    stream.blocked = blocked;
    return stream;
  },

  async presets(): Promise<PresetListItem[]> {
    await wait();
    return state.presets.map(listItem);
  },

  async preset(id: string): Promise<Preset> {
    await wait();
    const p = state.presets.find((x) => x.id === id);
    if (!p) throw new Error('not found');
    return p;
  },

  async createPreset(body: {
    name: string;
    kind: PipelineKind;
    plugins_order?: string[];
    instances?: Record<string, Record<string, unknown>>;
    blocklist?: string[];
  }): Promise<Preset> {
    await wait();
    const base = body.name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-|-$/g, '') || 'preset';
    let id = base;
    let n = 2;
    while (state.presets.some((p) => p.id === id)) id = `${base}-${n++}`;
    const preset: Preset = {
      id,
      name: body.name,
      kind: body.kind,
      blocklist: body.blocklist ?? [],
      plugins_order: body.plugins_order ?? [],
      instances: body.instances ?? {},
      warnings: [],
      missing_assets: [],
    };
    state.presets.push(preset);
    return preset;
  },

  async importPreset(source: string, name?: string): Promise<{ preset: Preset; warnings: string[] }> {
    await wait(120);
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(source);
    } catch {
      throw new Error('The file is not valid JSON.');
    }
    const kind: PipelineKind = 'output' in parsed ? 'output' : 'input' in parsed ? 'input' : 'output';
    const body = (parsed[kind] ?? {}) as Record<string, unknown>;
    const order = (body.plugins_order as string[]) ?? [];
    const preset: Preset = {
      id: `imported-${Date.now()}`,
      name: name ?? `Imported ${state.presets.length + 1}`,
      kind,
      blocklist: (body.blocklist as string[]) ?? [],
      plugins_order: order,
      instances: Object.fromEntries(order.map((id) => [id, (body[id] as Record<string, unknown>) ?? {}])),
      warnings: [],
      missing_assets: [],
    };
    state.presets.push(preset);
    return { preset, warnings: [] };
  },

  async applyPreset(id: string, pipeline: PipelineKind): Promise<Pipeline> {
    await wait(80);
    const preset = state.presets.find((p) => p.id === id);
    if (!preset) throw new Error('not found');
    const p = pipelineOf(pipeline);
    p.effects = preset.plugins_order.map((instanceId, idx) => {
      const kind = instanceId.split('#')[0];
      const inst = makeInstance(kind, idx);
      inst.id = instanceId;
      const raw = preset.instances[instanceId] ?? {};
      for (const [k, v] of Object.entries(raw)) {
        if (k in inst.params) inst.params[k] = v as number | boolean;
      }
      return inst;
    });
    p.revision += 1;
    return p;
  },

  async deletePreset(id: string): Promise<{ deleted: boolean }> {
    await wait();
    state.presets = state.presets.filter((p) => p.id !== id);
    return { deleted: true };
  },

  async exportPreset(id: string): Promise<string> {
    await wait();
    const p = state.presets.find((x) => x.id === id);
    if (!p) throw new Error('not found');
    return JSON.stringify({ [p.kind]: { blocklist: p.blocklist, plugins_order: p.plugins_order, ...p.instances } }, null, 4);
  },

  async metrics(): Promise<Metrics> {
    await wait();
    return {
      cpu_percent: 1.4,
      dsp_p50_us: 82,
      dsp_p99_us: 145,
      xruns: 0,
      latency_frames: 128,
      sample_rate: 48000,
      quantum: 128,
      uptime_s: 3725,
      frames: 178_800_000,
    };
  },

  async config(): Promise<Config> {
    await wait();
    return state.config;
  },

  async patchConfig(patch: Partial<Config>): Promise<Config> {
    await wait();
    state.config = { ...state.config, ...patch };
    return state.config;
  },

  async autoload(): Promise<AutoloadRule[]> {
    await wait();
    return state.autoload;
  },

  async putAutoload(rules: AutoloadRule[]): Promise<AutoloadRule[]> {
    await wait();
    state.autoload = rules;
    return state.autoload;
  },

  async selfTest(): Promise<{ checks: SelfTestCheck[] }> {
    await wait(200);
    return {
      checks: [
        { name: 'PipeWire connection', ok: true, detail: 'pipewire-0, 1.6.9' },
        { name: 'DSP graph', ok: true, detail: '2 effects, 0 xruns' },
        { name: 'Preset directory', ok: true, detail: '~/.local/share/resonance/presets' },
        { name: 'LSP Plugins', ok: true, detail: 'not required, not loaded' },
        { name: 'Asset store', ok: true, detail: '0 IRs, 0 models' },
      ],
    };
  },
};

let tick = 0;

export function mockEvents(): ServerEvent[] {
  tick += 1;
  const t = tick / 10;
  const peak = -18 + 9 * Math.sin(t * 1.7) + 3 * Math.sin(t * 5.3);
  const bins = Array.from({ length: 96 }, (_, i) => {
    const f = 20 * Math.pow(1000, i / 96);
    const tilt = -18 - 9 * Math.log10(f / 1000);
    const wobble = 6 * Math.sin(t * 2 + i * 0.35) + 4 * Math.sin(t * 0.7 + i * 0.11);
    return Math.max(-96, Math.min(0, tilt + wobble - 12));
  });
  const inPeak = -26 + 7 * Math.sin(t * 1.3 + 1.2) + 2 * Math.sin(t * 4.1);
  const inBins = Array.from({ length: 96 }, (_, i) => {
    const f = 20 * Math.pow(1000, i / 96);
    const tilt = -26 - 7 * Math.log10(f / 1000);
    const wobble = 5 * Math.sin(t * 1.6 + i * 0.29) + 3 * Math.sin(t * 0.5 + i * 0.13);
    return Math.max(-96, Math.min(0, tilt + wobble - 10));
  });
  return [
    {
      type: 'meter',
      pipeline: 'output',
      timestamp_ms: Date.now(),
      peak_dbfs: [peak, peak - 0.4],
      true_peak_dbfs: [peak + 0.6, peak + 0.2],
      rms_dbfs: [peak - 9, peak - 9.4],
      lufs_m: peak - 5.5,
      lufs_s: peak - 6.1,
      gain_reduction_db: -Math.max(0, Math.sin(t * 0.9) * 4),
      dsp_p99_us: 140 + Math.round(10 * Math.sin(t)),
      xruns: 0,
    },
    { type: 'spectrum', pipeline: 'output', bins, sample_rate: 48000 },
    {
      type: 'meter',
      pipeline: 'input',
      timestamp_ms: Date.now(),
      peak_dbfs: [inPeak, inPeak - 0.7],
      true_peak_dbfs: [inPeak + 0.4, inPeak - 0.2],
      rms_dbfs: [inPeak - 11, inPeak - 11.6],
      lufs_m: inPeak - 7.2,
      lufs_s: inPeak - 7.8,
      gain_reduction_db: -Math.max(0, Math.sin(t * 0.7 + 2) * 2.5),
      dsp_p99_us: 120 + Math.round(8 * Math.sin(t + 1)),
      xruns: 0,
    },
    { type: 'spectrum', pipeline: 'input', bins: inBins, sample_rate: 48000 },
  ];
}
