<script lang="ts">
  import type { Band } from '../lib/types';
  import {
    clamp,
    dbForY,
    eqResponse,
    formatDb,
    formatHz,
    freqForX,
    logFreqs,
    xForFreq,
    yForDb,
  } from '../lib/format';

  interface Props {
    bands: Band[];
    count: number;
    sampleRate: number;
    selected: number;
    channelLabel: string;
    onselect: (index: number) => void;
    onchange: (bands: Band[]) => void;
  }

  let { bands, count, sampleRate, selected, channelLabel, onselect, onchange }: Props = $props();

  const HEIGHT = 190;
  const PAD = { left: 34, right: 12, top: 12, bottom: 22 };
  const FREQS = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];
  const MINOR_FREQS = [
    30, 40, 60, 70, 80, 90, 300, 400, 600, 700, 800, 900, 3000, 4000, 6000, 7000, 8000, 9000,
  ];
  const DB_LINES = [-24, -18, -12, -6, 0, 6, 12, 18, 24];
  const MINOR_DB = [-21, -15, -9, -3, 3, 9, 15, 21];

  let width = $state(640);
  let svgEl = $state<SVGSVGElement | null>(null);
  let dragIndex = $state<number | null>(null);
  let hoverIndex = $state<number | null>(null);

  const plotW = $derived(Math.max(80, width - PAD.left - PAD.right));
  const plotH = $derived(HEIGHT - PAD.top - PAD.bottom);
  const active = $derived(bands.slice(0, Math.max(0, count)));
  const freqs = $derived(logFreqs(240, 20, 20000));
  const response = $derived(eqResponse(active, sampleRate, freqs));

  const labelFreqs = $derived(plotW < 380 ? [20, 100, 1000, 10000, 20000] : FREQS);

  function px(f: number): number {
    return PAD.left + xForFreq(f, plotW);
  }

  function py(db: number): number {
    return PAD.top + yForDb(db, plotH);
  }

  const curvePath = $derived(
    response
      .map((db, i) => `${i === 0 ? 'M' : 'L'} ${px(freqs[i]).toFixed(2)} ${py(db).toFixed(2)}`)
      .join(' '),
  );

  const fillPath = $derived.by(() => {
    if (!response.length) return '';
    const zero = py(0);
    const first = px(freqs[0]);
    const last = px(freqs[freqs.length - 1]);
    return `${curvePath} L ${last.toFixed(2)} ${zero.toFixed(2)} L ${first.toFixed(2)} ${zero.toFixed(2)} Z`;
  });

  const selectedBand = $derived(selected >= 0 && selected < active.length ? active[selected] : null);

  function pointFor(band: Band): { x: number; y: number } {
    return { x: px(band.frequency), y: py(clamp(band.gain, -24, 24)) };
  }

  function updateBand(index: number, patch: Partial<Band>): void {
    const next = bands.map((b, i) => (i === index ? { ...b, ...patch } : b));
    onchange(next);
  }

  function onPointDown(e: PointerEvent, index: number): void {
    e.stopPropagation();
    onselect(index);
    dragIndex = index;
    svgEl?.setPointerCapture(e.pointerId);
  }

  function onSvgMove(e: PointerEvent): void {
    if (dragIndex === null || !svgEl) return;
    const rect = svgEl.getBoundingClientRect();
    const x = clamp(e.clientX - rect.left - PAD.left, 0, plotW);
    const y = clamp(e.clientY - rect.top - PAD.top, 0, plotH);
    const frequency = Math.round(clamp(freqForX(x, plotW), 20, 20000));
    const gain = Math.round(clamp(dbForY(y, plotH), -24, 24) * 10) / 10;
    updateBand(dragIndex, { frequency, gain });
  }

  function onSvgUp(e: PointerEvent): void {
    if (dragIndex === null) return;
    dragIndex = null;
    if (svgEl?.hasPointerCapture(e.pointerId)) svgEl.releasePointerCapture(e.pointerId);
  }

  function onPointDoubleClick(e: MouseEvent, index: number): void {
    e.preventDefault();
    e.stopPropagation();
    updateBand(index, { gain: 0, q: 1.41 });
  }

  function wheelAction(node: SVGSVGElement) {
    const handler = (e: WheelEvent) => {
      if (!active.length) return;
      const rect = node.getBoundingClientRect();
      const x = e.clientX - rect.left - PAD.left;
      let target = selected;
      let best = Infinity;
      for (let i = 0; i < active.length; i++) {
        const d = Math.abs(px(active[i].frequency) - PAD.left - x);
        if (d < best) {
          best = d;
          target = i;
        }
      }
      if (best > 48) return;
      e.preventDefault();
      const band = active[target];
      if (!band) return;
      const q = Math.round(clamp(band.q * Math.exp(-e.deltaY * 0.0015), 0.1, 10) * 100) / 100;
      onselect(target);
      updateBand(target, { q });
    };
    node.addEventListener('wheel', handler, { passive: false });
    return {
      destroy() {
        node.removeEventListener('wheel', handler);
      },
    };
  }
</script>

<div class="eq-graph" bind:clientWidth={width}>
  <svg
    bind:this={svgEl}
    width={width}
    height={HEIGHT}
    viewBox="0 0 {width} {HEIGHT}"
    role="img"
    aria-label={`EQ response for ${channelLabel}, ${active.length} bands`}
    onpointermove={onSvgMove}
    onpointerup={onSvgUp}
    onpointercancel={onSvgUp}
    use:wheelAction
  >
    <rect
      x={PAD.left}
      y={PAD.top}
      width={plotW}
      height={plotH}
      fill="var(--panel-sunken)"
      stroke="var(--rule-strong)"
      stroke-width="1"
    />

    {#each MINOR_FREQS as f}
      <line
        x1={px(f)}
        y1={PAD.top}
        x2={px(f)}
        y2={PAD.top + plotH}
        stroke="var(--rule)"
        stroke-width="0.6"
        opacity="0.45"
      />
    {/each}
    {#each FREQS as f}
      <line
        x1={px(f)}
        y1={PAD.top}
        x2={px(f)}
        y2={PAD.top + plotH}
        stroke="var(--rule)"
        stroke-width="0.8"
      />
    {/each}
    {#each MINOR_DB as db}
      <line
        x1={PAD.left}
        y1={py(db)}
        x2={PAD.left + plotW}
        y2={py(db)}
        stroke="var(--rule)"
        stroke-width="0.6"
        opacity="0.45"
      />
    {/each}
    {#each DB_LINES as db}
      <line
        x1={PAD.left}
        y1={py(db)}
        x2={PAD.left + plotW}
        y2={py(db)}
        stroke={db === 0 ? 'var(--rule-strong)' : 'var(--rule)'}
        stroke-width={db === 0 ? 1 : 0.8}
      />
    {/each}

    {#each labelFreqs as f}
      <text class="axis-label" x={px(f)} y={HEIGHT - 6} text-anchor="middle">{formatHz(f)}</text>
    {/each}
    {#each DB_LINES as db}
      <text class="axis-label" x={PAD.left - 6} y={py(db) + 3} text-anchor="end">
        {db > 0 ? `+${db}` : db}
      </text>
    {/each}
    <text class="axis-unit" x={PAD.left + plotW} y={PAD.top - 3} text-anchor="end">dB</text>

    {#if active.length}
      <path d={fillPath} fill="var(--signal)" opacity="0.07" />
      <path d={curvePath} fill="none" stroke="var(--signal)" stroke-width="1.5" />
    {/if}

    {#each active as band, index (index)}
      {@const p = pointFor(band)}
      <g
        class="band-point"
        class:selected={index === selected}
        class:muted={band.mute}
        aria-hidden="true"
        onpointerdown={(e) => onPointDown(e, index)}
        onpointerenter={() => (hoverIndex = index)}
        onpointerleave={() => (hoverIndex = null)}
        ondblclick={(e) => onPointDoubleClick(e, index)}
      >
        <title>
          Band {index + 1}: {formatHz(band.frequency)} Hz, {formatDb(band.gain)} dB, Q {band.q.toFixed(2)}{band.mute ? ', muted' : ''}
        </title>
        {#if index === selected}
          <circle cx={p.x} cy={p.y} r="9" fill="none" stroke="var(--signal)" stroke-width="0.8" opacity="0.6" />
        {/if}
        <circle
          cx={p.x}
          cy={p.y}
          r={index === selected ? 6 : 5}
          fill={index === selected ? 'var(--signal)' : 'var(--panel)'}
          stroke={band.mute ? 'var(--ink-3)' : 'var(--signal)'}
          stroke-width="1.5"
          stroke-dasharray={band.mute ? '2 2' : undefined}
        />
        {#if index === selected || index === hoverIndex}
          <text class="band-label" x={p.x + 9} y={p.y - 8}>{index + 1}</text>
        {/if}
      </g>
    {/each}

    {#if !active.length}
      <text class="empty-label" x={PAD.left + plotW / 2} y={PAD.top + plotH / 2} text-anchor="middle">
        No bands on this channel.
      </text>
    {/if}
  </svg>

  {#if selectedBand}
    <div class="band-readout mono">
      <span class="band-index">B{selected + 1}</span>
      <span>{formatHz(selectedBand.frequency)} Hz</span>
      <span>{formatDb(selectedBand.gain)} dB</span>
      <span>Q {selectedBand.q.toFixed(2)}</span>
    </div>
  {/if}
</div>

<style>
  .eq-graph {
    position: relative;
    width: 100%;
    min-height: 190px;
    background: var(--panel-sunken);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
    overflow: hidden;
  }

  svg {
    display: block;
    touch-action: none;
    cursor: crosshair;
  }

  .axis-label {
    font-family: var(--font-mono);
    font-size: 8px;
    fill: var(--ink-3);
    letter-spacing: 0.02em;
  }

  .axis-unit {
    font-family: var(--font-mono);
    font-size: 8px;
    fill: var(--ink-3);
    letter-spacing: 0.04em;
  }

  .band-point {
    cursor: grab;
  }

  .band-point:active {
    cursor: grabbing;
  }

  .band-label {
    font-family: var(--font-mono);
    font-size: 9px;
    fill: var(--ink);
    paint-order: stroke;
    stroke: var(--panel-sunken);
    stroke-width: 2.5px;
  }

  .empty-label {
    font-family: var(--font-ui);
    font-size: 11px;
    fill: var(--ink-3);
  }

  .band-readout {
    position: absolute;
    top: 8px;
    right: 10px;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 2px 6px;
    background: var(--panel);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    font-size: 9px;
    color: var(--ink-2);
    pointer-events: none;
  }

  .band-index {
    color: var(--signal);
    font-weight: 500;
  }
</style>
