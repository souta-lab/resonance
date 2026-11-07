<script lang="ts">
  import { onMount } from 'svelte';
  import { store, errorMessage } from '../lib/store.svelte';
  import type { PipelineKind, Preset } from '../lib/types';
  import { formatTime, formatUnit } from '../lib/format';
  import Icon from '../components/Icon.svelte';
  import Banner from '../components/Banner.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';
  import Segmented from '../components/Segmented.svelte';

  let selectedId = $state<string | null>(null);
  let selectedPlugin = $state<string | null>(null);
  let detail = $state<Preset | null>(null);
  let detailLoading = $state(false);
  let detailError = $state<string | null>(null);
  let listError = $state<string | null>(null);
  let loading = $state(true);

  let search = $state('');
  let kindFilter = $state<'all' | PipelineKind>('all');

  let importOpen = $state(false);
  let importText = $state('');
  let importName = $state('');
  let importError = $state<string | null>(null);
  let importing = $state(false);
  let fileInput = $state<HTMLInputElement | null>(null);

  let busy = $state(false);
  let confirmDelete = $state(false);
  let selectSeq = 0;

  const list = $derived(store.presets);
  const filtered = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return list.filter((preset) => {
      if (kindFilter !== 'all' && preset.kind !== kindFilter) return false;
      if (!q) return true;
      return preset.name.toLowerCase().includes(q) || preset.id.toLowerCase().includes(q);
    });
  });
  const selectedListItem = $derived(list.find((preset) => preset.id === selectedId) ?? null);
  const selectedInstanceEntries = $derived(
    selectedPlugin
      ? Object.entries(instanceFor(selectedPlugin)).filter(
          ([key, value]) =>
            key !== 'bypass' && !key.includes('.band') && typeof value !== 'object',
        )
      : [],
  );
  const bypassedCount = $derived(
    detail ? detail.plugins_order.filter((id) => isBypassed(id)).length : 0,
  );

  onMount(() => {
    void load();
  });

  async function load(): Promise<void> {
    loading = true;
    listError = null;
    try {
      const items = await store.loadPresets();
      if (!selectedId || !items.some((preset) => preset.id === selectedId)) {
        selectedId = items[0]?.id ?? null;
      }
      if (selectedId) await select(selectedId);
      else {
        detail = null;
        selectedPlugin = null;
      }
    } catch (e) {
      listError = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function select(id: string): Promise<void> {
    selectedId = id;
    const seq = ++selectSeq;
    detailLoading = true;
    detailError = null;
    try {
      const preset = await store.loadPreset(id);
      if (seq === selectSeq) {
        detail = preset;
        selectedPlugin = preset.plugins_order[0] ?? null;
      }
    } catch (e) {
      if (seq === selectSeq) {
        detail = null;
        detailError = errorMessage(e);
      }
    } finally {
      if (seq === selectSeq) detailLoading = false;
    }
  }

  function pluginName(id: string): string {
    const kind = id.split('#')[0];
    return store.catalog?.effects.find((effect) => effect.kind === kind)?.name ?? kind;
  }

  function instanceFor(id: string): Record<string, unknown> {
    return detail?.instances[id] ?? {};
  }

  function isBypassed(id: string): boolean {
    return detail?.instances[id]?.bypass === true;
  }

  function formatValue(key: string, value: unknown, kind: string): string {
    const descriptor = store.catalog?.effects.find((effect) => effect.kind === kind);
    const spec = descriptor?.params.find((param) => param.key === key);
    if (typeof value === 'boolean') return value ? 'on' : 'off';
    if (typeof value === 'number') {
      if (spec?.kind.type === 'enum') return spec.kind.labels[value] ?? String(value);
      if (spec && (spec.kind.type === 'float' || spec.kind.type === 'int')) {
        return formatUnit(value, spec.kind.unit);
      }
      return String(value);
    }
    return String(value);
  }

  function formatInstanceValue(key: string, value: unknown, kind: string): string {
    return `${key} ${formatValue(key, value, kind)}`;
  }

  function summaryFor(id: string): string[] {
    const kind = id.split('#')[0];
    const skip = new Set(['bypass', 'input-gain', 'output-gain', 'dry', 'wet']);
    const entries = Object.entries(instanceFor(id))
      .filter(([key, value]) => !skip.has(key) && !key.includes('.band') && typeof value !== 'object')
      .slice(0, 3);
    if (!entries.length) return ['defaults'];
    return entries.map(([key, value]) => formatInstanceValue(key, value, kind));
  }

  async function apply(kind: PipelineKind): Promise<void> {
    if (!detail || busy) return;
    busy = true;
    await store.applyPreset(detail.id, kind);
    busy = false;
  }

  async function duplicate(): Promise<void> {
    if (!detail || busy) return;
    busy = true;
    const copy = await store.duplicatePreset(detail);
    if (copy) {
      selectedId = copy.id;
      await select(copy.id);
    }
    busy = false;
  }

  async function remove(): Promise<void> {
    if (!detail || busy) return;
    busy = true;
    confirmDelete = false;
    const ok = await store.deletePreset(detail.id);
    if (ok) {
      detail = null;
      selectedId = null;
      selectedPlugin = null;
      await load();
    }
    busy = false;
  }

  function openImport(): void {
    importOpen = true;
    importError = null;
  }

  function closeImport(): void {
    importOpen = false;
    importError = null;
  }

  async function onFile(e: Event): Promise<void> {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    try {
      importText = await file.text();
      if (!importName) importName = file.name.replace(/\.json$/i, '');
      importError = null;
    } catch (err) {
      importError = errorMessage(err);
    }
    input.value = '';
  }

  async function doImport(): Promise<void> {
    if (importing) return;
    if (!importText.trim()) {
      importError = 'Paste a preset JSON document or choose a file.';
      return;
    }
    importing = true;
    importError = null;
    try {
      const result = await store.importPreset(importText, importName.trim() || undefined);
      importOpen = false;
      importText = '';
      importName = '';
      selectedId = result.preset.id;
      await select(result.preset.id);
    } catch (e) {
      importError = errorMessage(e);
    } finally {
      importing = false;
    }
  }
</script>

<div class="presets-screen">
  <section class="preset-list panel" aria-label="Preset list">
    <header class="list-head">
      <span class="micro">Presets</span>
      <span class="count mono">{filtered.length}</span>
      <span class="spacer"></span>
      <button type="button" class="btn" onclick={openImport}>
        <Icon name="upload" size={14} />
        Import
      </button>
    </header>

    <div class="list-tools">
      <div class="search-wrap">
        <Icon name="search" size={14} />
        <input
          class="input search"
          type="search"
          placeholder="Search presets"
          aria-label="Search presets"
          bind:value={search}
        />
      </div>
      <Segmented
        options={[
          { value: 'all', label: 'All' },
          { value: 'output', label: 'Output' },
          { value: 'input', label: 'Input' },
        ]}
        value={kindFilter}
        ariaLabel="Filter presets by kind"
        onchange={(value) => (kindFilter = value as 'all' | PipelineKind)}
      />
    </div>

    {#if listError}
      <div class="list-banner">
        <Banner message={listError} onretry={() => void load()} />
      </div>
    {/if}

    <div class="list-scroll">
      {#if loading}
        <p class="list-empty micro">Loading presets…</p>
      {:else if !filtered.length}
        <div class="list-empty">
          <p>{list.length ? 'No presets match this filter.' : 'No presets yet.'}</p>
          {#if !list.length}
            <button type="button" class="btn" onclick={openImport}>Import preset</button>
          {/if}
        </div>
      {:else}
        <ul class="preset-items">
          {#each filtered as preset (preset.id)}
            <li>
              <button
                type="button"
                class="preset-item"
                class:selected={preset.id === selectedId}
                aria-current={preset.id === selectedId ? 'true' : undefined}
                onclick={() => void select(preset.id)}
              >
                <span class="item-top">
                  <span class="item-name">{preset.name}</span>
                  {#if preset.missing_assets.length}
                    <span class="missing" title={`${preset.missing_assets.length} missing assets`}>
                      <Icon name="alert" size={12} />
                      {preset.missing_assets.length}
                    </span>
                  {/if}
                </span>
                <span class="item-meta">
                  <span class="badge">{preset.kind}</span>
                  <span class="badge">{preset.source}</span>
                  <span class="mono item-count">{preset.effects} fx</span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </section>

  <section class="preset-detail" aria-label="Preset detail">
    {#if importOpen}
      <div class="import-panel panel">
        <header class="import-head">
          <span class="micro">Import preset</span>
          <span class="spacer"></span>
          <button type="button" class="icon-btn" aria-label="Close import" onclick={closeImport}>
            <Icon name="close" size={14} />
          </button>
        </header>
        <div class="import-body">
          <div class="import-row">
            <input
              class="input import-name"
              type="text"
              placeholder="Preset name (optional)"
              aria-label="Preset name"
              bind:value={importName}
            />
            <input
              class="sr-only"
              type="file"
              accept=".json,application/json"
              bind:this={fileInput}
              onchange={onFile}
            />
            <button type="button" class="btn" onclick={() => fileInput?.click()}>
              <Icon name="file" size={14} />
              Choose file
            </button>
          </div>
          <label class="import-label" for="import-json">Preset JSON</label>
          <textarea
            id="import-json"
            class="import-text mono"
            rows="7"
            placeholder={'{\n  "output": {\n    "plugins_order": ["equalizer#0"]\n  }\n}'}
            bind:value={importText}
          ></textarea>
          {#if importError}
            <Banner message={importError} />
          {/if}
          <div class="import-actions">
            <button type="button" class="btn" onclick={closeImport}>Cancel</button>
            <button type="button" class="btn btn-primary" disabled={importing} onclick={doImport}>
              {importing ? 'Importing…' : 'Import preset'}
            </button>
          </div>
        </div>
      </div>
    {/if}

    {#if detailError}
      <Banner message={detailError} onretry={() => selectedId && void select(selectedId)} />
    {/if}

    {#if detailLoading}
      <div class="detail-empty panel">
        <span class="micro">Loading preset…</span>
      </div>
    {:else if !detail}
      <EmptyState
        fill
        title={list.length ? 'Select a preset.' : 'No presets yet.'}
        description={list.length
          ? 'Choose a preset from the list to inspect its plugins, warnings and missing assets.'
          : 'Import an EasyEffects preset to get started. The daemon stores presets locally.'}
        actionLabel={list.length ? undefined : 'Import preset'}
        onaction={list.length ? undefined : openImport}
        icon="presets"
      />
    {:else}
      <div class="detail-scroll">
        <header class="detail-head">
          <div class="detail-title">
            <span class="micro">Preset · {detail.id}</span>
            <h2>{detail.name}</h2>
            <div class="detail-badges">
              <span class="badge">{detail.kind}</span>
              <span class="badge">{selectedListItem?.source ?? 'local'}</span>
              {#if selectedListItem}
                <span class="mono detail-time">{formatTime(selectedListItem.updated_at)}</span>
              {/if}
            </div>
          </div>
          <div class="detail-actions">
            <button
              type="button"
              class="btn"
              class:btn-primary={detail.kind === 'output'}
              disabled={busy}
              onclick={() => apply('output')}
            >
              Apply to Output
            </button>
            <button
              type="button"
              class="btn"
              class:btn-primary={detail.kind === 'input'}
              disabled={busy}
              onclick={() => apply('input')}
            >
              Apply to Input
            </button>
            <span class="action-rule"></span>
            <button
              type="button"
              class="icon-btn"
              aria-label="Export preset"
              title="Export preset"
              disabled={busy}
              onclick={() => detail && void store.exportPreset(detail.id)}
            >
              <Icon name="download" size={16} />
            </button>
            <button
              type="button"
              class="icon-btn"
              aria-label="Duplicate preset"
              title="Duplicate preset"
              disabled={busy}
              onclick={duplicate}
            >
              <Icon name="copy" size={16} />
            </button>
            <button
              type="button"
              class="icon-btn"
              aria-label="Delete preset"
              title="Delete preset"
              disabled={busy}
              onclick={() => (confirmDelete = true)}
            >
              <Icon name="trash" size={16} />
            </button>
          </div>
        </header>

        <div class="stat-strip">
          <div class="stat">
            <span class="micro">Plugins</span>
            <span class="stat-value mono">{detail.plugins_order.length}</span>
          </div>
          <div class="stat">
            <span class="micro">Bypassed</span>
            <span class="stat-value mono">{bypassedCount}</span>
          </div>
          <div class="stat">
            <span class="micro">Missing assets</span>
            <span class="stat-value mono" class:alert={detail.missing_assets.length > 0}>
              {detail.missing_assets.length}
            </span>
          </div>
          <div class="stat">
            <span class="micro">Warnings</span>
            <span class="stat-value mono" class:alert={detail.warnings.length > 0}>
              {detail.warnings.length}
            </span>
          </div>
        </div>

        {#if detail.missing_assets.length}
          <div class="alert-panel">
            <div class="alert-head">
              <Icon name="alert" size={16} />
              <span>Missing assets</span>
            </div>
            <p>
              This preset references {detail.missing_assets.length} asset{detail.missing_assets
                .length === 1
                ? ''
                : 's'} that are not installed. The affected effects will load with their default
              response.
            </p>
            <ul class="asset-list">
              {#each detail.missing_assets as asset}
                <li class="mono">{asset.instance} · {asset.name} ({asset.kind})</li>
              {/each}
            </ul>
          </div>
        {/if}

        {#if detail.warnings.length}
          <div class="warning-panel">
            <div class="alert-head">
              <Icon name="alert" size={16} />
              <span>Warnings</span>
            </div>
            <ul class="warning-list">
              {#each detail.warnings as warning}
                <li>{warning}</li>
              {/each}
            </ul>
          </div>
        {/if}

        <div class="plugins-layout">
          <section class="detail-section" aria-label="Plugins">
            <div class="section-head">
              <span class="micro">Plugins · {detail.plugins_order.length}</span>
              <span class="section-rule"></span>
            </div>
            {#if detail.plugins_order.length}
              <ol class="plugin-list">
                {#each detail.plugins_order as id, index (id)}
                  <li>
                    <button
                      type="button"
                      class="plugin-row"
                      class:selected={selectedPlugin === id}
                      aria-current={selectedPlugin === id ? 'true' : undefined}
                      onclick={() => (selectedPlugin = id)}
                    >
                      <span class="plugin-index mono">{String(index + 1).padStart(2, '0')}</span>
                      <span class="plugin-main">
                        <span class="plugin-name">{pluginName(id)}</span>
                        <span class="plugin-id mono">{id}</span>
                      </span>
                      <span class="plugin-summary mono">
                        {#each summaryFor(id) as part}
                          <span>{part}</span>
                        {/each}
                      </span>
                      {#if isBypassed(id)}
                        <span class="badge">bypassed</span>
                      {/if}
                    </button>
                  </li>
                {/each}
              </ol>
            {:else}
              <p class="detail-note">This preset has no plugins. Applying it clears the chain.</p>
            {/if}
          </section>

          <aside class="instance-panel panel" aria-label="Instance summary">
            <header class="instance-head">
              <span class="micro">Instance</span>
              {#if selectedPlugin}
                <span class="mono instance-id">{selectedPlugin}</span>
              {/if}
              <span class="spacer"></span>
              {#if selectedPlugin && isBypassed(selectedPlugin)}
                <span class="badge">bypassed</span>
              {/if}
            </header>
            <div class="instance-body">
              {#if selectedPlugin}
                {#if selectedInstanceEntries.length}
                  <dl class="instance-defs">
                    {#each selectedInstanceEntries as [key, value] (key)}
                      <dt class="mono">{key}</dt>
                      <dd class="mono">{formatValue(key, value, selectedPlugin.split('#')[0])}</dd>
                    {/each}
                  </dl>
                {:else}
                  <p class="detail-note">
                    No instance overrides. The effect loads with its defaults.
                  </p>
                {/if}
              {:else}
                <p class="detail-note">Select a plugin to inspect its instance values.</p>
              {/if}
            </div>
          </aside>
        </div>

        <section class="checks panel" aria-label="Preset checks">
          <div class="check-row">
            <span
              class="led"
              class:led-on={!detail.missing_assets.length}
              class:led-alert={detail.missing_assets.length > 0}
            ></span>
            <span class="check-label">Missing assets</span>
            <span class="check-value mono">
              {detail.missing_assets.length ? `${detail.missing_assets.length} missing` : 'none'}
            </span>
          </div>
          <div class="check-row">
            <span
              class="led"
              class:led-on={!detail.warnings.length}
              class:led-alert={detail.warnings.length > 0}
            ></span>
            <span class="check-label">Warnings</span>
            <span class="check-value mono">
              {detail.warnings.length
                ? `${detail.warnings.length} warning${detail.warnings.length === 1 ? '' : 's'}`
                : 'none'}
            </span>
          </div>
          <div class="check-row">
            <span class="led" class:led-on={!detail.blocklist.length}></span>
            <span class="check-label">Blocked apps</span>
            <span class="check-value mono">
              {detail.blocklist.length ? detail.blocklist.join(', ') : 'none'}
            </span>
          </div>
        </section>

        {#if detail.blocklist.length}
          <section class="detail-section" aria-label="Blocked applications">
            <div class="section-head">
              <span class="micro">Blocked apps</span>
              <span class="section-rule"></span>
            </div>
            <div class="chips">
              {#each detail.blocklist as app}
                <span class="badge">{app}</span>
              {/each}
            </div>
          </section>
        {/if}
      </div>
    {/if}
  </section>

  <ConfirmDialog
    open={confirmDelete}
    title="Delete preset"
    message={`Delete ${detail?.name ?? 'this preset'}? The preset file is removed from the daemon. This cannot be undone.`}
    confirmLabel="Delete"
    danger
    onconfirm={remove}
    oncancel={() => (confirmDelete = false)}
  />
</div>

<style>
  .presets-screen {
    display: grid;
    grid-template-columns: 340px minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    background: var(--chassis);
  }

  .preset-list {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 0;
    border-right: 1px solid var(--rule);
    border-radius: 0;
    box-shadow: none;
  }

  .list-head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 40px;
    padding: 0 12px;
    border-bottom: 1px solid var(--rule);
    flex: none;
  }

  .count {
    font-size: 10px;
    color: var(--ink-3);
  }

  .list-tools {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--rule);
    flex: none;
  }

  .search-wrap {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--ink-3);
  }

  .search {
    flex: 1;
    padding-left: 6px;
  }

  .list-banner {
    padding: 8px 12px;
    flex: none;
  }

  .list-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 6px;
  }

  .preset-items {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .preset-item {
    display: flex;
    flex-direction: column;
    gap: 5px;
    width: 100%;
    padding: 8px 9px;
    background: transparent;
    border: 1px solid transparent;
    border-left: 2px solid transparent;
    border-radius: var(--r);
    text-align: left;
    color: inherit;
  }

  .preset-item:hover {
    background: var(--panel-sunken);
    border-color: var(--rule);
  }

  .preset-item.selected {
    background: var(--panel-raised);
    border-color: var(--rule-strong);
    border-left-color: var(--signal);
    box-shadow: var(--shadow-raised);
  }

  .item-top {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
  }

  .item-name {
    flex: 1;
    font-size: 13px;
    font-weight: 500;
    line-height: 1.2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .missing {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    color: var(--alert);
    font-family: var(--font-mono);
    font-size: 9px;
  }

  .item-meta {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .item-count {
    font-size: 9px;
    color: var(--ink-3);
    margin-left: auto;
  }

  .list-empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 16px 8px;
    color: var(--ink-2);
    font-size: 12px;
  }

  .list-empty p {
    margin: 0;
  }

  .preset-detail {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    overflow: auto;
    padding: 12px;
    gap: 12px;
  }

  .import-panel {
    flex: none;
  }

  .import-head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 12px;
    border-bottom: 1px solid var(--rule);
  }

  .import-body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
  }

  .import-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .import-name {
    flex: 1;
  }

  .import-label {
    font-size: 11px;
    color: var(--ink-2);
  }

  .import-text {
    width: 100%;
    padding: 8px;
    background: var(--panel-sunken);
    border: 1px solid var(--rule-strong);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
    font-size: 11px;
    line-height: 1.45;
    color: var(--ink);
    resize: vertical;
  }

  .import-text:focus {
    border-color: var(--signal);
    outline: none;
  }

  .import-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .detail-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    min-height: 240px;
  }

  .detail-scroll {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 4px;
  }

  .detail-head {
    display: flex;
    align-items: flex-start;
    gap: 16px;
    flex-wrap: wrap;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--rule);
  }

  .detail-title {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    line-height: 1.15;
  }

  .detail-badges {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .detail-time {
    font-size: 9px;
    color: var(--ink-3);
  }

  .detail-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    flex-wrap: wrap;
  }

  .action-rule {
    width: 1px;
    height: 18px;
    background: var(--rule);
    margin: 0 2px;
  }

  .alert-panel,
  .warning-panel {
    padding: 10px 12px;
    background: color-mix(in srgb, var(--alert) 6%, var(--panel));
    border: 1px solid color-mix(in srgb, var(--alert) 45%, var(--rule));
    border-left: 2px solid var(--alert);
    border-radius: var(--r);
  }

  .warning-panel {
    background: color-mix(in srgb, var(--signal) 6%, var(--panel));
    border-color: color-mix(in srgb, var(--signal) 45%, var(--rule));
    border-left-color: var(--signal);
  }

  .alert-head {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink);
    margin-bottom: 4px;
  }

  .alert-panel .alert-head {
    color: var(--alert);
  }

  .warning-panel .alert-head {
    color: var(--signal);
  }

  .alert-panel p {
    margin: 0 0 6px;
    font-size: 12px;
    line-height: 1.45;
    color: var(--ink-2);
  }

  .asset-list,
  .warning-list {
    margin: 0;
    padding-left: 18px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--ink-2);
  }

  .asset-list li {
    font-size: 10px;
  }

  .detail-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .section-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .section-rule {
    flex: 1;
    height: 1px;
    background: var(--rule);
  }

  .plugin-list {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--rule);
    border-radius: var(--r);
    overflow: hidden;
  }

  .plugin-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 40px;
    padding: 6px 10px;
    background: var(--panel);
    border: 0;
    border-bottom: 1px solid var(--rule);
    border-left: 2px solid transparent;
    text-align: left;
    color: inherit;
  }

  .plugin-row:hover {
    background: var(--panel-sunken);
  }

  .plugin-row.selected {
    background: var(--panel-raised);
    border-left-color: var(--signal);
  }

  .plugin-list li:last-child .plugin-row {
    border-bottom: 0;
  }

  .stat-strip {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 1px;
    background: var(--rule);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    overflow: hidden;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    background: var(--panel);
  }

  .stat-value {
    font-size: 15px;
    line-height: 1.1;
    color: var(--ink);
  }

  .stat-value.alert {
    color: var(--alert);
  }

  .plugins-layout {
    display: grid;
    grid-template-columns: minmax(0, 1.5fr) minmax(260px, 1fr);
    gap: 12px;
    align-items: start;
  }

  .instance-panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .instance-head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 34px;
    padding: 0 10px;
    border-bottom: 1px solid var(--rule);
    flex-wrap: wrap;
  }

  .instance-id {
    font-size: 9px;
    color: var(--ink-3);
  }

  .instance-body {
    padding: 10px;
  }

  .instance-defs {
    display: grid;
    grid-template-columns: minmax(80px, 1fr) minmax(0, 1.2fr);
    gap: 4px 10px;
    margin: 0;
  }

  .instance-defs dt {
    font-size: 10px;
    color: var(--ink-3);
    overflow-wrap: anywhere;
  }

  .instance-defs dd {
    margin: 0;
    font-size: 10px;
    color: var(--ink);
    text-align: right;
    overflow-wrap: anywhere;
  }

  .checks {
    display: flex;
    flex-direction: column;
    padding: 4px 12px;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
    border-bottom: 1px solid var(--rule);
  }

  .check-row:last-child {
    border-bottom: 0;
  }

  .check-label {
    font-size: 11px;
    color: var(--ink-2);
    width: 120px;
    flex: none;
  }

  .check-value {
    font-size: 10px;
    color: var(--ink-3);
    margin-left: auto;
    text-align: right;
  }

  .plugin-index {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    flex: none;
    background: var(--panel-sunken);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    font-size: 9px;
    color: var(--ink-3);
  }

  .plugin-main {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    width: 180px;
    flex: none;
  }

  .plugin-name {
    font-size: 12px;
    font-weight: 500;
    color: var(--ink);
  }

  .plugin-id {
    font-size: 9px;
    color: var(--ink-3);
  }

  .plugin-summary {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    flex: 1;
    font-size: 10px;
    color: var(--ink-2);
    min-width: 0;
  }

  .detail-note {
    margin: 0;
    font-size: 12px;
    color: var(--ink-2);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  @media (max-width: 899px) {
    .presets-screen {
      grid-template-columns: 1fr;
      height: auto;
      min-height: 100%;
    }

    .preset-list {
      border-right: 0;
      border-bottom: 1px solid var(--rule);
      max-height: 46vh;
    }

    .preset-detail {
      overflow: visible;
      padding: 8px;
    }

    .plugin-main {
      width: 130px;
    }

    .plugins-layout {
      grid-template-columns: 1fr;
    }

    .stat-strip {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
