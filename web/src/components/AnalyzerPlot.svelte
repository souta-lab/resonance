<script lang="ts">
  import { clamp, formatHz, xForFreq } from '../lib/format';

  interface Props {
    spectrum: number[];
    eq?: number[] | null;
    sampleRate?: number;
    height?: number;
    labels?: boolean;
    ariaLabel?: string;
    className?: string;
  }

  let {
    spectrum,
    eq = null,
    sampleRate = 48000,
    height = 180,
    labels = true,
    ariaLabel = 'Spectrum analyzer',
    className = '',
  }: Props = $props();

  const FREQS = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];
  const MINOR_FREQS = [
    30, 40, 60, 70, 80, 90, 300, 400, 600, 700, 800, 900, 3000, 4000, 6000, 7000, 8000, 9000,
  ];
  const DB_LINES = [0, -24, -48, -72, -96];
  const EQ_LINES = [24, 12, 0, -12, -24];

  let width = $state(640);

  const PAD = $derived(
    labels
      ? { left: 30, right: 30, top: 12, bottom: 20 }
      : { left: 6, right: 6, top: 6, bottom: 6 },
  );
  const plotW = $derived(Math.max(40, width - PAD.left - PAD.right));
  const plotH = $derived(Math.max(20, height - PAD.top - PAD.bottom));

  const linear = $derived(spectrum.length > 0 && spectrum.every((v) => v >= 0 && v <= 1.5));
  const isFft = $derived(spectrum.length > 128 && (spectrum.length & (spectrum.length - 1)) === 0);
  const labelFreqs = $derived(plotW < 380 ? [20, 100, 1000, 10000, 20000] : FREQS);

  function toDb(v: number): number {
    if (linear) return 20 * Math.log10(Math.max(v, 1e-6));
    return v;
  }

  function specY(db: number): number {
    return PAD.top + (1 - clamp((db + 96) / 96, 0, 1)) * plotH;
  }

  function eqY(db: number): number {
    return PAD.top + (1 - clamp((db + 24) / 48, 0, 1)) * plotH;
  }

  function px(f: number): number {
    return PAD.left + xForFreq(f, plotW);
  }

  const spectrumPoints = $derived.by(() => {
    const n = spectrum.length;
    const points: { x: number; y: number }[] = [];
    if (!n) return points;
    if (isFft) {
      const nyq = sampleRate / 2;
      const step = Math.max(1, Math.floor(plotW / 220));
      for (let x = 0; x <= plotW; x += step) {
        const f = 20 * Math.pow(1000, x / plotW);
        const bin = clamp(Math.round((f / nyq) * n), 0, n - 1);
        let max = -Infinity;
        for (let k = Math.max(0, bin - 1); k <= Math.min(n - 1, bin + 1); k++) {
          max = Math.max(max, spectrum[k]);
        }
        points.push({ x: PAD.left + x, y: specY(toDb(max)) });
      }
    } else {
      for (let i = 0; i < n; i++) {
        points.push({
          x: PAD.left + (n === 1 ? 0 : (i / (n - 1)) * plotW),
          y: specY(toDb(spectrum[i])),
        });
      }
    }
    return points;
  });

  const spectrumPath = $derived.by(() => {
    if (!spectrumPoints.length) return '';
    return spectrumPoints
      .map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x.toFixed(2)} ${p.y.toFixed(2)}`)
      .join(' ');
  });

  const spectrumFill = $derived.by(() => {
    if (!spectrumPoints.length) return '';
    const first = spectrumPoints[0];
    const last = spectrumPoints[spectrumPoints.length - 1];
    const bottom = PAD.top + plotH;
    return `${spectrumPath} L ${last.x.toFixed(2)} ${bottom.toFixed(2)} L ${first.x.toFixed(2)} ${bottom.toFixed(2)} Z`;
  });

  const eqPath = $derived.by(() => {
    if (!eq || !eq.length) return '';
    return eq
      .map((db, i) => {
        const x = PAD.left + (eq.length === 1 ? 0 : (i / (eq.length - 1)) * plotW);
        return `${i === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${eqY(db).toFixed(2)}`;
      })
      .join(' ');
  });
</script>

<div class="analyzer-plot {className}" bind:clientWidth={width} style:height="{height}px">
  <svg
    width={width}
    height={height}
    viewBox="0 0 {width} {height}"
    role="img"
    aria-label={ariaLabel}
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
        opacity="0.4"
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
    {#each DB_LINES as db}
      <line
        x1={PAD.left}
        y1={specY(db)}
        x2={PAD.left + plotW}
        y2={specY(db)}
        stroke={db === 0 ? 'var(--rule-strong)' : 'var(--rule)'}
        stroke-width={db === 0 ? 1 : 0.8}
      />
    {/each}

    {#if labels}
      {#each labelFreqs as f}
        <text class="axis-label" x={px(f)} y={height - 5} text-anchor="middle">{formatHz(f)}</text>
      {/each}
      {#each DB_LINES as db}
        <text class="axis-label" x={PAD.left - 4} y={specY(db) + 3} text-anchor="end">
          {db}
        </text>
      {/each}
      <text class="axis-unit" x={PAD.left} y={PAD.top - 3} text-anchor="start">dBFS</text>
      {#if eqPath}
        {#each EQ_LINES as db}
          <text class="axis-eq" x={PAD.left + plotW + 4} y={eqY(db) + 3} text-anchor="start">
            {db > 0 ? `+${db}` : db}
          </text>
        {/each}
        <text class="axis-eq" x={PAD.left + plotW} y={PAD.top - 3} text-anchor="end">dB</text>
      {/if}
    {/if}

    {#if spectrumFill}
      <path d={spectrumFill} fill="var(--trace)" opacity="0.16" />
      <path d={spectrumPath} fill="none" stroke="var(--trace)" stroke-width="1.5" />
    {/if}

    {#if eqPath}
      <path d={eqPath} fill="none" stroke="var(--signal)" stroke-width="1.5" />
    {/if}

    {#if !spectrum.length && !eqPath}
      <text
        class="empty-label"
        x={PAD.left + plotW / 2}
        y={PAD.top + plotH / 2}
        text-anchor="middle"
      >
        No spectrum data.
      </text>
    {/if}
  </svg>
</div>

<style>
  .analyzer-plot {
    position: relative;
    width: 100%;
    background: var(--panel-sunken);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
    overflow: hidden;
  }

  svg {
    display: block;
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

  .axis-eq {
    font-family: var(--font-mono);
    font-size: 8px;
    fill: var(--signal);
    letter-spacing: 0.02em;
  }

  .empty-label {
    font-family: var(--font-ui);
    font-size: 11px;
    fill: var(--ink-3);
  }
</style>
