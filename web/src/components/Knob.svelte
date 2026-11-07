<script lang="ts">
  import { clamp, formatDb, formatHz } from '../lib/format';
  import type { Unit } from '../lib/types';

  interface Props {
    value: number;
    min: number;
    max: number;
    step: number;
    defaultValue: number;
    unit: Unit;
    label: string;
    ariaLabel?: string;
    log?: boolean;
    disabled?: boolean;
    size?: number;
    format?: (value: number) => { n: string; u: string };
    onchange?: (value: number) => void;
  }

  let {
    value,
    min,
    max,
    step,
    defaultValue,
    unit,
    label,
    ariaLabel,
    log = false,
    disabled = false,
    size = 56,
    format,
    onchange,
  }: Props = $props();

  // The effect below keeps this in sync; the initial value avoids a first-paint jump.
  // svelte-ignore state_referenced_locally
  let live = $state(value);
  let dragging = $state(false);
  let startY = 0;
  let startNorm = 0;
  let pointerId = -1;

  $effect(() => {
    if (!dragging) live = value;
  });

  const cx = $derived(size / 2);
  const cy = $derived(size / 2);
  const arcR = $derived(size / 2 - 6);
  const capR = $derived(size / 2 - 14);

  function toNorm(v: number): number {
    if (log && min > 0) {
      return Math.log(clamp(v, min, max) / min) / Math.log(max / min);
    }
    return (clamp(v, min, max) - min) / (max - min);
  }

  function fromNorm(n: number): number {
    const t = clamp(n, 0, 1);
    if (log && min > 0) return min * Math.pow(max / min, t);
    return min + t * (max - min);
  }

  function quantize(v: number): number {
    if (!Number.isFinite(v)) return min;
    const q = Math.round((v - min) / step) * step + min;
    return clamp(Number(q.toFixed(6)), min, max);
  }

  function polar(r: number, deg: number): { x: number; y: number } {
    const rad = ((deg - 90) * Math.PI) / 180;
    return { x: cx + r * Math.cos(rad), y: cy + r * Math.sin(rad) };
  }

  function arcPath(r: number, startDeg: number, endDeg: number): string {
    const start = polar(r, startDeg);
    const end = polar(r, endDeg);
    const large = Math.abs(endDeg - startDeg) > 180 ? 1 : 0;
    return `M ${start.x.toFixed(2)} ${start.y.toFixed(2)} A ${r} ${r} 0 ${large} 1 ${end.x.toFixed(2)} ${end.y.toFixed(2)}`;
  }

  const norm = $derived(toNorm(live));
  const endAngle = $derived(-135 + 270 * norm);
  const trackPath = $derived(arcPath(arcR, -135, 135));
  const valuePath = $derived(norm > 0.002 ? arcPath(arcR, -135, endAngle) : '');
  const pointer = $derived(polar(capR - 3, endAngle));

  const ticks = $derived(
    Array.from({ length: 11 }, (_, i) => {
      const deg = -135 + (270 * i) / 10;
      const major = i === 0 || i === 5 || i === 10;
      const inner = polar(arcR + 3, deg);
      const outer = polar(arcR + (major ? 6 : 4.5), deg);
      return { x1: inner.x, y1: inner.y, x2: outer.x, y2: outer.y, major };
    }),
  );

  function defaultFormat(v: number): { n: string; u: string } {
    switch (unit) {
      case 'db':
        return { n: formatDb(v), u: 'dB' };
      case 'hz':
        return { n: formatHz(v), u: 'Hz' };
      case 'ms':
        return { n: v >= 100 ? String(Math.round(v)) : v.toFixed(1), u: 'ms' };
      case 'percent':
        return { n: String(Math.round(v)), u: '%' };
      case 'ratio':
        return { n: v.toFixed(1), u: ':1' };
      case 'seconds':
        return { n: v.toFixed(2), u: 's' };
      case 'octaves':
        return { n: v.toFixed(1), u: 'oct' };
      case 'degrees':
        return { n: String(Math.round(v)), u: '°' };
      case 'count':
        return { n: String(Math.round(v)), u: '' };
      case 'semitones':
        return { n: `${v > 0 ? '+' : ''}${v.toFixed(2)}`, u: 'st' };
      case 'cents':
        return { n: `${v > 0 ? '+' : ''}${Math.round(v)}`, u: 'ct' };
      case 'bpm':
        return { n: String(Math.round(v)), u: 'BPM' };
      case 'db_per_oct':
        return { n: v.toFixed(1), u: 'dB/oct' };
      default:
        return { n: v.toFixed(2), u: '' };
    }
  }

  const display = $derived(format ? format(live) : defaultFormat(live));
  const valueText = $derived(`${display.n}${display.u ? ` ${display.u}` : ''}`);

  function commit(next: number): void {
    const q = quantize(next);
    if (q === live) return;
    live = q;
    onchange?.(q);
  }

  function onPointerDown(e: PointerEvent): void {
    if (disabled || e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    pointerId = e.pointerId;
    dragging = true;
    startY = e.clientY;
    startNorm = toNorm(live);
    (e.currentTarget as HTMLElement).focus();
  }

  function onPointerMove(e: PointerEvent): void {
    if (!dragging || e.pointerId !== pointerId) return;
    const dy = startY - e.clientY;
    const fine = e.shiftKey ? 0.1 : 1;
    const nextNorm = clamp(startNorm + (dy / 180) * fine, 0, 1);
    commit(fromNorm(nextNorm));
  }

  function onPointerUp(e: PointerEvent): void {
    if (e.pointerId !== pointerId) return;
    dragging = false;
    pointerId = -1;
    const el = e.currentTarget as HTMLElement;
    if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
  }

  function onKeyDown(e: KeyboardEvent): void {
    if (disabled) return;
    const fine = e.shiftKey ? 0.1 : 1;
    let next = live;
    switch (e.key) {
      case 'ArrowUp':
      case 'ArrowRight':
        next = log && min > 0 ? fromNorm(toNorm(live) + 0.005 * fine) : live + step * fine;
        break;
      case 'ArrowDown':
      case 'ArrowLeft':
        next = log && min > 0 ? fromNorm(toNorm(live) - 0.005 * fine) : live - step * fine;
        break;
      case 'PageUp':
        next = log && min > 0 ? fromNorm(toNorm(live) + 0.05) : live + step * 10;
        break;
      case 'PageDown':
        next = log && min > 0 ? fromNorm(toNorm(live) - 0.05) : live - step * 10;
        break;
      case 'Home':
        next = min;
        break;
      case 'End':
        next = max;
        break;
      default:
        return;
    }
    e.preventDefault();
    commit(next);
  }

  function onDoubleClick(): void {
    if (disabled) return;
    commit(defaultValue);
  }
</script>

<div
  class="knob-field"
  class:disabled
  style:width="{size + 20}px"
  role="slider"
  tabindex={disabled ? -1 : 0}
  aria-label={ariaLabel ?? label}
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={Number(live.toFixed(4))}
  aria-valuetext={valueText}
  aria-disabled={disabled}
  title="Drag vertically. Shift for fine. Double-click to reset."
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onkeydown={onKeyDown}
  ondblclick={onDoubleClick}
>
  <span class="knob-label">{label}</span>
  <svg class="knob-svg" width={size} height={size} viewBox="0 0 {size} {size}" aria-hidden="true">
    {#each ticks as tick}
      <line
        x1={tick.x1}
        y1={tick.y1}
        x2={tick.x2}
        y2={tick.y2}
        stroke="var(--rule-strong)"
        stroke-width={tick.major ? 1.2 : 0.8}
        opacity={tick.major ? 0.9 : 0.55}
      />
    {/each}
    <path d={trackPath} fill="none" stroke="var(--rule)" stroke-width="2" />
    {#if valuePath}
      <path d={valuePath} fill="none" stroke="var(--signal)" stroke-width="2.5" />
    {/if}
    <circle cx={cx} cy={cy} r={capR + 0.5} fill="var(--panel-raised)" stroke="var(--rule-strong)" stroke-width="1" />
    <circle
      cx={cx}
      cy={cy}
      r={capR - 2}
      fill="none"
      stroke="var(--rule)"
      stroke-width="0.8"
      stroke-dasharray="1 2"
      opacity="0.7"
    />
    <line
      x1={cx}
      y1={cy}
      x2={pointer.x}
      y2={pointer.y}
      stroke="var(--ink)"
      stroke-width="1.5"
      stroke-linecap="round"
    />
    <circle cx={cx} cy={cy} r="1.4" fill="var(--ink-3)" />
  </svg>
  <span class="knob-readout mono">
    <span class="knob-num">{display.n}</span>
    {#if display.u}<span class="knob-unit">{display.u}</span>{/if}
  </span>
</div>

<style>
  .knob-field {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    user-select: none;
    touch-action: none;
    cursor: ns-resize;
    border-radius: var(--r);
  }

  .knob-field:focus-visible {
    outline: 2px solid var(--signal);
    outline-offset: 2px;
  }

  .knob-field.disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .knob-label {
    font-size: 11px;
    line-height: 1.2;
    color: var(--ink-2);
    text-align: center;
    min-height: 14px;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .knob-svg {
    display: block;
    overflow: visible;
  }

  .knob-readout {
    display: inline-flex;
    align-items: baseline;
    gap: 3px;
    font-size: 11px;
    line-height: 1.1;
    color: var(--ink);
    white-space: nowrap;
  }

  .knob-unit {
    font-size: 9px;
    color: var(--ink-3);
    letter-spacing: 0.02em;
  }
</style>
