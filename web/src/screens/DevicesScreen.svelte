<script lang="ts">
  import { onMount } from 'svelte';
  import { store, errorMessage } from '../lib/store.svelte';
  import type { Device, Stream } from '../lib/types';
  import Icon from '../components/Icon.svelte';
  import Banner from '../components/Banner.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import Switch from '../components/Switch.svelte';

  let loading = $state(true);
  let error = $state<string | null>(null);
  let busyStream = $state<number | null>(null);
  let busyRule = $state<string | null>(null);

  const devices = $derived(store.devices);
  const streams = $derived(store.streams);
  const autoload = $derived(store.autoload);

  onMount(() => {
    void load();
  });

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      await store.loadDevices();
      void store.loadPresets().catch(() => undefined);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  function stateClass(state: string): string {
    if (state === 'running') return 'led led-on';
    if (state === 'error') return 'led led-alert';
    return 'led';
  }

  function deviceLabel(name: string): string {
    const all = [...(devices?.outputs ?? []), ...(devices?.inputs ?? [])];
    const device = all.find((item) => item.name === name);
    return device?.nick || device?.description || name;
  }

  function presetName(id: string): string {
    return store.presets.find((preset) => preset.id === id)?.name ?? id;
  }

  async function toggleBlocked(stream: Stream, blocked: boolean): Promise<void> {
    busyStream = stream.id;
    await store.setStreamBlocked(stream.id, blocked);
    busyStream = null;
  }

  async function toggleRule(id: string, enabled: boolean): Promise<void> {
    busyRule = id;
    const next = autoload.map((rule) => (rule.id === id ? { ...rule, enabled } : rule));
    await store.saveAutoload(next);
    busyRule = null;
  }

  function deviceCards(list: Device[] | undefined): Device[] {
    return list ?? [];
  }
</script>

<div class="devices-screen">
  <div class="screen-toolbar">
    <span class="micro">PipeWire graph</span>
    <span class="mono toolbar-count">
      {(devices?.outputs.length ?? 0) + (devices?.inputs.length ?? 0)} devices · {streams.length}
      {streams.length === 1 ? 'stream' : 'streams'} · {autoload.length}
      {autoload.length === 1 ? 'rule' : 'rules'}
    </span>
    <span class="spacer"></span>
    <button type="button" class="btn" disabled={loading} onclick={() => void load()}>
      <Icon name="refresh" size={14} />
      Refresh
    </button>
  </div>

  {#if error}
    <Banner message={error} onretry={() => void load()} />
  {/if}

  {#if loading && !devices}
    <div class="loading-panel panel">
      <span class="micro">Loading devices…</span>
    </div>
  {:else}
    <section class="device-section" aria-label="Output devices">
      <div class="section-head">
        <span class="micro">Output devices</span>
        <span class="count mono">{deviceCards(devices?.outputs).length}</span>
        <span class="section-rule"></span>
      </div>
      {#if deviceCards(devices?.outputs).length}
        <div class="device-grid">
          {#each deviceCards(devices?.outputs) as device (device.id)}
            <article class="device-card panel">
              <header class="device-head">
                <span class="device-nick">{device.nick || device.description}</span>
                {#if device.is_default}
                  <span class="badge badge-signal">default</span>
                {/if}
                <span class="spacer"></span>
                <span class="device-state">
                  <span class={stateClass(device.state)}></span>
                  <span class="mono">{device.state}</span>
                </span>
              </header>
              <p class="device-desc">{device.description}</p>
              <div class="device-meta mono">
                <span>id {device.id}</span>
                <span class="meta-rule"></span>
                <span>{device.profile}</span>
              </div>
              <p class="device-name mono" title={device.name}>{device.name}</p>
            </article>
          {/each}
        </div>
      {:else}
        <p class="section-empty">No output devices. PipeWire reports no playback sinks.</p>
      {/if}
    </section>

    <section class="device-section" aria-label="Input devices">
      <div class="section-head">
        <span class="micro">Input devices</span>
        <span class="count mono">{deviceCards(devices?.inputs).length}</span>
        <span class="section-rule"></span>
      </div>
      {#if deviceCards(devices?.inputs).length}
        <div class="device-grid">
          {#each deviceCards(devices?.inputs) as device (device.id)}
            <article class="device-card panel">
              <header class="device-head">
                <span class="device-nick">{device.nick || device.description}</span>
                {#if device.is_default}
                  <span class="badge badge-signal">default</span>
                {/if}
                <span class="spacer"></span>
                <span class="device-state">
                  <span class={stateClass(device.state)}></span>
                  <span class="mono">{device.state}</span>
                </span>
              </header>
              <p class="device-desc">{device.description}</p>
              <div class="device-meta mono">
                <span>id {device.id}</span>
                <span class="meta-rule"></span>
                <span>{device.profile}</span>
              </div>
              <p class="device-name mono" title={device.name}>{device.name}</p>
            </article>
          {/each}
        </div>
      {:else}
        <p class="section-empty">No input devices. PipeWire reports no capture sources.</p>
      {/if}
    </section>

    <section class="device-section" aria-label="Streams">
      <div class="section-head">
        <span class="micro">Streams</span>
        <span class="count mono">{streams.length}</span>
        <span class="section-rule"></span>
      </div>
      {#if streams.length}
        <div class="table-wrap">
          <table class="data-table">
            <thead>
              <tr>
                <th scope="col">Application</th>
                <th scope="col">Direction</th>
                <th scope="col">Target</th>
                <th scope="col">Linked</th>
                <th scope="col" class="center">Blocked</th>
              </tr>
            </thead>
            <tbody>
              {#each streams as stream (stream.id)}
                <tr>
                  <td>
                    <span class="stream-app">{stream.app}</span>
                    <span class="stream-name micro">{stream.name} · {stream.media}</span>
                  </td>
                  <td><span class="badge">{stream.direction}</span></td>
                  <td class="mono target">{stream.target}</td>
                  <td>
                    <span class="linked">
                      <span class="led" class:led-on={stream.linked}></span>
                      <span class="mono">{stream.linked ? 'linked' : 'unlinked'}</span>
                    </span>
                  </td>
                  <td class="center">
                    <Switch
                      checked={stream.blocked}
                      label={`Block ${stream.app}`}
                      disabled={busyStream === stream.id}
                      onchange={(checked) => void toggleBlocked(stream, checked)}
                    />
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <EmptyState
          title="No streams are routed through Resonance."
          description="Start playback in an application, then refresh. Streams appear here once PipeWire links them."
          actionLabel="Refresh"
          onaction={() => void load()}
          icon="link"
        />
      {/if}
    </section>

    <section class="device-section" aria-label="Autoload rules">
      <div class="section-head">
        <span class="micro">Autoload rules</span>
        <span class="count mono">{autoload.length}</span>
        <span class="section-rule"></span>
      </div>
      {#if autoload.length}
        <div class="table-wrap">
          <table class="data-table">
            <thead>
              <tr>
                <th scope="col">Device</th>
                <th scope="col">Route</th>
                <th scope="col">Pipeline</th>
                <th scope="col">Preset</th>
                <th scope="col" class="center">Enabled</th>
              </tr>
            </thead>
            <tbody>
              {#each autoload as rule (rule.id)}
                <tr>
                  <td>
                    <span class="stream-app">{deviceLabel(rule.device)}</span>
                    <span class="stream-name mono">{rule.device}</span>
                  </td>
                  <td class="mono target">{rule.route}</td>
                  <td><span class="badge">{rule.pipeline}</span></td>
                  <td>{presetName(rule.preset)}</td>
                  <td class="center">
                    <Switch
                      checked={rule.enabled}
                      label={`Enable autoload rule for ${deviceLabel(rule.device)}`}
                      disabled={busyRule === rule.id}
                      onchange={(checked) => void toggleRule(rule.id, checked)}
                    />
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <EmptyState
          title="No autoload rules."
          description="A rule loads a preset when a matching device appears. Add rules in the daemon configuration."
          icon="devices"
        />
      {/if}
    </section>
  {/if}
</div>

<style>
  .devices-screen {
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
    min-height: 0;
    overflow: auto;
    padding: 12px;
    background: var(--chassis);
  }

  .screen-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    flex: none;
  }

  .toolbar-count {
    font-size: 9px;
    color: var(--ink-3);
  }

  .loading-panel {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 200px;
  }

  .device-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .section-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .count {
    font-size: 9px;
    color: var(--ink-3);
  }

  .section-rule {
    flex: 1;
    height: 1px;
    background: var(--rule);
  }

  .device-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 8px;
  }

  .device-card {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 10px 12px;
  }

  .device-head {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .device-nick {
    font-size: 13px;
    font-weight: 500;
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .device-state {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 9px;
    color: var(--ink-3);
    flex: none;
  }

  .device-desc {
    margin: 0;
    font-size: 11px;
    color: var(--ink-2);
  }

  .device-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 9px;
    color: var(--ink-3);
  }

  .meta-rule {
    width: 1px;
    height: 9px;
    background: var(--rule);
  }

  .device-name {
    margin: 0;
    font-size: 9px;
    color: var(--ink-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .section-empty {
    margin: 0;
    padding: 10px 12px;
    background: var(--panel);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    font-size: 12px;
    color: var(--ink-2);
  }

  .table-wrap {
    border: 1px solid var(--rule);
    border-radius: var(--r);
    overflow: auto;
    background: var(--panel);
    box-shadow: var(--shadow-inset);
  }

  .data-table {
    width: 100%;
    border-collapse: collapse;
    min-width: 640px;
  }

  thead th {
    height: 24px;
    padding: 0 10px;
    background: var(--panel-sunken);
    border-bottom: 1px solid var(--rule-strong);
    font-family: var(--font-mono);
    font-size: 9px;
    font-weight: 500;
    letter-spacing: 0.04em;
    color: var(--ink-3);
    text-align: left;
    text-transform: uppercase;
    white-space: nowrap;
  }

  thead th.center {
    text-align: center;
  }

  tbody td {
    height: 38px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--rule);
    font-size: 12px;
    vertical-align: middle;
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  tbody tr:hover {
    background: color-mix(in srgb, var(--panel-sunken) 50%, transparent);
  }

  .stream-app {
    display: block;
    font-size: 12px;
    font-weight: 500;
    color: var(--ink);
  }

  .stream-name {
    display: block;
    font-size: 9px;
    color: var(--ink-3);
    margin-top: 1px;
  }

  .target {
    font-size: 10px;
    color: var(--ink-2);
  }

  .linked {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    color: var(--ink-2);
  }

  .center {
    text-align: center;
  }

  @media (max-width: 899px) {
    .devices-screen {
      height: auto;
      min-height: 100%;
      overflow: visible;
      padding: 8px;
    }
  }
</style>
