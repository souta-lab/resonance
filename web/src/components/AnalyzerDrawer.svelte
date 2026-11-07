<script lang="ts">
  import { onMount } from 'svelte';
  import { store } from '../lib/store.svelte';
  import type { PipelineKind } from '../lib/types';
  import { eqResponse, formatDb, logFreqs } from '../lib/format';
  import AnalyzerPlot from './AnalyzerPlot.svelte';
  import Icon from './Icon.svelte';
  import Segmented from './Segmented.svelte';

  const sampleRate = $derived(store.status?.dsp.sample_rate ?? 48000);

  const eq = $derived.by(() => {
    const effect = store.analyzerEq;
    if (!effect?.bands) return null;
    const channel = Object.keys(effect.bands)[0];
    if (!channel) return null;
    const bands = effect.bands[channel] ?? [];
    const count = Number(effect.params['num-bands'] ?? bands.length);
    return eqResponse(bands.slice(0, Math.max(0, count)), sampleRate, logFreqs(240, 20, 20000));
  });

  const meter = $derived(store.meters[store.analyzerPipeline]);

  onMount(() => {
    try {
      if (localStorage.getItem('resonance.drawer') === '0') store.drawerOpen = false;
    } catch {
      // Storage unavailable; keep the default open state.
    }
  });

  function toggle(): void {
    store.drawerOpen = !store.drawerOpen;
    try {
      localStorage.setItem('resonance.drawer', store.drawerOpen ? '1' : '0');
    } catch {
      // Storage unavailable; the state still changes for this session.
    }
  }
</script>

<section class="drawer" class:open={store.drawerOpen} aria-label="Analyzer drawer">
  <header class="drawer-head">
    <button
      type="button"
      class="drawer-toggle"
      aria-expanded={store.drawerOpen}
      aria-controls="analyzer-drawer-body"
      onclick={toggle}
    >
      <Icon name={store.drawerOpen ? 'chevron-down' : 'chevron-up'} size={14} />
      <span class="micro">Analyzer</span>
    </button>

    <Segmented
      options={[
        { value: 'output', label: 'Output' },
        { value: 'input', label: 'Input' },
      ]}
      value={store.analyzerPipeline}
      ariaLabel="Analyzer pipeline"
      onchange={(v) => (store.analyzerPipeline = v as PipelineKind)}
    />

    <span class="legend"><span class="swatch trace"></span>Spectrum</span>
    <span class="legend"><span class="swatch signal"></span>EQ</span>
    <span class="spacer"></span>
    <span class="mono readout">PEAK {formatDb(meter.peak[0])} dBFS</span>
    <span class="mono readout dim">{sampleRate / 1000} kHz</span>
  </header>

  {#if store.drawerOpen}
    <div class="drawer-body" id="analyzer-drawer-body">
      <AnalyzerPlot
        spectrum={store.spectrum[store.analyzerPipeline]}
        {eq}
        {sampleRate}
        height={160}
        labels={true}
        ariaLabel="Analyzer spectrum and EQ curve"
      />
    </div>
  {/if}
</section>

<style>
  .drawer {
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border-top: 1px solid var(--rule);
    min-height: 0;
  }

  .drawer-head {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 30px;
    padding: 0 12px;
    flex: none;
  }

  .drawer-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 4px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r);
    color: var(--ink-2);
  }

  .drawer-toggle:hover {
    background: var(--panel-sunken);
    border-color: var(--rule);
    color: var(--ink);
  }

  .legend {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  .swatch {
    width: 12px;
    height: 2px;
    display: inline-block;
  }

  .swatch.trace {
    background: var(--trace);
  }

  .swatch.signal {
    background: var(--signal);
  }

  .readout {
    font-size: 9px;
    color: var(--ink-2);
    letter-spacing: 0.02em;
  }

  .readout.dim {
    color: var(--ink-3);
  }

  .drawer-body {
    padding: 0 12px 10px;
    min-height: 0;
  }

  @media (max-width: 899px) {
    .drawer {
      display: none;
    }
  }
</style>
