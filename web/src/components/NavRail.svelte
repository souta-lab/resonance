<script lang="ts">
  import { store } from '../lib/store.svelte';
  import type { Screen } from '../lib/store.svelte';
  import Icon from './Icon.svelte';
  import type { IconName } from '../lib/icons';

  interface NavItem {
    id: Screen;
    label: string;
    title: string;
    icon: IconName;
  }

  const items: NavItem[] = [
    { id: 'output', label: 'Output', title: 'Output chain', icon: 'output' },
    { id: 'input', label: 'Input', title: 'Input chain', icon: 'input' },
    { id: 'presets', label: 'Presets', title: 'Presets', icon: 'presets' },
    { id: 'devices', label: 'Devices', title: 'Devices & Apps', icon: 'devices' },
    { id: 'analyzer', label: 'Analyzer', title: 'Analyzer', icon: 'analyzer' },
    { id: 'settings', label: 'Settings', title: 'Settings & Diagnostics', icon: 'settings' },
  ];

  function toggleTheme(): void {
    store.setTheme(store.theme === 'bench' ? 'scope' : 'bench');
  }
</script>

<nav class="nav-rail" aria-label="Primary">
  <div class="nav-items">
    {#each items as item (item.id)}
      <button
        type="button"
        class="nav-item"
        class:active={store.route === item.id}
        aria-current={store.route === item.id ? 'page' : undefined}
        title={item.title}
        onclick={() => store.navigate(item.id)}
      >
        <Icon name={item.icon} size={18} />
        <span class="nav-label">{item.label}</span>
      </button>
    {/each}
  </div>

  <div class="nav-bottom">
    <button
      type="button"
      class="theme-toggle"
      aria-label={store.theme === 'bench' ? 'Switch to night scope' : 'Switch to bench'}
      title={store.theme === 'bench' ? 'Switch to night scope' : 'Switch to bench'}
      onclick={toggleTheme}
    >
      <Icon name={store.theme === 'bench' ? 'moon' : 'sun'} size={16} />
    </button>
    <span class="nav-version mono">0.1</span>
  </div>
</nav>

<style>
  .nav-rail {
    display: flex;
    flex-direction: column;
    width: var(--nav-w);
    min-height: 0;
    background: var(--panel);
    border-right: 1px solid var(--rule);
  }

  .nav-items {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 0;
    flex: 1;
  }

  .nav-item {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 54px;
    padding: 0;
    background: transparent;
    border: 0;
    border-left: 2px solid transparent;
    color: var(--ink-2);
  }

  .nav-item:hover {
    background: var(--panel-sunken);
    color: var(--ink);
  }

  .nav-item.active {
    background: var(--panel-sunken);
    border-left-color: var(--signal);
    color: var(--signal);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.25);
  }

  .nav-label {
    font-family: var(--font-mono);
    font-size: 9px;
    line-height: 1;
    letter-spacing: 0.02em;
    color: var(--ink-3);
  }

  .nav-item.active .nav-label {
    color: var(--ink);
  }

  .nav-bottom {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 6px 0 8px;
    border-top: 1px solid var(--rule);
  }

  .theme-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 28px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r);
    color: var(--ink-3);
  }

  .theme-toggle:hover {
    background: var(--panel-sunken);
    border-color: var(--rule);
    color: var(--ink);
  }

  .nav-version {
    font-size: 8px;
    color: var(--ink-3);
    letter-spacing: 0.04em;
  }

  @media (max-width: 899px) {
    .nav-rail {
      position: fixed;
      left: 0;
      right: 0;
      bottom: 0;
      z-index: 40;
      width: 100%;
      height: 56px;
      flex-direction: row;
      border-right: 0;
      border-top: 1px solid var(--rule-strong);
      box-shadow: 0 -2px 8px rgb(20 18 12 / 0.12);
    }

    .nav-items {
      flex-direction: row;
      padding: 0;
      gap: 0;
    }

    .nav-item {
      flex: 1;
      height: 56px;
      border-left: 0;
      border-top: 2px solid transparent;
    }

    .nav-item.active {
      border-left-color: transparent;
      border-top-color: var(--signal);
      box-shadow: none;
    }

    .nav-label {
      font-size: 8px;
    }

    .nav-bottom {
      display: none;
    }
  }
</style>
