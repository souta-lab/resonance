<script lang="ts">
  import { tick } from 'svelte';
  import { store } from '../lib/store.svelte';
  import type { EffectDescriptor } from '../lib/types';
  import Icon from './Icon.svelte';

  let query = $state('');
  let active = $state(0);
  let searchEl = $state<HTMLInputElement | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);

  const groups = $derived.by(() => {
    const catalog = store.catalog;
    if (!catalog) return [];
    const q = query.trim().toLowerCase();
    const matches = catalog.effects.filter((effect) => {
      if (!q) return true;
      const group = catalog.groups.find((g) => g.id === effect.group)?.label ?? effect.group;
      return (
        effect.name.toLowerCase().includes(q) ||
        effect.kind.toLowerCase().includes(q) ||
        effect.summary.toLowerCase().includes(q) ||
        group.toLowerCase().includes(q)
      );
    });
    const known = new Set(catalog.groups.map((g) => g.id));
    const result = catalog.groups
      .map((group) => ({
        id: group.id,
        label: group.label,
        effects: matches.filter((effect) => effect.group === group.id),
      }))
      .filter((group) => group.effects.length > 0);
    const extra = matches.filter((effect) => !known.has(effect.group));
    if (extra.length) result.push({ id: 'other', label: 'Other', effects: extra });
    return result;
  });

  const flat = $derived(groups.flatMap((group) => group.effects));
  const activeEffect = $derived(flat[active] ?? null);

  const insertAt = $derived.by(() => {
    const pipeline = store.pipelines[store.selectedPipeline];
    if (!pipeline) return undefined;
    const index = pipeline.effects.findIndex((e) => e.id === store.selectedEffectId);
    return index >= 0 ? index + 1 : undefined;
  });

  const insertHint = $derived(
    insertAt === undefined
      ? 'Adds at the end of the chain.'
      : `Adds after slot ${String(insertAt).padStart(2, '0')}.`,
  );

  $effect(() => {
    if (!store.addOpen) return;
    void tick().then(() => searchEl?.focus());
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  $effect(() => {
    const effect = flat[active];
    if (!effect || !listEl) return;
    const el = listEl.querySelector<HTMLElement>(`[data-kind="${CSS.escape(effect.kind)}"]`);
    el?.scrollIntoView({ block: 'nearest' });
  });

  function close(): void {
    store.addOpen = false;
    query = '';
    active = 0;
  }

  function add(effect: EffectDescriptor): void {
    void store.addEffect(store.selectedPipeline, effect.kind, insertAt);
    close();
  }

  function onSearchKey(e: KeyboardEvent): void {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      active = Math.min(flat.length - 1, active + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      active = Math.max(0, active - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (activeEffect) add(activeEffect);
    }
  }

  function compatClass(compat: EffectDescriptor['compat']): string {
    if (compat === 'exact') return 'badge badge-ok';
    if (compat === 'approximate') return 'badge badge-signal';
    if (compat === 'substitute') return 'badge badge-alert';
    return 'badge';
  }

  function latencyLabel(latency: EffectDescriptor['latency']): string {
    if (latency === 'zero') return 'zero lat';
    if (latency === 'block') return '1 block';
    return 'adaptive';
  }
</script>

{#if store.addOpen}
  <button type="button" class="backdrop" aria-label="Close add effect" onclick={close}></button>
  <div class="popover" role="dialog" aria-label="Add effect">
    <header class="pop-head">
      <Icon name="search" size={16} />
      <input
        bind:this={searchEl}
        bind:value={query}
        class="search-input"
        type="search"
        placeholder="Search effects"
        aria-label="Search effects"
        aria-controls="add-effect-list"
        aria-activedescendant={activeEffect ? `add-effect-${activeEffect.kind}` : undefined}
        autocomplete="off"
        spellcheck="false"
        oninput={() => (active = 0)}
        onkeydown={onSearchKey}
      />
      <kbd>Esc</kbd>
    </header>

    <div class="pop-body" id="add-effect-list" bind:this={listEl}>
      {#if !store.catalog}
        <p class="pop-empty">Loading catalog…</p>
      {:else if !flat.length}
        <div class="pop-empty">
          <p>No effects match “{query}”.</p>
          <button type="button" class="btn" onclick={() => (query = '')}>Clear search</button>
        </div>
      {:else}
        {#each groups as group (group.id)}
          <div class="group">
            <div class="group-head">
              <span class="micro">{group.label}</span>
              <span class="group-count mono">{group.effects.length}</span>
            </div>
            {#each group.effects as effect (effect.kind)}
              <button
                type="button"
                id={`add-effect-${effect.kind}`}
                data-kind={effect.kind}
                class="effect-option"
                class:active={activeEffect?.kind === effect.kind}
                onclick={() => add(effect)}
                onpointerenter={() => (active = flat.findIndex((e) => e.kind === effect.kind))}
              >
                <span class="option-main">
                  <span class="option-name">{effect.name}</span>
                  <span class="option-summary">{effect.summary}</span>
                </span>
                <span class="option-meta">
                  <span class={compatClass(effect.compat)}>{effect.compat}</span>
                  <span class="micro">{latencyLabel(effect.latency)}</span>
                </span>
              </button>
            {/each}
          </div>
        {/each}
      {/if}
    </div>

    <footer class="pop-foot">
      <span class="micro">{insertHint}</span>
      <span class="spacer"></span>
      <span class="micro">Enter adds · Esc closes</span>
    </footer>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 59;
    width: 100%;
    height: 100%;
    padding: 0;
    background: rgb(20 18 12 / 0.16);
    border: 0;
    cursor: default;
  }

  .popover {
    position: fixed;
    left: calc(var(--nav-w) + 10px);
    top: 64px;
    z-index: 60;
    display: flex;
    flex-direction: column;
    width: 400px;
    max-width: calc(100vw - 24px);
    max-height: calc(100vh - 120px);
    background: var(--panel);
    border: 1px solid var(--rule-strong);
    border-radius: var(--r);
    box-shadow: var(--shadow-raised);
    animation: pop-in 100ms ease-out;
  }

  .pop-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--rule);
    color: var(--ink-3);
    flex: none;
  }

  .search-input {
    flex: 1;
    height: 26px;
    padding: 0 6px;
    background: var(--panel-sunken);
    border: 1px solid var(--rule-strong);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--ink);
    min-width: 0;
  }

  .search-input:focus {
    border-color: var(--signal);
    outline: none;
  }

  kbd {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--ink-3);
    border: 1px solid var(--rule);
    border-radius: 1px;
    padding: 1px 4px;
    background: var(--panel-sunken);
  }

  .pop-body {
    overflow: auto;
    min-height: 0;
    padding: 4px 0 6px;
  }

  .group-head {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 10px 4px;
    background: var(--panel);
    border-bottom: 1px solid var(--rule);
  }

  .group-count {
    font-size: 9px;
    color: var(--ink-3);
  }

  .effect-option {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    background: transparent;
    border: 0;
    border-left: 2px solid transparent;
    text-align: left;
    color: inherit;
  }

  .effect-option:hover,
  .effect-option.active {
    background: var(--panel-sunken);
    border-left-color: var(--signal);
  }

  .option-main {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }

  .option-name {
    font-size: 13px;
    font-weight: 500;
    line-height: 1.2;
    color: var(--ink);
  }

  .option-summary {
    font-size: 11px;
    line-height: 1.35;
    color: var(--ink-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .option-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 3px;
    flex: none;
  }

  .pop-empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 16px 12px;
    font-size: 12px;
    color: var(--ink-2);
  }

  .pop-empty p {
    margin: 0;
  }

  .pop-foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-top: 1px solid var(--rule);
    flex: none;
  }

  @keyframes pop-in {
    from {
      opacity: 0;
      transform: translateY(-3px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (max-width: 899px) {
    .popover {
      left: 8px;
      right: 8px;
      top: auto;
      bottom: 64px;
      width: auto;
      max-height: 62vh;
    }
  }
</style>
