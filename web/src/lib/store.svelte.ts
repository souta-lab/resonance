// Shared reactive state for the Resonance control surface.
// One store instance, imported by every screen and component.

import { api, ApiError, MOCK, subscribeEvents } from './api';
import type {
  AutoloadRule,
  Band,
  Catalog,
  Config,
  Devices,
  EffectDescriptor,
  EffectInstance,
  MeterEvent,
  Metrics,
  Pipeline,
  PipelineKind,
  Preset,
  PresetListItem,
  SelfTestCheck,
  Status,
  Stream,
} from './types';

export type Screen = 'output' | 'input' | 'presets' | 'devices' | 'analyzer' | 'settings';
export type Theme = 'bench' | 'scope';
export type ToastKind = 'info' | 'ok' | 'error';

export interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
}

export interface MeterState {
  peak: [number, number];
  truePeak: [number, number];
  rms: [number, number];
  lufsM: number;
  lufsS: number;
  gr: number;
  updated: number;
}

export interface HistoryState {
  peak: number[];
  lufs: number[];
}

interface PendingPatch {
  kind: PipelineKind;
  id: string;
  params: Record<string, number | boolean>;
  bands: Record<string, Band[]>;
  timer: number | null;
  inFlight: boolean;
}

const SCREENS: Screen[] = ['output', 'input', 'presets', 'devices', 'analyzer', 'settings'];
const HISTORY_MAX = 240;

function emptyMeter(): MeterState {
  return {
    peak: [-60, -60],
    truePeak: [-60, -60],
    rms: [-60, -60],
    lufsM: -60,
    lufsS: -60,
    gr: 0,
    updated: 0,
  };
}

function emptyHistory(): HistoryState {
  return { peak: [], lufs: [] };
}

export function errorMessage(e: unknown): string {
  if (e instanceof ApiError) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}

export function pipelineLabel(kind: PipelineKind): string {
  return kind === 'output' ? 'Output' : 'Input';
}

class Store {
  status = $state<Status | null>(null);
  pipelines = $state<Record<PipelineKind, Pipeline | null>>({ output: null, input: null });
  catalog = $state<Catalog | null>(null);

  selectedPipeline = $state<PipelineKind>('output');
  selectedEffectId = $state<string | null>(null);
  analyzerPipeline = $state<PipelineKind>('output');

  meters = $state<Record<PipelineKind, MeterState>>({ output: emptyMeter(), input: emptyMeter() });
  spectrum = $state<Record<PipelineKind, number[]>>({ output: [], input: [] });
  history = $state<Record<PipelineKind, HistoryState>>({ output: emptyHistory(), input: emptyHistory() });

  toasts = $state<ToastItem[]>([]);
  loading = $state(true);
  error = $state<{ message: string; code?: string } | null>(null);
  connected = $state(false);

  config = $state<Config | null>(null);
  devices = $state<Devices | null>(null);
  streams = $state<Stream[]>([]);
  presets = $state<PresetListItem[]>([]);
  autoload = $state<AutoloadRule[]>([]);
  metrics = $state<Metrics | null>(null);

  selfTestResults = $state<SelfTestCheck[] | null>(null);
  selfTestRunning = $state(false);

  theme = $state<Theme>('bench');
  route = $state<Screen>('output');
  addOpen = $state(false);
  drawerOpen = $state(true);

  private themeStored = false;
  private toastSeq = 0;
  private toastTimers = new Map<number, number>();
  private pending = new Map<string, PendingPatch>();
  private chain: Promise<unknown> = Promise.resolve();
  private routerCleanup: (() => void) | null = null;

  // -------------------------------------------------------------------------
  // Derived reads
  // -------------------------------------------------------------------------

  get pipeline(): Pipeline | null {
    return this.pipelines[this.selectedPipeline];
  }

  get selectedEffect(): EffectInstance | null {
    const p = this.pipeline;
    if (!p || !this.selectedEffectId) return null;
    return p.effects.find((e) => e.id === this.selectedEffectId) ?? null;
  }

  get selectedDescriptor(): EffectDescriptor | null {
    const e = this.selectedEffect;
    if (!e || !this.catalog) return null;
    return this.catalog.effects.find((d) => d.kind === e.kind) ?? null;
  }

  get selectedIndex(): number {
    const p = this.pipeline;
    if (!p || !this.selectedEffectId) return -1;
    return p.effects.findIndex((e) => e.id === this.selectedEffectId);
  }

  get eqEffects(): EffectInstance[] {
    return this.pipeline?.effects.filter((e) => !!e.bands) ?? [];
  }

  get selectedEq(): EffectInstance | null {
    const selected = this.selectedEffect;
    if (selected?.bands) return selected;
    return this.eqEffects[0] ?? null;
  }

  get selectedEqDescriptor(): EffectDescriptor | null {
    const eq = this.selectedEq;
    if (!eq || !this.catalog) return null;
    return this.catalog.effects.find((d) => d.kind === eq.kind) ?? null;
  }

  get analyzerEq(): EffectInstance | null {
    const p = this.pipelines[this.analyzerPipeline];
    if (!p) return null;
    const selected = p.effects.find((e) => e.id === this.selectedEffectId && e.bands);
    return selected ?? p.effects.find((e) => e.bands) ?? null;
  }

  get analyzerEqDescriptor(): EffectDescriptor | null {
    const eq = this.analyzerEq;
    if (!eq || !this.catalog) return null;
    return this.catalog.effects.find((d) => d.kind === eq.kind) ?? null;
  }

  get chainLatency(): number {
    return this.pipeline?.latency_frames ?? 0;
  }

  // -------------------------------------------------------------------------
  // Boot / refresh
  // -------------------------------------------------------------------------

  async refreshAll(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      const [status, pipelines, catalog] = await Promise.all([
        api.status(),
        api.pipelines(),
        api.catalog(),
      ]);
      this.status = status;
      this.setPipelines(pipelines);
      this.catalog = catalog;
      try {
        const cfg = await api.config();
        this.config = cfg;
        if (!this.themeStored) this.setTheme(cfg.theme, false);
      } catch {
        // Config is optional for the rack; Settings will retry.
      }
    } catch (e) {
      this.error = {
        message: errorMessage(e),
        code: e instanceof ApiError ? e.code : undefined,
      };
    } finally {
      this.loading = false;
    }
  }

  async refreshStatus(): Promise<void> {
    this.status = await api.status();
  }

  async refreshPipelines(): Promise<void> {
    const list = await api.pipelines();
    this.setPipelines(list);
  }

  private async safeRefreshPipelines(): Promise<void> {
    try {
      await this.refreshPipelines();
    } catch {
      // The original error is already surfaced; a failed refresh must not mask it.
    }
  }

  private setPipelines(list: Pipeline[]): void {
    for (const p of list) this.pipelines[p.kind] = p;
    const p = this.pipeline;
    if (!p) {
      this.selectedEffectId = null;
      return;
    }
    if (!p.effects.some((e) => e.id === this.selectedEffectId)) {
      this.selectedEffectId = p.effects[0]?.id ?? null;
    }
  }

  async loadConfig(): Promise<Config> {
    const cfg = await api.config();
    this.config = cfg;
    if (!this.themeStored) this.setTheme(cfg.theme, false);
    return cfg;
  }

  async loadPresets(): Promise<PresetListItem[]> {
    this.presets = await api.presets();
    return this.presets;
  }

  async loadPreset(id: string): Promise<Preset> {
    return api.preset(id);
  }

  async loadDevices(): Promise<void> {
    const [devices, streams, autoload] = await Promise.all([
      api.devices(),
      api.streams(),
      api.autoload(),
    ]);
    this.devices = devices;
    this.streams = streams;
    this.autoload = autoload;
  }

  async loadMetrics(): Promise<Metrics> {
    this.metrics = await api.metrics();
    return this.metrics;
  }

  // -------------------------------------------------------------------------
  // Selection
  // -------------------------------------------------------------------------

  selectPipeline(kind: PipelineKind): void {
    this.selectedPipeline = kind;
    const p = this.pipelines[kind];
    if (!p) {
      this.selectedEffectId = null;
      return;
    }
    if (!p.effects.some((e) => e.id === this.selectedEffectId)) {
      this.selectedEffectId = p.effects[0]?.id ?? null;
    }
  }

  selectEffect(id: string, kind?: PipelineKind): void {
    if (kind && kind !== this.selectedPipeline) this.selectedPipeline = kind;
    this.selectedEffectId = id;
  }

  // -------------------------------------------------------------------------
  // Mutations
  // -------------------------------------------------------------------------

  async addEffect(kind: PipelineKind, effectKind: string, index?: number): Promise<void> {
    const desc = this.catalog?.effects.find((d) => d.kind === effectKind);
    try {
      const created = await this.run(() => api.addEffect(kind, effectKind, index));
      await this.refreshPipelines();
      this.selectEffect(created.id, kind);
      this.toast(`Added ${desc?.name ?? effectKind}.`, 'ok');
    } catch (e) {
      this.toast(`Could not add effect: ${errorMessage(e)}`, 'error');
    }
  }

  async removeEffect(kind: PipelineKind, id: string): Promise<void> {
    const p = this.pipelines[kind];
    if (!p) return;
    const index = p.effects.findIndex((e) => e.id === id);
    const name = p.effects[index]?.name ?? 'effect';
    try {
      await this.run(() => api.removeEffect(kind, id, this.pipelines[kind]?.revision ?? 0));
      await this.refreshPipelines();
      const next = this.pipelines[kind];
      if (next) {
        this.selectedEffectId = next.effects[Math.min(index, next.effects.length - 1)]?.id ?? null;
      }
      this.toast(`Removed ${name}.`, 'info');
    } catch (e) {
      this.toast(`Could not remove ${name}: ${errorMessage(e)}`, 'error');
      await this.safeRefreshPipelines();
    }
  }

  async duplicateEffect(kind: PipelineKind, id: string): Promise<void> {
    try {
      const copy = await this.run(() =>
        api.duplicateEffect(kind, id, this.pipelines[kind]?.revision ?? 0),
      );
      await this.refreshPipelines();
      this.selectEffect(copy.id, kind);
      this.toast(`Duplicated ${copy.name}.`, 'ok');
    } catch (e) {
      this.toast(`Could not duplicate effect: ${errorMessage(e)}`, 'error');
      await this.safeRefreshPipelines();
    }
  }

  async reorder(kind: PipelineKind, from: number, to: number): Promise<void> {
    const p = this.pipelines[kind];
    if (!p) return;
    const len = p.effects.length;
    if (from < 0 || from >= len) return;
    const target = Math.max(0, Math.min(to, len - 1));
    if (target === from) return;
    const arr = [...p.effects];
    const [moved] = arr.splice(from, 1);
    arr.splice(target, 0, moved);
    p.effects = arr.map((e, i) => ({ ...e, index: i }));
    try {
      const updated = await this.run(() =>
        api.reorder(kind, this.pipelines[kind]?.revision ?? 0, arr.map((e) => e.id)),
      );
      this.pipelines[kind] = updated;
      this.toast(`Moved ${moved.name} to slot ${target + 1}.`, 'info');
    } catch (e) {
      this.toast(`Could not reorder chain: ${errorMessage(e)}`, 'error');
      await this.safeRefreshPipelines();
    }
  }

  async setBypass(kind: PipelineKind, bypass: boolean): Promise<void> {
    const p = this.pipelines[kind];
    if (!p) return;
    const prev = p.bypass;
    p.bypass = bypass;
    try {
      const updated = await this.run(() =>
        api.setBypass(kind, this.pipelines[kind]?.revision ?? 0, bypass),
      );
      this.pipelines[kind] = updated;
      this.toast(`${pipelineLabel(kind)} chain ${bypass ? 'bypassed' : 'active'}.`, 'info');
    } catch (e) {
      p.bypass = prev;
      this.toast(`Could not change chain bypass: ${errorMessage(e)}`, 'error');
    }
  }

  async setEffectBypass(kind: PipelineKind, id: string, bypass: boolean): Promise<void> {
    const eff = this.findEffect(kind, id);
    if (!eff) return;
    eff.bypass = bypass;
    try {
      await this.run(async () => {
        const rev = this.pipelines[kind]?.revision ?? 0;
        await api.patchEffect(kind, id, rev, { bypass });
        if (this.pipelines[kind]) this.pipelines[kind]!.revision = rev + 1;
      });
    } catch (e) {
      eff.bypass = !bypass;
      this.toast(`Could not change bypass: ${errorMessage(e)}`, 'error');
      await this.safeRefreshPipelines();
    }
  }

  patchParam(kind: PipelineKind, id: string, key: string, value: number | boolean): void {
    const eff = this.findEffect(kind, id);
    if (!eff) return;
    eff.params[key] = value;
    this.schedule(kind, id, { params: { [key]: value } });
  }

  patchBands(kind: PipelineKind, id: string, channel: string, bands: Band[]): void {
    const eff = this.findEffect(kind, id);
    if (!eff) return;
    if (!eff.bands) eff.bands = {};
    eff.bands[channel] = bands;
    this.schedule(kind, id, { bands: { [channel]: bands } });
  }

  private findEffect(kind: PipelineKind, id: string): EffectInstance | null {
    return this.pipelines[kind]?.effects.find((e) => e.id === id) ?? null;
  }

  private effectName(kind: PipelineKind, id: string): string {
    return this.findEffect(kind, id)?.name ?? 'effect';
  }

  private schedule(
    kind: PipelineKind,
    id: string,
    patch: { params?: Record<string, number | boolean>; bands?: Record<string, Band[]> },
  ): void {
    const key = `${kind}:${id}`;
    let p = this.pending.get(key);
    if (!p) {
      p = { kind, id, params: {}, bands: {}, timer: null, inFlight: false };
      this.pending.set(key, p);
    }
    if (patch.params) Object.assign(p.params, patch.params);
    if (patch.bands) Object.assign(p.bands, patch.bands);
    if (p.timer) window.clearTimeout(p.timer);
    p.timer = window.setTimeout(() => {
      if (p) p.timer = null;
      void this.flush(key);
    }, 60);
  }

  private async flush(key: string): Promise<void> {
    const p = this.pending.get(key);
    if (!p || p.inFlight) return;
    const params = p.params;
    const bands = p.bands;
    if (!Object.keys(params).length && !Object.keys(bands).length) {
      this.pending.delete(key);
      return;
    }
    p.params = {};
    p.bands = {};
    p.inFlight = true;
    try {
      await this.run(() => this.patchWithRetry(p.kind, p.id, { params, bands }));
    } catch (e) {
      this.toast(`Could not save ${this.effectName(p.kind, p.id)}: ${errorMessage(e)}`, 'error');
      await this.safeRefreshPipelines();
    } finally {
      p.inFlight = false;
      if (Object.keys(p.params).length || Object.keys(p.bands).length) {
        p.timer = window.setTimeout(() => {
          if (p) p.timer = null;
          void this.flush(key);
        }, 60);
      } else {
        this.pending.delete(key);
      }
    }
  }

  private async patchWithRetry(
    kind: PipelineKind,
    id: string,
    patch: { params?: Record<string, number | boolean>; bands?: Record<string, Band[]> },
  ): Promise<void> {
    const rev = this.pipelines[kind]?.revision ?? 0;
    try {
      await api.patchEffect(kind, id, rev, patch);
      if (this.pipelines[kind]) this.pipelines[kind]!.revision = rev + 1;
    } catch (e) {
      if (e instanceof ApiError && e.code === 'revision_conflict') {
        await this.refreshPipelines();
        const rev2 = this.pipelines[kind]?.revision ?? 0;
        await api.patchEffect(kind, id, rev2, patch);
        if (this.pipelines[kind]) this.pipelines[kind]!.revision = rev2 + 1;
      } else {
        throw e;
      }
    }
  }

  private run<T>(fn: () => Promise<T>): Promise<T> {
    const next = this.chain.then(fn, fn);
    this.chain = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  }

  // -------------------------------------------------------------------------
  // Presets
  // -------------------------------------------------------------------------

  async applyPreset(id: string, kind: PipelineKind): Promise<boolean> {
    try {
      const updated = await this.run(() =>
        api.applyPreset(id, kind, this.pipelines[kind]?.revision ?? 0),
      );
      this.pipelines[kind] = updated;
      this.selectedPipeline = kind;
      this.selectedEffectId = updated.effects[0]?.id ?? null;
      this.toast(`Applied preset to ${pipelineLabel(kind)}.`, 'ok');
      return true;
    } catch (e) {
      this.toast(`Could not apply preset: ${errorMessage(e)}`, 'error');
      return false;
    }
  }

  async importPreset(source: string, name?: string): Promise<{ preset: Preset; warnings: string[] }> {
    const result = await api.importPreset(source, name);
    await this.loadPresets();
    this.toast(`Imported ${result.preset.name}.`, 'ok');
    return result;
  }

  async exportPreset(id: string): Promise<boolean> {
    try {
      const text = await api.exportPreset(id);
      const blob = new Blob([text], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${id}.json`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
      this.toast(`Exported ${id}.json.`, 'ok');
      return true;
    } catch (e) {
      this.toast(`Could not export preset: ${errorMessage(e)}`, 'error');
      return false;
    }
  }

  async duplicatePreset(preset: Preset): Promise<Preset | null> {
    try {
      const copy = await api.createPreset({
        name: `${preset.name} copy`,
        kind: preset.kind,
        plugins_order: [...preset.plugins_order],
        instances: JSON.parse(JSON.stringify(preset.instances)) as Record<string, Record<string, unknown>>,
        blocklist: [...preset.blocklist],
      });
      await this.loadPresets();
      this.toast(`Duplicated ${preset.name}.`, 'ok');
      return copy;
    } catch (e) {
      this.toast(`Could not duplicate preset: ${errorMessage(e)}`, 'error');
      return null;
    }
  }

  async deletePreset(id: string): Promise<boolean> {
    try {
      await api.deletePreset(id);
      await this.loadPresets();
      this.toast('Preset deleted.', 'info');
      return true;
    } catch (e) {
      this.toast(`Could not delete preset: ${errorMessage(e)}`, 'error');
      return false;
    }
  }

  // -------------------------------------------------------------------------
  // Devices, streams, autoload
  // -------------------------------------------------------------------------

  async setStreamBlocked(id: number, blocked: boolean): Promise<void> {
    const stream = this.streams.find((s) => s.id === id);
    if (!stream) return;
    const prev = stream.blocked;
    stream.blocked = blocked;
    try {
      const updated = await api.setStreamBlocked(id, blocked);
      Object.assign(stream, updated);
    } catch (e) {
      stream.blocked = prev;
      this.toast(
        `Could not ${blocked ? 'block' : 'unblock'} ${stream.app}: ${errorMessage(e)}`,
        'error',
      );
    }
  }

  async saveAutoload(rules: AutoloadRule[]): Promise<void> {
    const prev = this.autoload;
    this.autoload = rules;
    try {
      this.autoload = await api.putAutoload(rules);
      this.toast('Autoload rules saved.', 'ok');
    } catch (e) {
      this.autoload = prev;
      this.toast(`Could not save autoload rules: ${errorMessage(e)}`, 'error');
    }
  }

  // -------------------------------------------------------------------------
  // Settings / diagnostics
  // -------------------------------------------------------------------------

  async saveConfig(patch: Partial<Config>): Promise<boolean> {
    try {
      const cfg = await api.patchConfig(patch);
      this.config = cfg;
      this.toast('Settings saved.', 'ok');
      return true;
    } catch (e) {
      this.toast(`Could not save settings: ${errorMessage(e)}`, 'error');
      return false;
    }
  }

  async runSelfTest(): Promise<void> {
    this.selfTestRunning = true;
    try {
      const res = await api.selfTest();
      this.selfTestResults = res.checks;
      this.toast('Self-test complete.', 'ok');
    } catch (e) {
      this.toast(`Self-test failed: ${errorMessage(e)}`, 'error');
    } finally {
      this.selfTestRunning = false;
    }
  }

  diagnosticsText(): string {
    const s = this.status;
    const c = this.config;
    const m = this.metrics;
    const lines: string[] = [];
    lines.push('Resonance control surface — diagnostics');
    lines.push(`Generated: ${new Date().toISOString()}`);
    lines.push(`Mode: ${MOCK ? 'mock' : 'daemon'}`);
    if (s) {
      lines.push(
        `Daemon: ${s.daemon.version} pid ${s.daemon.pid} mode ${s.daemon.mode} uptime ${s.daemon.uptime_s}s`,
      );
      lines.push(
        `PipeWire: ${s.pipewire.available ? 'available' : 'offline'} ${s.pipewire.version} server ${s.pipewire.server}`,
      );
      lines.push(
        `DSP: ${s.dsp.sample_rate} Hz quantum ${s.dsp.quantum} latency ${s.dsp.latency_frames} frames xruns ${s.dsp.xruns} cpu ${s.dsp.cpu_percent}% p99 ${s.dsp.dsp_p99_us} us`,
      );
      lines.push(
        `Compat: ${s.compat.baseline} lsp_required ${s.compat.lsp_required} effects ${s.compat.effects}`,
      );
      lines.push(
        `Virtual nodes: ${s.pipewire.virtual_nodes.output} / ${s.pipewire.virtual_nodes.input}`,
      );
    } else {
      lines.push('Daemon: unreachable');
    }
    if (c) {
      lines.push(
        `Config: bind ${c.bind}:${c.port} theme ${c.theme} meter ${c.meter_rate_hz} Hz log ${c.log_level} autoload ${c.autoload} virtual_nodes ${c.virtual_nodes}`,
      );
    }
    for (const kind of ['output', 'input'] as PipelineKind[]) {
      const p = this.pipelines[kind];
      lines.push(
        `Pipeline ${kind}: revision ${p?.revision ?? '—'} effects ${p?.effects.length ?? 0} bypass ${p?.bypass ?? '—'}`,
      );
    }
    if (m) {
      lines.push(
        `Metrics: cpu ${m.cpu_percent}% p50 ${m.dsp_p50_us} us p99 ${m.dsp_p99_us} us xruns ${m.xruns} frames ${m.frames}`,
      );
    }
    if (this.selfTestResults) {
      lines.push('Self-test:');
      for (const check of this.selfTestResults) {
        lines.push(`  ${check.ok ? 'ok' : 'FAIL'} ${check.name}: ${check.detail}`);
      }
    }
    return lines.join('\n');
  }

  async copyDiagnostics(): Promise<void> {
    const text = this.diagnosticsText();
    try {
      await navigator.clipboard.writeText(text);
      this.toast('Diagnostics copied.', 'ok');
    } catch {
      const area = document.createElement('textarea');
      area.value = text;
      area.style.position = 'fixed';
      area.style.opacity = '0';
      document.body.appendChild(area);
      area.select();
      try {
        document.execCommand('copy');
        this.toast('Diagnostics copied.', 'ok');
      } catch {
        this.toast('Could not copy diagnostics. Select the text manually.', 'error');
      }
      area.remove();
    }
  }

  // -------------------------------------------------------------------------
  // Theme / routing
  // -------------------------------------------------------------------------

  initTheme(): void {
    let stored: string | null = null;
    try {
      stored = localStorage.getItem('resonance.theme');
    } catch {
      stored = null;
    }
    const fromUrl = new URLSearchParams(location.search).get('theme');
    if (fromUrl === 'bench' || fromUrl === 'scope') {
      // A URL override wins over the daemon config for this session.
      this.themeStored = true;
      this.setTheme(fromUrl, false);
      return;
    }
    this.themeStored = stored === 'bench' || stored === 'scope';
    this.setTheme(this.themeStored ? (stored as Theme) : 'bench', false);
  }

  setTheme(theme: Theme, persist = true): void {
    this.theme = theme;
    if (typeof document !== 'undefined') document.documentElement.dataset.theme = theme;
    if (persist) {
      try {
        localStorage.setItem('resonance.theme', theme);
      } catch {
        // Private mode: the theme still applies for this session.
      }
      this.themeStored = true;
    }
  }

  initRouter(): void {
    const apply = () => this.applyRoute(this.parseHash());
    window.addEventListener('hashchange', apply);
    apply();
    this.routerCleanup = () => window.removeEventListener('hashchange', apply);
  }

  parseHash(): Screen {
    const raw = location.hash.replace(/^#\/?/, '').split('?')[0];
    return (SCREENS as string[]).includes(raw) ? (raw as Screen) : 'output';
  }

  applyRoute(screen: Screen): void {
    this.route = screen;
    if (screen === 'output' || screen === 'input') this.selectPipeline(screen);
  }

  navigate(screen: Screen): void {
    if (location.hash === `#/${screen}`) this.applyRoute(screen);
    else location.hash = `#/${screen}`;
  }

  // -------------------------------------------------------------------------
  // Events
  // -------------------------------------------------------------------------

  subscribe(): () => void {
    return subscribeEvents({
      onMeter: (e) => this.onMeter(e),
      onSpectrum: (e) => {
        this.spectrum[e.pipeline] = e.bins;
      },
      onGraph: () => {
        void this.safeRefreshPipelines();
      },
      onDevice: () => {
        void this.loadDevices().catch(() => undefined);
      },
      onError: (e) => this.toast(e.message, 'error'),
      onOpen: () => {
        this.connected = true;
      },
      onClose: () => {
        this.connected = false;
      },
    });
  }

  private onMeter(e: MeterEvent): void {
    this.meters[e.pipeline] = {
      peak: [e.peak_dbfs[0], e.peak_dbfs[1]],
      truePeak: [e.true_peak_dbfs[0], e.true_peak_dbfs[1]],
      rms: [e.rms_dbfs[0], e.rms_dbfs[1]],
      lufsM: e.lufs_m,
      lufsS: e.lufs_s,
      gr: e.gain_reduction_db,
      updated: e.timestamp_ms,
    };
    const h = this.history[e.pipeline];
    h.peak = [...h.peak.slice(-(HISTORY_MAX - 1)), e.peak_dbfs[0]];
    h.lufs = [...h.lufs.slice(-(HISTORY_MAX - 1)), e.lufs_m];
  }

  // -------------------------------------------------------------------------
  // Toasts
  // -------------------------------------------------------------------------

  toast(message: string, kind: ToastKind = 'info'): void {
    const id = ++this.toastSeq;
    this.toasts = [...this.toasts, { id, kind, message }];
    const timer = window.setTimeout(() => this.dismissToast(id), 5000);
    this.toastTimers.set(id, timer);
  }

  dismissToast(id: number): void {
    const timer = this.toastTimers.get(id);
    if (timer) window.clearTimeout(timer);
    this.toastTimers.delete(id);
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  dispose(): void {
    this.routerCleanup?.();
    this.routerCleanup = null;
    for (const timer of this.toastTimers.values()) window.clearTimeout(timer);
    this.toastTimers.clear();
    for (const p of this.pending.values()) {
      if (p.timer) window.clearTimeout(p.timer);
    }
    this.pending.clear();
  }
}

export const store = new Store();
