<script lang="ts">
  import { onMount } from 'svelte';
  import { store, pipelineLabel } from '../lib/store.svelte';
  import type { PipelineKind } from '../lib/types';
  import { clamp, formatDb } from '../lib/format';
  import AnalyzerPlot from './AnalyzerPlot.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    kind: PipelineKind;
    layout?: 'rail' | 'wide';
  }

  let { kind, layout = 'rail' }: Props = $props();

  const SEGMENTS = 30;
  const thresholds = Array.from({ length: SEGMENTS }, (_, i) => -60 + (i + 1) * 2);
  const scaleMarks = [0, -6, -12, -24, -36, -48, -60];

  let disp = $state({
    peak: [-60, -60] as [number, number],
    gr: 0,
  });
  let hold = $state<[number, number]>([-60, -60]);
  let holdAt: [number, number] = [0, 0];
  let raf = 0;
  let last = 0;

  const label = $derived(pipelineLabel(kind));
  const hasData = $derived(store.meters[kind].updated > 0);

  function pct(db: number): number {
    return clamp((db + 60) / 60, 0, 1) * 100;
  }

  function resetHold(): void {
    hold = [disp.peak[0], disp.peak[1]];
    holdAt = [performance.now(), performance.now()];
  }

  function tick(now: number): void {
    if (document.hidden) {
      raf = 0;
      return;
    }
    if (now - last < 33) {
      raf = requestAnimationFrame(tick);
      return;
    }
    last = now;
    const target = store.meters[kind];
    const reduce =
      typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
    for (let ch = 0; ch < 2; ch++) {
      const p = target.peak[ch];
      if (reduce) {
        disp.peak[ch] = p;
      } else {
        disp.peak[ch] = p > disp.peak[ch] ? p : disp.peak[ch] + (p - disp.peak[ch]) * 0.14;
      }
      if (p > hold[ch]) {
        hold[ch] = p;
        holdAt[ch] = now;
      } else if (now - holdAt[ch] > 1500) {
        hold[ch] = Math.max(p, hold[ch] - 0.35);
      }
    }
    if (reduce) disp.gr = target.gr;
    else disp.gr = target.gr < disp.gr ? target.gr : disp.gr + (target.gr - disp.gr) * 0.16;
    raf = requestAnimationFrame(tick);
  }

  function start(): void {
    if (!raf) raf = requestAnimationFrame(tick);
  }

  onMount(() => {
    start();
    const onVisibility = () => {
      if (document.hidden) {
        cancelAnimationFrame(raf);
        raf = 0;
      } else {
        start();
      }
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      document.removeEventListener('visibilitychange', onVisibility);
      cancelAnimationFrame(raf);
      raf = 0;
    };
  });

  function value(v: number): string {
    return hasData ? formatDb(v) : '—';
  }
</script>

<section class="meter-bridge" class:wide={layout === 'wide'} aria-label={`${label} meter bridge`}>
  <header class="mb-head">
    <span class="micro">Meter bridge</span>
    <span class="badge">{label}</span>
    <span class="spacer"></span>
    <button
      type="button"
      class="icon-btn"
      aria-label="Reset peak hold"
      title="Reset peak hold"
      onclick={resetHold}
    >
      <Icon name="refresh" size={14} />
    </button>
  </header>

  <div class="mb-body">
    <div class="ladders-row">
      <div class="scale" aria-hidden="true">
        <span class="scale-spacer"></span>
        <div class="scale-track">
          {#each scaleMarks as db}
            <span class="scale-mark" style:bottom="{pct(db)}%">
              <span class="scale-tick"></span>
              <span class="scale-text mono">{db}</span>
            </span>
          {/each}
        </div>
      </div>

      {#each [0, 1] as ch}
        <div class="ladder-col">
          <span class="ladder-label mono">{ch === 0 ? 'L' : 'R'}</span>
          <div
            class="ladder"
            role="meter"
            aria-label={`${label} ${ch === 0 ? 'left' : 'right'} peak level`}
            aria-valuemin={-60}
            aria-valuemax={0}
            aria-valuenow={Number(disp.peak[ch].toFixed(1))}
            aria-valuetext={`${value(disp.peak[ch])} dBFS`}
          >
            {#each thresholds as threshold}
              <span
                class="seg"
                class:lit={disp.peak[ch] >= threshold}
                class:warn={threshold > -18}
                class:hot={threshold > -6}
              ></span>
            {/each}
            <span class="hold-tick" style:bottom="{pct(hold[ch])}%"></span>
          </div>
        </div>
      {/each}
    </div>

    <div class="readouts">
      <div class="readout-head">
        <span class="micro">Levels · dBFS</span>
        <span class="micro">L</span>
        <span class="micro">R</span>
      </div>
      <div class="readout-row">
        <span class="rlabel">Peak</span>
        <span class="rval mono">{value(store.meters[kind].peak[0])}</span>
        <span class="rval mono">{value(store.meters[kind].peak[1])}</span>
      </div>
      <div class="readout-row">
        <span class="rlabel">True pk</span>
        <span class="rval mono">{value(store.meters[kind].truePeak[0])}</span>
        <span class="rval mono">{value(store.meters[kind].truePeak[1])}</span>
      </div>
      <div class="readout-row">
        <span class="rlabel">RMS</span>
        <span class="rval mono">{value(store.meters[kind].rms[0])}</span>
        <span class="rval mono">{value(store.meters[kind].rms[1])}</span>
      </div>
      <div class="readout-row">
        <span class="rlabel">LUFS-M</span>
        <span class="rval mono span2">{value(store.meters[kind].lufsM)}</span>
      </div>
      <div class="readout-row">
        <span class="rlabel">LUFS-S</span>
        <span class="rval mono span2">{value(store.meters[kind].lufsS)}</span>
      </div>
    </div>

    <div class="side">
      <div class="gr-block">
        <div class="gr-head">
          <span class="micro">Gain reduction</span>
          <span class="mono gr-value">{hasData ? `${formatDb(disp.gr)} dB` : '—'}</span>
        </div>
        <div
          class="gr-track"
          role="meter"
          aria-label={`${label} gain reduction`}
          aria-valuemin={-24}
          aria-valuemax={0}
          aria-valuenow={Number(disp.gr.toFixed(1))}
          aria-valuetext={`${formatDb(disp.gr)} dB`}
        >
          <span class="gr-fill" style:width="{clamp(Math.abs(disp.gr) / 24, 0, 1) * 100}%"></span>
          <span class="gr-zero mono">0</span>
          <span class="gr-min mono">-24</span>
        </div>
      </div>

      <div class="spectrum-block">
        <div class="spectrum-head">
          <span class="micro">Spectrum</span>
          <span class="micro">20 Hz — 20 kHz</span>
        </div>
        <AnalyzerPlot
          spectrum={store.spectrum[kind]}
          height={layout === 'wide' ? 84 : 58}
          labels={false}
          ariaLabel={`${label} spectrum strip`}
        />
      </div>
    </div>
  </div>
</section>

<style>
  .meter-bridge {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    background: var(--panel);
    border-left: 1px solid var(--rule);
  }

  .meter-bridge.wide {
    border-left: 0;
    border-top: 1px solid var(--rule);
    height: auto;
  }

  .mb-head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border-bottom: 1px solid var(--rule);
    flex: none;
  }

  .mb-body {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 14px 12px;
    overflow: auto;
    min-height: 0;
  }

  .wide .mb-body {
    display: grid;
    grid-template-columns: 190px minmax(220px, 1fr) minmax(220px, 300px);
    gap: 20px;
    align-items: start;
  }

  .ladders-row {
    display: flex;
    gap: 12px;
    justify-content: center;
  }

  .scale {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 28px;
    flex: none;
  }

  .scale-spacer {
    height: 12px;
  }

  .scale-track {
    position: relative;
    height: 180px;
  }

  .scale-mark {
    position: absolute;
    right: 0;
    display: flex;
    align-items: center;
    gap: 3px;
    transform: translateY(50%);
  }

  .scale-tick {
    width: 4px;
    height: 1px;
    background: var(--rule-strong);
  }

  .scale-text {
    font-size: 8px;
    color: var(--ink-3);
    line-height: 1;
  }

  .ladder-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .ladder-label {
    font-size: 9px;
    color: var(--ink-2);
    height: 12px;
    line-height: 12px;
  }

  .ladder {
    position: relative;
    display: flex;
    flex-direction: column-reverse;
    gap: 2px;
    width: 22px;
    height: 180px;
    padding: 2px;
    background: var(--panel-sunken);
    border: 1px solid var(--rule-strong);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
  }

  .seg {
    flex: 1;
    min-height: 2px;
    background: color-mix(in srgb, var(--panel-sunken) 55%, var(--ink) 12%);
    border-radius: 0;
  }

  .seg.lit {
    background: var(--ok);
    box-shadow: 0 0 4px color-mix(in srgb, var(--ok) 55%, transparent);
  }

  .seg.lit.warn {
    background: var(--signal);
    box-shadow: 0 0 4px color-mix(in srgb, var(--signal) 55%, transparent);
  }

  .seg.lit.hot {
    background: var(--alert);
    box-shadow: 0 0 5px color-mix(in srgb, var(--alert) 60%, transparent);
  }

  .hold-tick {
    position: absolute;
    left: -3px;
    right: -3px;
    height: 2px;
    background: var(--ink);
    z-index: 2;
    pointer-events: none;
  }

  .readouts {
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-top: 1px solid var(--rule);
    padding-top: 8px;
  }

  .readout-head,
  .readout-row {
    display: grid;
    grid-template-columns: 1fr 54px 54px;
    align-items: baseline;
    gap: 4px;
  }

  .readout-head {
    padding-bottom: 3px;
    border-bottom: 1px solid var(--rule);
    margin-bottom: 2px;
  }

  .readout-head .micro:nth-child(2),
  .readout-head .micro:nth-child(3) {
    text-align: right;
  }

  .readout-row {
    min-height: 18px;
  }

  .rlabel {
    font-size: 11px;
    color: var(--ink-2);
  }

  .rval {
    font-size: 11px;
    color: var(--ink);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .rval.span2 {
    grid-column: 2 / 4;
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .gr-block {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .gr-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }

  .gr-value {
    font-size: 10px;
    color: var(--ink-2);
  }

  .gr-track {
    position: relative;
    height: 8px;
    background: var(--panel-sunken);
    border: 1px solid var(--rule-strong);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
    overflow: hidden;
  }

  .gr-fill {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    background: var(--signal);
    transition: width 80ms linear;
  }

  .gr-zero,
  .gr-min {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    font-size: 7px;
    color: var(--ink-3);
    pointer-events: none;
  }

  .gr-zero {
    left: 3px;
  }

  .gr-min {
    right: 3px;
  }

  .spectrum-block {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .spectrum-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }

  @media (max-width: 1199px) {
    .wide .mb-body {
      grid-template-columns: 180px minmax(200px, 1fr) minmax(200px, 280px);
    }
  }

  @media (max-width: 899px) {
    .wide .mb-body {
      grid-template-columns: 1fr;
    }

    .ladders-row {
      justify-content: flex-start;
    }
  }
</style>
