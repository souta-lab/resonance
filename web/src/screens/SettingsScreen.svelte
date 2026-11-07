<script lang="ts">
  import { onMount } from 'svelte';
  import { store, errorMessage } from '../lib/store.svelte';
  import type { Theme } from '../lib/store.svelte';
  import type { Config } from '../lib/types';
  import { formatUptime } from '../lib/format';
  import Icon from '../components/Icon.svelte';
  import Banner from '../components/Banner.svelte';
  import Segmented from '../components/Segmented.svelte';
  import Switch from '../components/Switch.svelte';

  let loading = $state(true);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);

  let bind = $state('127.0.0.1');
  let port = $state('8737');
  let meterRate = $state('30');
  let logLevel = $state('info');
  let autoload = $state(true);
  let virtualNodes = $state(true);
  let theme = $state<Theme>('bench');

  const status = $derived(store.status);
  const metrics = $derived(store.metrics);
  const checks = $derived(store.selfTestResults);

  onMount(() => {
    void load();
  });

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      const config = await store.loadConfig();
      syncForm(config);
      await store.loadMetrics();
      if (!store.status) await store.refreshStatus();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  function syncForm(config: Config): void {
    bind = config.bind;
    port = String(config.port);
    meterRate = String(config.meter_rate_hz);
    logLevel = config.log_level;
    autoload = config.autoload;
    virtualNodes = config.virtual_nodes;
    theme = store.theme;
  }

  function setTheme(value: string | number): void {
    theme = value as Theme;
    store.setTheme(theme);
  }

  async function save(): Promise<void> {
    saveError = null;
    if (!bind.trim()) {
      saveError = 'Bind address is required.';
      return;
    }
    const portNumber = Number(port);
    if (!Number.isInteger(portNumber) || portNumber < 1 || portNumber > 65535) {
      saveError = 'Port must be an integer between 1 and 65535.';
      return;
    }
    saving = true;
    const ok = await store.saveConfig({
      bind: bind.trim(),
      port: portNumber,
      theme,
      meter_rate_hz: Number(meterRate),
      log_level: logLevel,
      autoload,
      virtual_nodes: virtualNodes,
    });
    if (!ok) saveError = 'The daemon rejected the configuration change.';
    saving = false;
  }

  function formatCount(value: number): string {
    return value.toLocaleString('en-US');
  }
</script>

<div class="settings-screen">
  <div class="screen-toolbar">
    <span class="micro">Daemon configuration</span>
    <span class="mono toolbar-meta">
      {status ? `${status.daemon.version} · pid ${status.daemon.pid}` : 'offline'}
    </span>
    <span class="spacer"></span>
    <button type="button" class="btn" disabled={loading} onclick={() => void load()}>
      <Icon name="refresh" size={14} />
      Reload
    </button>
  </div>

  {#if error}
    <Banner message={error} onretry={() => void load()} />
  {/if}

  {#if loading && !status}
    <div class="loading-panel panel">
      <span class="micro">Loading settings…</span>
    </div>
  {:else}
    <div class="settings-grid">
      <section class="settings-panel panel" aria-label="Daemon">
        <header class="panel-head"><span class="micro">Daemon</span><span class="head-rule"></span></header>
        <dl class="defs">
          <dt>Version</dt>
          <dd class="mono">{status?.daemon.version ?? '—'}</dd>
          <dt>PID</dt>
          <dd class="mono">{status?.daemon.pid ?? '—'}</dd>
          <dt>Uptime</dt>
          <dd class="mono">{status ? formatUptime(status.daemon.uptime_s) : '—'}</dd>
          <dt>Mode</dt>
          <dd class="mono">{status?.daemon.mode ?? '—'}</dd>
          <dt>Revision</dt>
          <dd class="mono">{status?.revision ?? '—'}</dd>
          <dt>Compat</dt>
          <dd class="mono">{status?.compat.baseline ?? '—'}</dd>
          <dt>Effects</dt>
          <dd class="mono">{status?.compat.effects ?? '—'}</dd>
          <dt>LSP required</dt>
          <dd class="mono">{status ? (status.compat.lsp_required ? 'yes' : 'no') : '—'}</dd>
        </dl>
      </section>

      <section class="settings-panel panel" aria-label="PipeWire">
        <header class="panel-head"><span class="micro">PipeWire</span><span class="head-rule"></span></header>
        <dl class="defs">
          <dt>State</dt>
          <dd class="state-value">
            <span class="led" class:led-on={status?.pipewire.available} class:led-alert={!!status && !status.pipewire.available}></span>
            <span class="mono">{status?.pipewire.available ? 'available' : 'offline'}</span>
          </dd>
          <dt>Version</dt>
          <dd class="mono">{status?.pipewire.version ?? '—'}</dd>
          <dt>Server</dt>
          <dd class="mono">{status?.pipewire.server ?? '—'}</dd>
          <dt>Output node</dt>
          <dd class="mono">{status?.pipewire.virtual_nodes.output ?? '—'}</dd>
          <dt>Input node</dt>
          <dd class="mono">{status?.pipewire.virtual_nodes.input ?? '—'}</dd>
        </dl>
      </section>

      <section class="settings-panel panel" aria-label="Network">
        <header class="panel-head"><span class="micro">Network</span><span class="head-rule"></span></header>
        <div class="form-body">
          <div class="form-row">
            <label class="form-label" for="bind">Bind address</label>
            <input id="bind" class="input" type="text" bind:value={bind} spellcheck="false" />
          </div>
          <div class="form-row">
            <label class="form-label" for="port">Port</label>
            <input id="port" class="input port" type="number" min="1" max="65535" bind:value={port} />
          </div>
          <p class="form-note">
            The daemon binds to loopback by default. Bind and port changes apply after a daemon
            restart.
          </p>
        </div>
      </section>

      <section class="settings-panel panel" aria-label="Interface">
        <header class="panel-head"><span class="micro">Interface</span><span class="head-rule"></span></header>
        <div class="form-body">
          <div class="form-row">
            <span class="form-label">Theme</span>
            <Segmented
              options={[
                { value: 'bench', label: 'Bench' },
                { value: 'scope', label: 'Scope' },
              ]}
              value={theme}
              ariaLabel="Interface theme"
              onchange={setTheme}
            />
          </div>
          <div class="form-row">
            <label class="form-label" for="meter-rate">Meter rate</label>
            <select id="meter-rate" class="select" bind:value={meterRate}>
              <option value="15">15 Hz</option>
              <option value="30">30 Hz</option>
              <option value="60">60 Hz</option>
            </select>
          </div>
          <div class="form-row">
            <label class="form-label" for="log-level">Log level</label>
            <select id="log-level" class="select" bind:value={logLevel}>
              <option value="error">error</option>
              <option value="warn">warn</option>
              <option value="info">info</option>
              <option value="debug">debug</option>
              <option value="trace">trace</option>
            </select>
          </div>
          <div class="form-row">
            <span class="form-label">Autoload</span>
            <Switch checked={autoload} label="Enable autoload rules" onchange={(v) => (autoload = v)} />
          </div>
          <div class="form-row">
            <span class="form-label">Virtual nodes</span>
            <Switch
              checked={virtualNodes}
              label="Create virtual PipeWire nodes"
              onchange={(v) => (virtualNodes = v)}
            />
          </div>
        </div>
      </section>

      <section class="settings-panel panel" aria-label="Performance">
        <header class="panel-head"><span class="micro">Performance</span><span class="head-rule"></span></header>
        <dl class="defs">
          <dt>CPU</dt>
          <dd class="mono">{metrics ? `${metrics.cpu_percent.toFixed(1)} %` : '—'}</dd>
          <dt>DSP p50</dt>
          <dd class="mono">{metrics ? `${metrics.dsp_p50_us.toFixed(1)} µs` : '—'}</dd>
          <dt>DSP p99</dt>
          <dd class="mono">{metrics ? `${metrics.dsp_p99_us.toFixed(1)} µs` : '—'}</dd>
          <dt>Xruns</dt>
          <dd class="mono">{metrics?.xruns ?? '—'}</dd>
          <dt>Latency</dt>
          <dd class="mono">{metrics ? `${metrics.latency_frames} frames` : '—'}</dd>
          <dt>Sample rate</dt>
          <dd class="mono">{metrics ? `${metrics.sample_rate} Hz` : '—'}</dd>
          <dt>Quantum</dt>
          <dd class="mono">{metrics?.quantum ?? '—'}</dd>
          <dt>Frames</dt>
          <dd class="mono">{metrics ? formatCount(metrics.frames) : '—'}</dd>
        </dl>
      </section>

      <section class="settings-panel panel diagnostics" aria-label="Diagnostics">
        <header class="panel-head">
          <span class="micro">Diagnostics</span>
          <span class="head-rule"></span>
          <button
            type="button"
            class="btn"
            disabled={store.selfTestRunning}
            onclick={() => void store.runSelfTest()}
          >
            <Icon name="check" size={14} />
            {store.selfTestRunning ? 'Running…' : 'Run self-test'}
          </button>
          <button type="button" class="btn" onclick={() => void store.copyDiagnostics()}>
            <Icon name="copy" size={14} />
            Copy diagnostics
          </button>
        </header>
        <div class="diagnostics-body">
          {#if checks}
            <ul class="check-list">
              {#each checks as check (check.name)}
                <li class="check-row">
                  <span class="led" class:led-on={check.ok} class:led-alert={!check.ok}></span>
                  <span class="check-name">{check.name}</span>
                  <span class="check-detail mono">{check.detail}</span>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="form-note">
              Run the self-test to check the PipeWire connection, DSP graph, preset directory and
              asset store. Copy diagnostics includes the daemon status, config and metrics.
            </p>
          {/if}
        </div>
      </section>
    </div>

    <div class="save-bar">
      {#if saveError}
        <span class="save-error">{saveError}</span>
      {:else}
        <span class="micro">Changes are written to the daemon config.</span>
      {/if}
      <span class="spacer"></span>
      <button type="button" class="btn btn-primary" disabled={saving} onclick={() => void save()}>
        {saving ? 'Saving…' : 'Save settings'}
      </button>
    </div>
  {/if}
</div>

<style>
  .settings-screen {
    display: flex;
    flex-direction: column;
    gap: 12px;
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

  .toolbar-meta {
    font-size: 9px;
    color: var(--ink-3);
  }

  .loading-panel {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 200px;
  }

  .settings-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    align-items: start;
  }

  .settings-panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .panel-head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 34px;
    padding: 0 12px;
    border-bottom: 1px solid var(--rule);
    flex-wrap: wrap;
  }

  .head-rule {
    flex: 1;
    height: 1px;
    background: var(--rule);
  }

  .defs {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr);
    gap: 5px 12px;
    margin: 0;
    padding: 12px;
  }

  .defs dt {
    font-size: 11px;
    color: var(--ink-3);
  }

  .defs dd {
    margin: 0;
    font-size: 11px;
    color: var(--ink);
    overflow-wrap: anywhere;
  }

  .state-value {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .form-body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
  }

  .form-row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 28px;
  }

  .form-label {
    width: 110px;
    flex: none;
    font-size: 11px;
    color: var(--ink-2);
  }

  .form-row .input,
  .form-row .select {
    flex: 1;
    min-width: 0;
  }

  .form-row .port {
    max-width: 120px;
    flex: none;
  }

  .form-note {
    margin: 2px 0 0;
    font-size: 11px;
    line-height: 1.45;
    color: var(--ink-3);
    max-width: 60ch;
  }

  .diagnostics {
    grid-column: 1 / -1;
  }

  .diagnostics-body {
    padding: 12px;
  }

  .check-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
    padding: 2px 0;
    border-bottom: 1px solid var(--rule);
  }

  .check-row:last-child {
    border-bottom: 0;
  }

  .check-name {
    width: 180px;
    flex: none;
    font-size: 12px;
    color: var(--ink);
  }

  .check-detail {
    font-size: 10px;
    color: var(--ink-3);
    overflow-wrap: anywhere;
  }

  .save-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    background: var(--panel);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    flex: none;
  }

  .save-error {
    font-size: 12px;
    color: var(--alert);
  }

  @media (max-width: 1099px) {
    .settings-grid {
      grid-template-columns: 1fr;
    }

    .diagnostics {
      grid-column: auto;
    }
  }

  @media (max-width: 899px) {
    .settings-screen {
      height: auto;
      min-height: 100%;
      overflow: visible;
      padding: 8px;
    }

    .check-name {
      width: 130px;
    }
  }
</style>
