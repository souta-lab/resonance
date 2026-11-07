<script lang="ts">
  import { onMount } from 'svelte';
  import { store } from '../lib/store.svelte';
  import type { PipelineKind } from '../lib/types';
  import { formatDb } from '../lib/format';
  import ChainRail from '../components/ChainRail.svelte';
  import EffectFaceplate from '../components/EffectFaceplate.svelte';
  import MeterBridge from '../components/MeterBridge.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import Icon from '../components/Icon.svelte';

  interface Props {
    kind: PipelineKind;
  }

  let { kind }: Props = $props();

  let wide = $state(true);
  let meterOpen = $state(false);

  const pipeline = $derived(store.pipelines[kind]);
  const selected = $derived(
    pipeline?.effects.find((effect) => effect.id === store.selectedEffectId) ??
      pipeline?.effects[0] ??
      null,
  );
  const descriptor = $derived(
    selected && store.catalog
      ? (store.catalog.effects.find((effect) => effect.kind === selected.kind) ?? null)
      : null,
  );

  onMount(() => {
    store.selectPipeline(kind);
    const mq = matchMedia('(min-width: 1200px)');
    wide = mq.matches;
    const onChange = () => {
      wide = mq.matches;
    };
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  });
</script>

<div class="chain-screen">
  <ChainRail {kind} />

  <div class="faceplate-col">
    <div class="faceplate-wrap">
      {#if selected}
        {#key selected.id}
          <EffectFaceplate {kind} effect={selected} {descriptor} />
        {/key}
      {:else if pipeline}
        <EmptyState
          fill
          title="No effects in this chain."
          description="Add an effect to start building the chain, or apply a saved preset."
          actionLabel="Add effect"
          onaction={() => (store.addOpen = true)}
          icon="rack"
        />
      {:else}
        <div class="loading-panel">
          <span class="micro">Loading chain…</span>
        </div>
      {/if}
    </div>

    {#if !wide}
      <div class="meter-toggle-wrap">
        <button
          type="button"
          class="meter-toggle"
          aria-expanded={meterOpen}
          onclick={() => (meterOpen = !meterOpen)}
        >
          <Icon name={meterOpen ? 'chevron-down' : 'chevron-up'} size={14} />
          <span>Meters</span>
          <span class="spacer"></span>
          <span class="mono meter-peak">{formatDb(store.meters[kind].peak[0])} dBFS</span>
        </button>
        {#if meterOpen}
          <div class="meter-panel">
            <MeterBridge {kind} layout="wide" />
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#if wide}
    <div class="meter-col">
      <MeterBridge {kind} />
    </div>
  {/if}
</div>

<style>
  .chain-screen {
    display: grid;
    grid-template-columns: var(--chain-w) minmax(0, 1fr) var(--meter-w);
    height: 100%;
    min-height: 0;
    background: var(--chassis);
  }

  .faceplate-col {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    min-width: 0;
    padding: 12px;
    overflow: auto;
  }

  .faceplate-wrap {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .meter-col {
    min-height: 0;
    min-width: 0;
  }

  .loading-panel {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    min-height: 240px;
    background: var(--panel);
    border: 1px solid var(--rule);
    border-radius: var(--r);
  }

  .meter-toggle-wrap {
    flex: none;
    border: 1px solid var(--rule);
    border-radius: var(--r);
    overflow: hidden;
    background: var(--panel);
  }

  .meter-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    background: var(--panel);
    border: 0;
    color: var(--ink-2);
    font-size: 12px;
    font-weight: 500;
    text-align: left;
  }

  .meter-toggle:hover {
    background: var(--panel-sunken);
    color: var(--ink);
  }

  .meter-peak {
    font-size: 10px;
    color: var(--ink-3);
  }

  .meter-panel {
    border-top: 1px solid var(--rule);
  }

  @media (max-width: 1199px) {
    .chain-screen {
      grid-template-columns: var(--chain-w) minmax(0, 1fr);
    }
  }

  @media (max-width: 899px) {
    .chain-screen {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto 1fr;
      height: auto;
      min-height: 100%;
      width: 100%;
      min-width: 0;
    }

    .faceplate-col {
      overflow-x: hidden;
      overflow-y: visible;
      padding: 8px;
    }

    .faceplate-wrap {
      width: 100%;
      min-width: 0;
      min-height: 420px;
    }
  }
</style>
