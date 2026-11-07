<script lang="ts">
  import { store } from '../lib/store.svelte';
  import { MOCK } from '../lib/api';
  import { formatMs } from '../lib/format';

  const status = $derived(store.status);
  const online = $derived(store.connected && (status?.pipewire.available ?? false));
  const latencyMs = $derived(
    status ? (status.dsp.latency_frames / status.dsp.sample_rate) * 1000 : 0,
  );
</script>

<footer class="status-bar" aria-label="Daemon status">
  <span class="status-group">
    <span
      class="led"
      class:led-on={online}
      class:led-alert={!!status && !online}
      title={store.connected ? 'Event stream connected' : 'Event stream disconnected'}
    ></span>
    <span class="status-key">PIPEWIRE</span>
    <span class="status-val mono">{status?.pipewire.version ?? 'offline'}</span>
  </span>

  {#if status}
    <span class="status-sep"></span>
    <span class="status-group">
      <span class="status-key">SR</span>
      <span class="status-val mono">{status.dsp.sample_rate / 1000} kHz</span>
    </span>
    <span class="status-sep"></span>
    <span class="status-group">
      <span class="status-key">Q</span>
      <span class="status-val mono">{status.dsp.quantum}</span>
    </span>
    <span class="status-sep"></span>
    <span class="status-group">
      <span class="status-key">XRUNS</span>
      <span class="status-val mono" class:alert={status.dsp.xruns > 0}>{status.dsp.xruns}</span>
    </span>
    <span class="status-sep"></span>
    <span class="status-group">
      <span class="status-key">DSP P99</span>
      <span class="status-val mono">{Math.round(status.dsp.dsp_p99_us)} µs</span>
    </span>
    <span class="status-sep"></span>
    <span class="status-group">
      <span class="status-key">LAT</span>
      <span class="status-val mono">{formatMs(latencyMs)}</span>
    </span>
  {:else}
    <span class="status-sep"></span>
    <span class="status-group">
      <span class="status-val mono">Connecting to the daemon…</span>
    </span>
  {/if}

  <span class="spacer"></span>

  {#if status}
    <span class="status-group">
      <span class="status-key">REV</span>
      <span class="status-val mono">{status.revision}</span>
    </span>
    <span class="status-sep"></span>
    <span class="status-group">
      <span class="status-key">DAEMON</span>
      <span class="status-val mono">{status.daemon.version}</span>
    </span>
  {/if}
  {#if MOCK}
    <span class="badge mock-badge">mock</span>
  {/if}
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: var(--status-h);
    padding: 0 12px;
    background: var(--panel-sunken);
    border-top: 1px solid var(--rule-strong);
    font-size: 9px;
    color: var(--ink-3);
    overflow: hidden;
    white-space: nowrap;
  }

  .status-group {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
  }

  .status-key {
    font-family: var(--font-mono);
    font-size: 8px;
    letter-spacing: 0.04em;
    color: var(--ink-3);
  }

  .status-val {
    font-size: 9px;
    color: var(--ink-2);
  }

  .status-val.alert {
    color: var(--alert);
  }

  .status-sep {
    width: 1px;
    height: 10px;
    background: var(--rule);
    flex: none;
  }

  .mock-badge {
    height: 16px;
    font-size: 8px;
    border-color: var(--signal);
    color: var(--signal);
  }

  @media (max-width: 1099px) {
    .status-bar {
      gap: 6px;
    }

    .status-group:nth-of-type(n + 5) {
      display: none;
    }
  }

  @media (max-width: 899px) {
    .status-bar {
      display: none;
    }
  }
</style>
