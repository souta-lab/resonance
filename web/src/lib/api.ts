// Typed client for the Resonance control API.

import type {
  AutoloadRule,
  Catalog,
  Config,
  Devices,
  EffectInstance,
  Metrics,
  Pipeline,
  PipelineKind,
  Preset,
  PresetListItem,
  SelfTestCheck,
  Status,
  Stream,
} from './types';
import { mockApi, mockEvents } from './mock';

const BASE = '/api/v1';

export const MOCK =
  import.meta.env.VITE_MOCK === '1' ||
  (typeof location !== 'undefined' && location.search.includes('mock=1'));

export class ApiError extends Error {
  code: string;
  details: Record<string, unknown>;
  status: number;

  constructor(status: number, code: string, message: string, details: Record<string, unknown> = {}) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.details = details;
  }
}

let token: string | null = null;
let tokenExpires = 0;

async function ensureToken(): Promise<string> {
  if (token && Date.now() < tokenExpires) return token;
  const res = await fetch(`${BASE}/session`, { credentials: 'same-origin' });
  const body = await res.json().catch(() => null);
  if (!res.ok || !body?.data?.token) {
    throw new ApiError(res.status, 'unauthorized', 'Could not obtain a session token.');
  }
  token = body.data.token as string;
  tokenExpires = Date.now() + (body.data.expires_in ?? 3600) * 1000 - 30_000;
  return token;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const method = (init.method ?? 'GET').toUpperCase();
  const headers = new Headers(init.headers);
  if (method !== 'GET' && method !== 'HEAD') {
    headers.set('X-Resonance-Token', await ensureToken());
    if (init.body && !headers.has('Content-Type')) headers.set('Content-Type', 'application/json');
  }
  const res = await fetch(`${BASE}${path}`, { ...init, headers, credentials: 'same-origin' });
  const body = await res.json().catch(() => null);
  if (!res.ok) {
    const err = body?.error ?? {};
    throw new ApiError(res.status, err.code ?? 'internal', err.message ?? res.statusText, err.details ?? {});
  }
  return body?.data as T;
}

function json(body: unknown): RequestInit {
  return { method: 'POST', body: JSON.stringify(body) };
}

export const api = {
  async status(): Promise<Status> {
    return MOCK ? mockApi.status() : request<Status>('/status');
  },

  async pipelines(): Promise<Pipeline[]> {
    return MOCK ? mockApi.pipelines() : request<Pipeline[]>('/pipelines');
  },

  async pipeline(kind: PipelineKind): Promise<Pipeline> {
    return MOCK ? mockApi.pipeline(kind) : request<Pipeline>(`/pipelines/${kind}/effects`);
  },

  async catalog(): Promise<Catalog> {
    return MOCK ? mockApi.catalog() : request<Catalog>('/effects/catalog');
  },

  async addEffect(
    kind: PipelineKind,
    effect: string,
    index?: number,
    params?: Record<string, unknown>,
  ): Promise<EffectInstance> {
    return MOCK
      ? mockApi.addEffect(kind, effect, index, params)
      : request<EffectInstance>(`/pipelines/${kind}/effects`, json({ kind: effect, index, params }));
  },

  async patchEffect(
    kind: PipelineKind,
    id: string,
    revision: number,
    patch: { params?: Record<string, unknown>; bands?: Record<string, unknown>; bypass?: boolean },
  ): Promise<EffectInstance> {
    return MOCK
      ? mockApi.patchEffect(kind, id, patch)
      : request<EffectInstance>(`/pipelines/${kind}/effects/${encodeURIComponent(id)}`, {
          method: 'PATCH',
          body: JSON.stringify({ revision, ...patch }),
        });
  },

  async removeEffect(kind: PipelineKind, id: string, revision: number): Promise<{ revision: number }> {
    return MOCK
      ? mockApi.removeEffect(kind, id)
      : request<{ revision: number }>(
          `/pipelines/${kind}/effects/${encodeURIComponent(id)}?revision=${revision}`,
          { method: 'DELETE' },
        );
  },

  async duplicateEffect(kind: PipelineKind, id: string, revision: number): Promise<EffectInstance> {
    return MOCK
      ? mockApi.duplicateEffect(kind, id)
      : request<EffectInstance>(
          `/pipelines/${kind}/effects/${encodeURIComponent(id)}/duplicate`,
          json({ revision }),
        );
  },

  async reorder(kind: PipelineKind, revision: number, order: string[]): Promise<Pipeline> {
    return MOCK
      ? mockApi.reorder(kind, order)
      : request<Pipeline>(`/pipelines/${kind}/order`, {
          method: 'PUT',
          body: JSON.stringify({ revision, order }),
        });
  },

  async setBypass(kind: PipelineKind, revision: number, bypass: boolean): Promise<Pipeline> {
    return MOCK
      ? mockApi.setBypass(kind, bypass)
      : request<Pipeline>(`/pipelines/${kind}/bypass`, {
          method: 'PUT',
          body: JSON.stringify({ revision, bypass }),
        });
  },

  async devices(): Promise<Devices> {
    return MOCK ? mockApi.devices() : request<Devices>('/devices');
  },

  async streams(): Promise<Stream[]> {
    return MOCK ? mockApi.streams() : request<Stream[]>('/streams');
  },

  /**
   * Stream blocking is not part of the v1 table in docs/api-contract.md yet;
   * the daemon is expected to expose PATCH /streams/{id}. The UI degrades to a
   * local toggle with a visible error if the endpoint is absent.
   */
  async setStreamBlocked(id: number, blocked: boolean): Promise<Stream> {
    return MOCK
      ? mockApi.setStreamBlocked(id, blocked)
      : request<Stream>(`/streams/${id}`, { method: 'PATCH', body: JSON.stringify({ blocked }) });
  },

  async presets(): Promise<PresetListItem[]> {
    return MOCK ? mockApi.presets() : request<PresetListItem[]>('/presets');
  },

  async preset(id: string): Promise<Preset> {
    return MOCK ? mockApi.preset(id) : request<Preset>(`/presets/${encodeURIComponent(id)}`);
  },

  async createPreset(body: {
    name: string;
    kind: PipelineKind;
    plugins_order?: string[];
    instances?: Record<string, Record<string, unknown>>;
    blocklist?: string[];
  }): Promise<Preset> {
    return MOCK ? mockApi.createPreset(body) : request<Preset>('/presets', json(body));
  },

  async importPreset(source: string, name?: string): Promise<{ preset: Preset; warnings: string[] }> {
    return MOCK
      ? mockApi.importPreset(source, name)
      : request<{ preset: Preset; warnings: string[] }>('/presets/import', json({ json: source, name }));
  },

  async applyPreset(id: string, pipeline: PipelineKind, revision: number): Promise<Pipeline> {
    return MOCK
      ? mockApi.applyPreset(id, pipeline)
      : request<Pipeline>(`/presets/${encodeURIComponent(id)}/apply`, json({ pipeline, revision }));
  },

  async deletePreset(id: string): Promise<{ deleted: boolean }> {
    return MOCK
      ? mockApi.deletePreset(id)
      : request<{ deleted: boolean }>(`/presets/${encodeURIComponent(id)}`, { method: 'DELETE' });
  },

  async exportPreset(id: string): Promise<string> {
    if (MOCK) return mockApi.exportPreset(id);
    const res = await fetch(`${BASE}/presets/${encodeURIComponent(id)}/export`, {
      credentials: 'same-origin',
    });
    if (!res.ok) throw new ApiError(res.status, 'not_found', 'Preset could not be exported.');
    return res.text();
  },

  async metrics(): Promise<Metrics> {
    return MOCK ? mockApi.metrics() : request<Metrics>('/metrics');
  },

  async config(): Promise<Config> {
    return MOCK ? mockApi.config() : request<Config>('/config');
  },

  async patchConfig(patch: Partial<Config>): Promise<Config> {
    return MOCK
      ? mockApi.patchConfig(patch)
      : request<Config>('/config', { method: 'PATCH', body: JSON.stringify(patch) });
  },

  async autoload(): Promise<AutoloadRule[]> {
    return MOCK ? mockApi.autoload() : request<AutoloadRule[]>('/autoload');
  },

  async putAutoload(rules: AutoloadRule[]): Promise<AutoloadRule[]> {
    return MOCK
      ? mockApi.putAutoload(rules)
      : request<AutoloadRule[]>('/autoload', { method: 'PUT', body: JSON.stringify({ rules }) });
  },

  async selfTest(): Promise<{ checks: SelfTestCheck[] }> {
    return MOCK ? mockApi.selfTest() : request<{ checks: SelfTestCheck[] }>('/diagnostics/self-test', json({}));
  },
};

export interface EventHandlers {
  onMeter?: (e: import('./types').MeterEvent) => void;
  onSpectrum?: (e: import('./types').SpectrumEvent) => void;
  onGraph?: (e: import('./types').GraphEvent) => void;
  onDevice?: (e: import('./types').DeviceEvent) => void;
  onError?: (e: import('./types').ErrorEvent) => void;
  onOpen?: () => void;
  onClose?: () => void;
}

/** Subscribe to the event stream. Returns a disposer. */
export function subscribeEvents(handlers: EventHandlers): () => void {
  if (MOCK) {
    const timer = setInterval(() => {
      for (const e of mockEvents()) {
        if (e.type === 'meter') handlers.onMeter?.(e);
        else if (e.type === 'spectrum') handlers.onSpectrum?.(e);
      }
    }, 100);
    handlers.onOpen?.();
    return () => {
      clearInterval(timer);
      handlers.onClose?.();
    };
  }

  let ws: WebSocket | null = null;
  let closed = false;
  let retry = 0;
  let timer: number | undefined;

  const connect = async () => {
    if (closed) return;
    try {
      const t = await ensureToken();
      const proto = location.protocol === 'https:' ? 'wss' : 'ws';
      ws = new WebSocket(`${proto}://${location.host}${BASE}/events?token=${encodeURIComponent(t)}`);
      ws.onopen = () => {
        retry = 0;
        handlers.onOpen?.();
      };
      ws.onmessage = (msg) => {
        let e: import('./types').ServerEvent;
        try {
          e = JSON.parse(msg.data);
        } catch {
          return;
        }
        switch (e.type) {
          case 'meter':
            handlers.onMeter?.(e);
            break;
          case 'spectrum':
            handlers.onSpectrum?.(e);
            break;
          case 'graph':
            handlers.onGraph?.(e);
            break;
          case 'device':
            handlers.onDevice?.(e);
            break;
          case 'error':
            handlers.onError?.(e);
            break;
        }
      };
      ws.onclose = () => {
        handlers.onClose?.();
        if (closed) return;
        retry = Math.min(retry + 1, 6);
        timer = window.setTimeout(connect, 500 * 2 ** retry);
      };
      ws.onerror = () => ws?.close();
    } catch {
      retry = Math.min(retry + 1, 6);
      timer = window.setTimeout(connect, 500 * 2 ** retry);
    }
  };

  void connect();

  return () => {
    closed = true;
    if (timer) window.clearTimeout(timer);
    ws?.close();
  };
}
