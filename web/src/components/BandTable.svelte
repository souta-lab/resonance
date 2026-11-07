<script lang="ts">
  import type { Band, BandField } from '../lib/types';
  import { clamp } from '../lib/format';

  interface Props {
    bands: Band[];
    count: number;
    fields: BandField[];
    selected: number;
    channelLabel: string;
    onselect: (index: number) => void;
    onchange: (bands: Band[]) => void;
  }

  let { bands, count, fields, selected, channelLabel, onselect, onchange }: Props = $props();

  const typeField = $derived(fields.find((f) => f.key === 'type'));
  const typeLabels = $derived(
    typeField && typeField.kind.type === 'enum' ? typeField.kind.labels : [],
  );
  const visible = $derived(bands.slice(0, Math.max(0, count)));

  function update(index: number, patch: Partial<Band>): void {
    onchange(bands.map((b, i) => (i === index ? { ...b, ...patch } : b)));
  }

  function numberFrom(e: Event, fallback: number): number {
    const raw = Number((e.currentTarget as HTMLInputElement).value);
    return Number.isFinite(raw) ? raw : fallback;
  }

  function commitNumber(
    index: number,
    key: 'frequency' | 'gain' | 'q',
    e: Event,
    min: number,
    max: number,
  ): void {
    const band = bands[index];
    if (!band) return;
    const value = clamp(numberFrom(e, band[key]), min, max);
    update(index, { [key]: value });
  }
</script>

<div class="band-table-wrap">
  <table class="band-table">
    <caption class="sr-only">Bands for {channelLabel}</caption>
    <thead>
      <tr>
        <th scope="col" class="idx">#</th>
        <th scope="col">Type</th>
        <th scope="col" class="num">Freq</th>
        <th scope="col" class="num">Gain</th>
        <th scope="col" class="num">Q</th>
        <th scope="col" class="flag">M</th>
        <th scope="col" class="flag">S</th>
      </tr>
    </thead>
    <tbody>
      {#each visible as band, index (index)}
        <tr class:selected={index === selected}>
          <td class="idx">
            <button
              type="button"
              class="idx-btn mono"
              aria-label={`Select band ${index + 1}`}
              aria-pressed={index === selected}
              onclick={() => onselect(index)}
            >
              {String(index + 1).padStart(2, '0')}
            </button>
          </td>
          <td>
            <select
              class="cell-select"
              aria-label={`Band ${index + 1} type`}
              value={band.type}
              onchange={(e) => update(index, { type: Number(e.currentTarget.value) })}
            >
              {#each typeLabels as label, typeIndex}
                <option value={typeIndex}>{label}</option>
              {/each}
            </select>
          </td>
          <td>
            <input
              class="cell-input mono"
              type="number"
              min="10"
              max="24000"
              step="1"
              aria-label={`Band ${index + 1} frequency in hertz`}
              value={Number(band.frequency.toFixed(1))}
              onchange={(e) => commitNumber(index, 'frequency', e, 10, 24000)}
            />
          </td>
          <td>
            <input
              class="cell-input mono"
              type="number"
              min="-36"
              max="36"
              step="0.1"
              aria-label={`Band ${index + 1} gain in decibels`}
              value={Number(band.gain.toFixed(2))}
              onchange={(e) => commitNumber(index, 'gain', e, -36, 36)}
            />
          </td>
          <td>
            <input
              class="cell-input mono"
              type="number"
              min="0.1"
              max="10"
              step="0.01"
              aria-label={`Band ${index + 1} Q`}
              value={Number(band.q.toFixed(2))}
              onchange={(e) => commitNumber(index, 'q', e, 0.1, 10)}
            />
          </td>
          <td class="flag">
            <button
              type="button"
              class="flag-btn"
              class:active={band.mute}
              aria-label={`Mute band ${index + 1}`}
              aria-pressed={band.mute}
              onclick={() => update(index, { mute: !band.mute })}
            >
              M
            </button>
          </td>
          <td class="flag">
            <button
              type="button"
              class="flag-btn solo"
              class:active={band.solo}
              aria-label={`Solo band ${index + 1}`}
              aria-pressed={band.solo}
              onclick={() => update(index, { solo: !band.solo })}
            >
              S
            </button>
          </td>
        </tr>
      {:else}
        <tr>
          <td colspan="7" class="empty">No bands on this channel.</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .band-table-wrap {
    width: 100%;
    max-width: 100%;
    max-height: 134px;
    overflow: auto;
    border: 1px solid var(--rule);
    border-radius: var(--r);
    background: var(--panel);
    box-shadow: var(--shadow-inset);
  }

  .band-table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }

  thead th {
    position: sticky;
    top: 0;
    z-index: 1;
    height: 22px;
    padding: 0 4px;
    background: var(--panel-sunken);
    border-bottom: 1px solid var(--rule-strong);
    font-family: var(--font-mono);
    font-size: 9px;
    font-weight: 500;
    letter-spacing: 0.04em;
    color: var(--ink-3);
    text-align: left;
    text-transform: uppercase;
  }

  thead th.num {
    text-align: right;
  }

  thead th.flag {
    text-align: center;
    width: 26px;
  }

  thead th.idx {
    width: 30px;
  }

  thead th:nth-child(2) {
    width: 120px;
  }

  tbody td {
    height: 28px;
    padding: 1px 3px;
    border-bottom: 1px solid var(--rule);
    vertical-align: middle;
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  tbody tr:hover {
    background: color-mix(in srgb, var(--panel-sunken) 55%, transparent);
  }

  tbody tr.selected {
    background: color-mix(in srgb, var(--signal-soft) 35%, transparent);
    box-shadow: inset 2px 0 0 var(--signal);
  }

  .idx-btn {
    width: 24px;
    height: 22px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r);
    color: var(--ink-3);
    font-size: 10px;
    text-align: center;
  }

  .idx-btn:hover {
    border-color: var(--rule);
    color: var(--ink);
  }

  tr.selected .idx-btn {
    color: var(--signal);
    border-color: var(--signal);
  }

  .cell-select,
  .cell-input {
    width: 100%;
    height: 22px;
    padding: 0 4px;
    background: var(--panel);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    font-size: 11px;
    color: var(--ink);
    min-width: 0;
  }

  .cell-input {
    font-family: var(--font-mono);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .cell-select:focus,
  .cell-input:focus {
    border-color: var(--signal);
    outline: none;
  }

  .flag {
    text-align: center;
  }

  .flag-btn {
    width: 20px;
    height: 20px;
    padding: 0;
    background: var(--panel);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--ink-3);
  }

  .flag-btn:hover {
    border-color: var(--rule-strong);
    color: var(--ink);
  }

  .flag-btn.active {
    background: var(--signal);
    border-color: var(--signal);
    color: #fff8ec;
  }

  .flag-btn.solo.active {
    background: var(--trace);
    border-color: var(--trace);
    color: #f2fbf9;
  }

  .empty {
    height: 40px;
    text-align: center;
    color: var(--ink-3);
    font-size: 11px;
  }
</style>
