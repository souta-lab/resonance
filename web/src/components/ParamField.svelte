<script lang="ts">
  import type { ParamSpec, ParamValue } from '../lib/types';
  import Knob from './Knob.svelte';
  import Segmented from './Segmented.svelte';
  import Switch from './Switch.svelte';

  interface Props {
    spec: ParamSpec;
    value: ParamValue;
    onchange: (value: number | boolean) => void;
    disabled?: boolean;
  }

  let { spec, value, onchange, disabled = false }: Props = $props();

  const numeric = $derived(typeof value === 'number' ? value : 0);
  const boolValue = $derived(value === true);

  const percentFormat = (v: number): { n: string; u: string } => ({
    n: String(Math.round(v * 100)),
    u: '%',
  });
</script>

{#if spec.kind.type === 'float' || spec.kind.type === 'int'}
  <Knob
    value={numeric}
    min={spec.kind.min}
    max={spec.kind.max}
    step={spec.kind.type === 'int' ? 1 : spec.kind.step}
    defaultValue={spec.kind.default}
    unit={spec.kind.unit}
    log={spec.kind.type === 'float' ? spec.kind.log : false}
    label={spec.label}
    ariaLabel={`${spec.label} (${spec.key})`}
    format={spec.key === 'dry' || spec.key === 'wet' ? percentFormat : undefined}
    {disabled}
    onchange={(v) => onchange(v)}
  />
{:else if spec.kind.type === 'enum'}
  <div class="field enum-field">
    <span class="field-label">{spec.label}</span>
    <div class="enum-scroll">
      <Segmented
        options={spec.kind.labels.map((label, index) => ({ value: index, label }))}
        value={numeric}
        ariaLabel={spec.label}
        onchange={(v) => onchange(Number(v))}
      />
    </div>
  </div>
{:else}
  <div class="field bool-field">
    <span class="field-label">{spec.label}</span>
    <Switch
      checked={boolValue}
      label={spec.label}
      {disabled}
      onchange={(checked) => onchange(checked)}
    />
  </div>
{/if}

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .enum-field {
    width: 232px;
    max-width: 100%;
  }

  .bool-field {
    width: 148px;
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 32px;
    padding: 0 8px;
    border: 1px solid var(--rule);
    border-radius: var(--r);
    background: var(--panel-sunken);
    box-shadow: var(--shadow-inset);
  }

  .field-label {
    font-size: 11px;
    line-height: 1.2;
    color: var(--ink-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .enum-scroll {
    overflow-x: auto;
    max-width: 100%;
    padding-bottom: 1px;
  }
</style>
