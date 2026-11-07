<script lang="ts">
  import { store, pipelineLabel } from '../lib/store.svelte';
  import type { PipelineKind } from '../lib/types';
  import { formatDb } from '../lib/format';
  import Icon from './Icon.svelte';
  import Switch from './Switch.svelte';
  import EmptyState from './EmptyState.svelte';

  interface Props {
    kind: PipelineKind;
  }

  let { kind }: Props = $props();

  const pipeline = $derived(store.pipelines[kind]);
  const effects = $derived(pipeline?.effects ?? []);
  const label = $derived(pipelineLabel(kind));

  let armedId = $state<string | null>(null);
  let draggingId = $state<string | null>(null);
  let dropIndex = $state(-1);
  let dropAfter = $state(false);
  let announcement = $state('');

  const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
  const shortcut = isMac ? '⌘K' : 'Ctrl K';

  function grWidth(gr: number): number {
    return Math.min(100, (Math.abs(gr) / 24) * 100);
  }

  function groupLabel(effectKind: string): string {
    const descriptor = store.catalog?.effects.find((effect) => effect.kind === effectKind);
    return (
      store.catalog?.groups.find((group) => group.id === descriptor?.group)?.label ??
      effectKind.replace(/_/g, ' ')
    );
  }

  function onDragStart(e: DragEvent, id: string): void {
    if (armedId !== id) {
      e.preventDefault();
      return;
    }
    draggingId = id;
    e.dataTransfer?.setData('text/plain', id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }

  function onDragOver(e: DragEvent, index: number): void {
    if (!draggingId) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    dropIndex = index;
    dropAfter = e.clientY > rect.top + rect.height / 2;
  }

  function onDragLeave(e: DragEvent): void {
    const related = e.relatedTarget as Node | null;
    if (!related || !(e.currentTarget as HTMLElement).contains(related)) {
      dropIndex = -1;
    }
  }

  function onDrop(e: DragEvent, index: number): void {
    e.preventDefault();
    const id = draggingId ?? e.dataTransfer?.getData('text/plain') ?? null;
    if (!id) {
      clearDrag();
      return;
    }
    const from = effects.findIndex((x) => x.id === id);
    if (from < 0) {
      clearDrag();
      return;
    }
    let to = dropAfter ? index + 1 : index;
    if (from < to) to -= 1;
    to = Math.max(0, Math.min(to, effects.length - 1));
    if (to !== from) {
      announcement = `Moved ${effects[from].name} to slot ${to + 1} of ${effects.length}.`;
      void store.reorder(kind, from, to);
    }
    clearDrag();
  }

  function clearDrag(): void {
    armedId = null;
    draggingId = null;
    dropIndex = -1;
    dropAfter = false;
  }

  function onSlotKeyDown(e: KeyboardEvent, index: number): void {
    if (!e.altKey) return;
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
    e.preventDefault();
    const to = e.key === 'ArrowUp' ? index - 1 : index + 1;
    if (to < 0 || to >= effects.length) return;
    announcement = `Moved ${effects[index].name} to slot ${to + 1} of ${effects.length}.`;
    void store.reorder(kind, index, to);
  }

  function openAdd(): void {
    store.addOpen = true;
  }
</script>

<aside class="chain-rail" aria-label={`${label} signal chain`}>
  <header class="chain-head">
    <span class="micro">Signal chain</span>
    <span class="badge">{label}</span>
    <span class="spacer"></span>
    <span class="micro bypass-label">Bypass</span>
    <Switch
      checked={pipeline?.bypass ?? false}
      label={`Bypass ${label.toLowerCase()} chain`}
      onchange={(checked) => store.setBypass(kind, checked)}
    />
  </header>

  <div class="chain-scroll">
    {#if effects.length}
      <ol class="slots" class:bypassed={pipeline?.bypass}>
        {#each effects as effect, index (effect.id)}
          <li
            class="slot"
            class:selected={effect.id === store.selectedEffectId}
            class:bypassed={effect.bypass}
            class:dragging={draggingId === effect.id}
            class:drop-before={dropIndex === index && !dropAfter}
            class:drop-after={dropIndex === index && dropAfter}
            draggable="true"
            ondragstart={(e) => onDragStart(e, effect.id)}
            ondragover={(e) => onDragOver(e, index)}
            ondragleave={onDragLeave}
            ondrop={(e) => onDrop(e, index)}
            ondragend={clearDrag}
          >
            <button
              type="button"
              class="slot-main"
              aria-current={effect.id === store.selectedEffectId ? 'true' : undefined}
              onclick={() => store.selectEffect(effect.id, kind)}
              onkeydown={(e) => onSlotKeyDown(e, index)}
            >
              <span class="slot-index mono">{String(index + 1).padStart(2, '0')}</span>
              <span class="slot-text">
                <span class="slot-name">{effect.name}</span>
                <span class="slot-kind micro">{groupLabel(effect.kind)}</span>
              </span>
            </button>

            <div class="slot-controls">
              <Switch
                checked={effect.bypass}
                label={`Bypass ${effect.name}`}
                onchange={(checked) => store.setEffectBypass(kind, effect.id, checked)}
              />
              <span
                class="handle"
                aria-hidden="true"
                title="Drag to reorder"
                onpointerdown={() => (armedId = effect.id)}
              >
                <Icon name="grip" size={16} stroke={2.5} />
              </span>
            </div>

            <span
              class="gr-track"
              title={`Gain reduction ${formatDb(effect.gain_reduction_db)} dB`}
            >
              <span class="gr-fill" style:width="{grWidth(effect.gain_reduction_db)}%"></span>
            </span>
          </li>
        {/each}
      </ol>
    {:else}
      <div class="chain-empty">
        <EmptyState
          title="No effects in this chain."
          description="Add an effect to start building the chain, or apply a saved preset."
          actionLabel="Add effect"
          onaction={openAdd}
          icon="rack"
        />
      </div>
    {/if}

    <button type="button" class="add-effect" onclick={openAdd}>
      <Icon name="plus" size={16} />
      <span>Add effect</span>
      <span class="spacer"></span>
      <kbd>{shortcut}</kbd>
    </button>

    <p class="chain-hint micro">Alt+↑/↓ reorders the selected slot. Drag the handle to move it.</p>
  </div>

  <footer class="chain-foot">
    <span class="mono">{effects.length} effect{effects.length === 1 ? '' : 's'}</span>
    <span class="foot-rule"></span>
    <span class="mono">{pipeline?.latency_frames ?? 0} frames</span>
    <span class="spacer"></span>
    <span class="micro">{pipeline?.revision !== undefined ? `rev ${pipeline.revision}` : '—'}</span>
  </footer>

  <div class="sr-only" aria-live="polite">{announcement}</div>
</aside>

<style>
  .chain-rail {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    background: var(--panel);
    border-right: 1px solid var(--rule);
  }

  .chain-head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border-bottom: 1px solid var(--rule);
    flex: none;
  }

  .bypass-label {
    font-size: 9px;
  }

  .chain-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 8px;
  }

  .slots {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .slots.bypassed {
    opacity: 0.55;
  }

  .slot {
    position: relative;
    display: flex;
    align-items: stretch;
    min-height: 52px;
    background: var(--panel);
    border: 1px solid var(--rule);
    border-left: 2px solid transparent;
    border-radius: var(--r);
    box-shadow: var(--shadow-inset);
    transition: background 80ms linear, border-color 80ms linear;
  }

  .slot:hover {
    background: var(--panel-raised);
    border-color: var(--rule-strong);
  }

  .slot.selected {
    background: var(--panel-raised);
    border-color: var(--rule-strong);
    border-left-color: var(--signal);
    box-shadow: var(--shadow-raised);
  }

  .slot.bypassed .slot-name {
    color: var(--ink-3);
  }

  .slot.dragging {
    opacity: 0.45;
  }

  .slot.drop-before::before,
  .slot.drop-after::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--signal);
    z-index: 3;
  }

  .slot.drop-before::before {
    top: -3px;
  }

  .slot.drop-after::after {
    bottom: -3px;
  }

  .slot-main {
    display: flex;
    align-items: center;
    gap: 9px;
    flex: 1;
    min-width: 0;
    padding: 6px 4px 8px 8px;
    background: transparent;
    border: 0;
    text-align: left;
    color: inherit;
  }

  .slot-index {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    flex: none;
    background: var(--panel-sunken);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
    font-size: 10px;
    color: var(--ink-3);
  }

  .slot.selected .slot-index {
    color: var(--signal);
    border-color: var(--signal);
  }

  .slot-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .slot-name {
    font-size: 13px;
    font-weight: 500;
    line-height: 1.2;
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .slot-kind {
    font-size: 9px;
    line-height: 1.2;
  }

  .slot-controls {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 4px;
    flex: none;
  }

  .handle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 28px;
    color: var(--ink-3);
    cursor: grab;
    touch-action: none;
  }

  .handle:active {
    cursor: grabbing;
  }

  .handle:hover {
    color: var(--ink);
  }

  .gr-track {
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: 3px;
    height: 2px;
    background: color-mix(in srgb, var(--panel-sunken) 60%, var(--ink) 12%);
    border-radius: 0;
    overflow: hidden;
  }

  .gr-fill {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    background: var(--signal);
  }

  .chain-empty {
    margin-bottom: 8px;
  }

  .add-effect {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 34px;
    margin-top: 8px;
    padding: 0 10px;
    background: var(--panel-sunken);
    border: 1px solid var(--rule-strong);
    border-radius: var(--r);
    box-shadow: var(--shadow-inset);
    color: var(--ink);
    font-size: 12px;
    font-weight: 500;
    text-align: left;
  }

  .add-effect:hover {
    background: var(--panel-raised);
    border-color: var(--ink-3);
  }

  kbd {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--ink-3);
    border: 1px solid var(--rule);
    border-radius: 1px;
    padding: 1px 4px;
    background: var(--panel);
  }

  .chain-hint {
    margin: 8px 2px 0;
    font-size: 9px;
    line-height: 1.4;
    text-transform: none;
    letter-spacing: 0;
  }

  .chain-foot {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 26px;
    padding: 0 12px;
    border-top: 1px solid var(--rule);
    font-size: 9px;
    color: var(--ink-3);
    flex: none;
  }

  .foot-rule {
    width: 1px;
    height: 10px;
    background: var(--rule);
  }

  @media (max-width: 899px) {
    .chain-rail {
      height: auto;
      width: 100%;
      min-width: 0;
      border-right: 0;
      border-bottom: 1px solid var(--rule);
    }

    .chain-head {
      height: 32px;
    }

    .chain-scroll {
      display: flex;
      align-items: flex-start;
      gap: 8px;
      min-width: 0;
      overflow-x: auto;
      overflow-y: hidden;
      padding: 8px;
    }

    .slots {
      flex-direction: row;
      gap: 6px;
      flex: none;
    }

    .slot {
      width: 168px;
      flex: none;
    }

    .chain-empty {
      width: 260px;
      flex: none;
      margin-bottom: 0;
    }

    .add-effect {
      width: 132px;
      flex: none;
      margin-top: 0;
      min-height: 52px;
    }

    .chain-hint,
    .chain-foot {
      display: none;
    }
  }
</style>
