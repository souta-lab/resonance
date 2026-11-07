<script lang="ts">
  import { store } from '../lib/store.svelte';
  import type { PipelineKind } from '../lib/types';
  import { clamp, eqResponse, formatDb, logFreqs } from '../lib/format';
  import AnalyzerPlot from '../components/AnalyzerPlot.svelte';
  import BandTable from '../components/BandTable.svelte';
  import Segmented from '../components/Segmented.svelte';
  import EmptyState from '../components/EmptyState.svelte';

  let eqId = $state<string | null>(null);
  let channel = $state(0);
  let selectedBand = $state(0);
  let historyWidth = $state(600);

  const sampleRate = $derived(store.status?.dsp.sample_rate ?? 48000);
  const pipeline = $derived(store.pipelines[store.analyzerPipeline]);
  const eqEffects = $derived(pipeline?.effects.filter((effect) => !!effect.bands) ?? []);
  const selectedEq = $derived(eqEffects.find((effect) => effect.id === eqId) ?? eqEffects[0] ?? null);
  const descriptor = $derived(
    selectedEq && store.catalog
      ? (store.catalog.effects.find((effect) => effect.kind === selectedEq.kind) ?? null)
      : null,
  );

  const channelLabels = $derived(descriptor?.bands?.channel_labels ?? []);
  const channelLabel = $derived(channelLabels[channel] ?? channelLabels[0] ?? 'left');
  const bands = $derived(selectedEq?.bands?.[channelLabel] ?? []);
  const bandCount = $derived(
    Math.max(0, Math.min(Number(selectedEq?.params['num-bands'] ?? bands.length), bands.length)),
  );
  const safeSelected = $derived(Math.min(selectedBand, Math.max(0, bandCount - 1)));

  const eqCurve = $derived.by(() => {
    if (!selectedEq) return null;
    const list = selectedEq.bands?.[channelLabel] ?? [];
    const count = Math.max(0, Math.min(Number(selectedEq.params['num-bands'] ?? list.length), list.length));
    return eqResponse(list.slice(0, count), sampleRate, logFreqs(240, 20, 20000));
  });

  const history = $derived(store.history[store.analyzerPipeline]);
  const meter = $derived(store.meters[store.analyzerPipeline]);

  const channelOptions = $derived(
    channelLabels.map((label, index) => ({
      value: index,
      label: label.charAt(0).toUpperCase() + label.slice(1),
    })),
  );

  const eqOptions = $derived(
    eqEffects.map((effect) => ({
      value: effect.id,
      label: `${effect.name} · slot ${String(effect.index + 1).padStart(2, '0')}`,
    })),
  );

  function setPipeline(value: string | number): void {
    store.analyzerPipeline = value as PipelineKind;
    eqId = null;
    channel = 0;
    selectedBand = 0;
  }

  function setEq(e: Event): void {
    eqId = (e.currentTarget as HTMLSelectElement).value;
    channel = 0;
    selectedBand = 0;
  }

  function setChannel(value: string | number): void {
    channel = Number(value);
    selectedBand = 0;
  }

  function patchBands(next: typeof bands): void {
    if (!selectedEq) return;
    store.patchBands(store.analyzerPipeline, selectedEq.id, channelLabel, next);
  }

  function addEqualizer(): void {
    store.selectPipeline(store.analyzerPipeline);
    store.addOpen = true;
  }

  function historyPath(values: number[], min: number, max: number, width: number, height: number): string {
    if (values.length < 2) return '';
    return values
      .map((value, index) => {
        const x = (index / (values.length - 1)) * width;
        const y = height - ((clamp(value, min, max) - min) / (max - min)) * height;
        return `${index === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`;
      })
      .join(' ');
  }
</script>

<div class="analyzer-screen">
  <div class="screen-toolbar">
    <span class="micro">Analyzer</span>
    <Segmented
      options={[
        { value: 'output', label: 'Output' },
        { value: 'input', label: 'Input' },
      ]}
      value={store.analyzerPipeline}
      ariaLabel="Analyzer pipeline"
      onchange={setPipeline}
    />
    <span class="spacer"></span>
    <span class="legend"><span class="swatch trace"></span>Spectrum</span>
    <span class="legend"><span class="swatch signal"></span>EQ</span>
    <span class="mono toolbar-readout">
      PEAK {formatDb(meter.peak[0])} · LUFS-M {formatDb(meter.lufsM)}
    </span>
  </div>

  <section class="plot-panel panel" aria-label="Spectrum and EQ">
    <header class="panel-head">
      <span class="micro">Spectrum + EQ</span>
      <span class="mono panel-meta">{sampleRate / 1000} kHz · 20 Hz — 20 kHz</span>
      <span class="spacer"></span>
      {#if eqOptions.length > 1}
        <label class="eq-select-label" for="analyzer-eq">EQ</label>
        <select id="analyzer-eq" class="select" value={selectedEq?.id ?? ''} onchange={setEq}>
          {#each eqOptions as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      {/if}
      {#if channelOptions.length > 1}
        <Segmented
          options={channelOptions}
          value={channel}
          ariaLabel="Analyzer EQ channel"
          onchange={setChannel}
        />
      {/if}
    </header>
    <div class="plot-body">
      <AnalyzerPlot
        spectrum={store.spectrum[store.analyzerPipeline]}
        eq={eqCurve}
        {sampleRate}
        height={320}
        labels={true}
        ariaLabel="Large spectrum and EQ response"
      />
    </div>
  </section>

  <div class="lower-grid">
  <section class="history-panel panel" aria-label="Level history">
    <header class="panel-head">
      <span class="micro">History</span>
      <span class="mono panel-meta">last 24 s</span>
      <span class="spacer"></span>
      <span class="micro">Peak dBFS</span>
      <span class="micro">LUFS-M</span>
    </header>
    <div class="history-body">
      <div class="lane">
        <span class="lane-label micro">Peak</span>
        <div class="lane-plot" bind:clientWidth={historyWidth}>
          <svg width={historyWidth} height="44" viewBox="0 0 {historyWidth} 44" aria-hidden="true">
            <rect x="0" y="0" width={historyWidth} height="44" fill="var(--panel-sunken)" />
            {#each [0, -12, -24, -36, -48, -60] as db}
              <line
                x1="0"
                y1={44 - ((db + 60) / 60) * 44}
                x2={historyWidth}
                y2={44 - ((db + 60) / 60) * 44}
                stroke="var(--rule)"
                stroke-width="0.6"
                opacity="0.6"
              />
            {/each}
            {#if history.peak.length > 1}
              <path
                d={historyPath(history.peak, -60, 0, historyWidth, 44)}
                fill="none"
                stroke="var(--trace)"
                stroke-width="1.5"
              />
            {/if}
          </svg>
          {#if history.peak.length < 2}
            <span class="lane-empty micro">Waiting for meter data.</span>
          {/if}
        </div>
        <span class="lane-value mono">{formatDb(meter.peak[0])}</span>
      </div>

      <div class="lane">
        <span class="lane-label micro">LUFS-M</span>
        <div class="lane-plot" bind:clientWidth={historyWidth}>
          <svg width={historyWidth} height="44" viewBox="0 0 {historyWidth} 44" aria-hidden="true">
            <rect x="0" y="0" width={historyWidth} height="44" fill="var(--panel-sunken)" />
            {#each [0, -10, -20, -30, -40] as db}
              <line
                x1="0"
                y1={44 - ((db + 40) / 40) * 44}
                x2={historyWidth}
                y2={44 - ((db + 40) / 40) * 44}
                stroke="var(--rule)"
                stroke-width="0.6"
                opacity="0.6"
              />
            {/each}
            {#if history.lufs.length > 1}
              <path
                d={historyPath(history.lufs, -40, 0, historyWidth, 44)}
                fill="none"
                stroke="var(--signal)"
                stroke-width="1.5"
              />
            {/if}
          </svg>
          {#if history.lufs.length < 2}
            <span class="lane-empty micro">Waiting for meter data.</span>
          {/if}
        </div>
        <span class="lane-value mono">{formatDb(meter.lufsM)}</span>
      </div>
    </div>
  </section>

  <section class="bands-panel panel" aria-label="EQ bands">
    <header class="panel-head">
      <span class="micro">EQ bands</span>
      {#if selectedEq}
        <span class="mono panel-meta">
          {selectedEq.name} · slot {String(selectedEq.index + 1).padStart(2, '0')} · {bandCount} bands
        </span>
      {/if}
      <span class="spacer"></span>
      {#if channelOptions.length > 1}
        <Segmented
          options={channelOptions}
          value={channel}
          ariaLabel="Band table channel"
          onchange={setChannel}
        />
      {/if}
    </header>
    <div class="bands-body">
      {#if selectedEq && descriptor?.bands}
        <BandTable
          {bands}
          count={bandCount}
          fields={descriptor.bands.fields}
          selected={safeSelected}
          {channelLabel}
          onselect={(index) => (selectedBand = index)}
          onchange={patchBands}
        />
      {:else}
        <EmptyState
          title="No equalizer in this chain."
          description={`Add an equalizer to the ${store.analyzerPipeline} chain to edit bands here.`}
          actionLabel="Add equalizer"
          onaction={addEqualizer}
          icon="analyzer"
        />
      {/if}
    </div>
  </section>
  </div>
</div>

<style>
  .analyzer-screen {
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

  .toolbar-readout {
    font-size: 9px;
    color: var(--ink-2);
  }

  .plot-panel,
  .history-panel,
  .bands-panel {
    display: flex;
    flex-direction: column;
    flex: none;
  }

  .lower-grid {
    display: grid;
    grid-template-columns: minmax(320px, 0.9fr) minmax(0, 1.4fr);
    gap: 12px;
    align-items: start;
  }

  .panel-head {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 34px;
    padding: 0 12px;
    border-bottom: 1px solid var(--rule);
    flex-wrap: wrap;
  }

  .panel-meta {
    font-size: 9px;
    color: var(--ink-3);
  }

  .eq-select-label {
    font-size: 10px;
    color: var(--ink-2);
  }

  .plot-body {
    padding: 10px;
  }

  .history-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
  }

  .lane {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .lane-label {
    width: 46px;
    flex: none;
    font-size: 9px;
  }

  .lane-plot {
    position: relative;
    flex: 1;
    min-width: 0;
    border: 1px solid var(--rule);
    border-radius: var(--r);
    overflow: hidden;
    background: var(--panel-sunken);
  }

  .lane-plot svg {
    display: block;
  }

  .lane-empty {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    color: var(--ink-3);
  }

  .lane-value {
    width: 48px;
    flex: none;
    text-align: right;
    font-size: 10px;
    color: var(--ink-2);
  }

  .bands-body {
    padding: 10px;
  }

  @media (max-width: 899px) {
    .lower-grid {
      grid-template-columns: 1fr;
    }

    .analyzer-screen {
      height: auto;
      min-height: 100%;
      overflow: visible;
      padding: 8px;
    }

    .lane-label {
      width: 40px;
    }

    .lane-value {
      width: 42px;
    }
  }
</style>
