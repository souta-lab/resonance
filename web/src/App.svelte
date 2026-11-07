<script lang="ts">
  import { onMount } from 'svelte';
  import { store } from './lib/store.svelte';
  import type { Screen } from './lib/store.svelte';
  import { MOCK } from './lib/api';
  import NavRail from './components/NavRail.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import AnalyzerDrawer from './components/AnalyzerDrawer.svelte';
  import AddEffectPopover from './components/AddEffectPopover.svelte';
  import Toast from './components/Toast.svelte';
  import Banner from './components/Banner.svelte';
  import Icon from './components/Icon.svelte';
  import ChainScreen from './screens/ChainScreen.svelte';
  import PresetsScreen from './screens/PresetsScreen.svelte';
  import DevicesScreen from './screens/DevicesScreen.svelte';
  import AnalyzerScreen from './screens/AnalyzerScreen.svelte';
  import SettingsScreen from './screens/SettingsScreen.svelte';

  const TITLES: Record<Screen, { title: string; sub: string }> = {
    output: { title: 'Output chain', sub: 'Playback processing' },
    input: { title: 'Input chain', sub: 'Capture processing' },
    presets: { title: 'Presets', sub: 'Saved chains' },
    devices: { title: 'Devices & Apps', sub: 'PipeWire graph' },
    analyzer: { title: 'Analyzer', sub: 'Spectrum and EQ' },
    settings: { title: 'Settings & Diagnostics', sub: 'Daemon configuration' },
  };

  const title = $derived(TITLES[store.route]);

  onMount(() => {
    store.initTheme();
    store.initRouter();
    void store.refreshAll();
    const unsubscribe = store.subscribe();

    const onKeyDown = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        if (store.route !== 'output' && store.route !== 'input') {
          store.navigate(store.selectedPipeline);
        }
        store.addOpen = !store.addOpen;
      }
    };
    window.addEventListener('keydown', onKeyDown);

    return () => {
      unsubscribe();
      window.removeEventListener('keydown', onKeyDown);
      store.dispose();
    };
  });

  function toggleTheme(): void {
    store.setTheme(store.theme === 'bench' ? 'scope' : 'bench');
  }
</script>

<div class="app">
  <header class="app-header">
    <div class="brand">
      <span class="logo" aria-hidden="true">
        <Icon name="analyzer" size={16} />
      </span>
      <span class="wordmark">RESONANCE</span>
      <span class="brand-sub micro">Control surface</span>
    </div>

    <span class="head-rule"></span>

    <div class="screen-title">
      <h1>{title.title}</h1>
      <span class="micro screen-sub">{title.sub}</span>
    </div>

    <span class="spacer"></span>

    <div class="head-status">
      {#if MOCK}
        <span class="badge badge-signal">mock</span>
      {/if}
      <span class="conn" title={store.connected ? 'Event stream connected' : 'Event stream disconnected'}>
        <span
          class="led"
          class:led-on={store.connected}
          class:led-alert={!store.connected && !!store.status}
        ></span>
        <span class="mono conn-label">
          {store.status ? (store.status.pipewire.available ? 'PIPEWIRE' : 'OFFLINE') : 'CONNECTING'}
        </span>
      </span>
      <span class="head-sep"></span>
      <span class="mono head-rate">
        {store.status ? `${store.status.dsp.sample_rate / 1000} kHz` : '—'}
      </span>
      <button
        type="button"
        class="icon-btn theme-btn"
        aria-label={store.theme === 'bench' ? 'Switch to night scope' : 'Switch to bench'}
        title={store.theme === 'bench' ? 'Switch to night scope' : 'Switch to bench'}
        onclick={toggleTheme}
      >
        <Icon name={store.theme === 'bench' ? 'moon' : 'sun'} size={16} />
      </button>
    </div>
  </header>

  <div class="nav-slot">
    <NavRail />
  </div>

  <main class="app-main">
    {#if store.loading && !store.status}
      <div class="boot panel">
        <span class="micro">Resonance control surface</span>
        <h2>Connecting to the daemon</h2>
        <div class="boot-line">
          <span class="led led-signal"></span>
          <span class="mono boot-endpoint">GET /api/v1/status</span>
        </div>
        <div class="boot-bar" aria-hidden="true"><span></span></div>
      </div>
    {:else if store.error && !store.status}
      <div class="boot-error">
        <Banner
          message={store.error.message}
          code={store.error.code}
          onretry={() => void store.refreshAll()}
        />
        <p class="boot-hint">
          Check that the daemon is running on <span class="mono">127.0.0.1:8737</span>, then retry.
          If PipeWire is not running, start it with
          <span class="mono">systemctl --user start pipewire</span>.
        </p>
      </div>
    {:else if store.route === 'output' || store.route === 'input'}
      {#key store.route}
        <ChainScreen kind={store.route} />
      {/key}
    {:else if store.route === 'presets'}
      <PresetsScreen />
    {:else if store.route === 'devices'}
      <DevicesScreen />
    {:else if store.route === 'analyzer'}
      <AnalyzerScreen />
    {:else}
      <SettingsScreen />
    {/if}
  </main>

  {#if store.route !== 'analyzer' && store.route !== 'settings'}
    <div class="drawer-slot">
      <AnalyzerDrawer />
    </div>
  {/if}

  <div class="status-slot">
    <StatusBar />
  </div>

  <AddEffectPopover />

  <div class="toast-stack">
    {#each store.toasts as toast (toast.id)}
      <Toast {toast} ondismiss={(id) => store.dismissToast(id)} />
    {/each}
  </div>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: var(--nav-w) minmax(0, 1fr);
    grid-template-rows: var(--header-h) minmax(0, 1fr) auto var(--status-h);
    height: 100vh;
    height: 100dvh;
    overflow: hidden;
    background: var(--chassis);
  }

  .app-header {
    grid-column: 1 / -1;
    grid-row: 1;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--rule-strong);
    min-width: 0;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }

  .logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    background: var(--panel-sunken);
    border: 1px solid var(--rule-strong);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
    color: var(--signal);
  }

  .wordmark {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 500;
    letter-spacing: 0.08em;
    color: var(--ink);
    white-space: nowrap;
  }

  .brand-sub {
    font-size: 8px;
    letter-spacing: 0.06em;
    white-space: nowrap;
  }

  .head-rule {
    width: 1px;
    height: 22px;
    background: var(--rule);
    flex: none;
  }

  .screen-title {
    display: flex;
    flex-direction: column;
    gap: 0;
    min-width: 0;
  }

  h1 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    line-height: 1.15;
    letter-spacing: 0;
    color: var(--ink);
    white-space: nowrap;
  }

  .screen-sub {
    font-size: 8px;
    line-height: 1.2;
    white-space: nowrap;
  }

  .head-status {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }

  .conn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .conn-label {
    font-size: 9px;
    letter-spacing: 0.04em;
    color: var(--ink-2);
  }

  .head-sep {
    width: 1px;
    height: 14px;
    background: var(--rule);
  }

  .head-rate {
    font-size: 9px;
    color: var(--ink-3);
  }

  .theme-btn {
    width: 26px;
    height: 26px;
  }

  .nav-slot {
    grid-column: 1;
    grid-row: 2 / 4;
    display: flex;
    min-height: 0;
  }

  .app-main {
    grid-column: 2;
    grid-row: 2;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }

  .drawer-slot {
    grid-column: 2;
    grid-row: 3;
    min-height: 0;
  }

  .status-slot {
    grid-column: 1 / -1;
    grid-row: 4;
  }

  .boot {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 24px;
    padding: 20px;
    max-width: 520px;
  }

  .boot h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    line-height: 1.2;
  }

  .boot-line {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .boot-endpoint {
    font-size: 10px;
    color: var(--ink-3);
  }

  .boot-bar {
    height: 2px;
    margin-top: 4px;
    background: var(--panel-sunken);
    overflow: hidden;
  }

  .boot-bar span {
    display: block;
    width: 28%;
    height: 100%;
    background: var(--signal);
    animation: boot-scan 1.2s linear infinite;
  }

  .boot-error {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 24px;
    max-width: 640px;
  }

  .boot-hint {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--ink-2);
  }

  .toast-stack {
    position: fixed;
    right: 12px;
    bottom: calc(var(--status-h) + 12px);
    z-index: 80;
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-items: flex-end;
    pointer-events: none;
  }

  .toast-stack :global(.toast) {
    pointer-events: auto;
  }

  @keyframes boot-scan {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(400%);
    }
  }

  @media (max-width: 1099px) {
    .brand-sub,
    .head-rate,
    .head-sep {
      display: none;
    }
  }

  @media (max-width: 899px) {
    .app {
      grid-template-columns: 1fr;
      grid-template-rows: 48px minmax(0, 1fr);
    }

    .app-header {
      grid-column: 1;
      grid-row: 1;
      gap: 8px;
      padding: 0 10px;
    }

    .brand-sub,
    .head-rule,
    .screen-sub,
    .head-sep,
    .head-rate {
      display: none;
    }

    .wordmark {
      font-size: 12px;
    }

    .logo {
      width: 24px;
      height: 24px;
    }

    h1 {
      font-size: 14px;
    }

    .conn-label {
      display: none;
    }

    .nav-slot {
      position: fixed;
      left: 0;
      right: 0;
      bottom: 0;
      z-index: 40;
      display: block;
    }

    .app-main {
      grid-column: 1;
      grid-row: 2;
      overflow: auto;
      padding-bottom: 56px;
    }

    .drawer-slot,
    .status-slot {
      display: none;
    }

    .boot,
    .boot-error {
      margin: 12px;
    }

    .toast-stack {
      right: 8px;
      left: 8px;
      bottom: 64px;
      align-items: stretch;
    }
  }
</style>
