<script lang="ts">
  import { store, pipelineLabel } from '../lib/store.svelte';
  import type {
    EffectDescriptor,
    EffectInstance,
    ParamSpec,
    PipelineKind,
  } from '../lib/types';
  import { formatHz } from '../lib/format';
  import Icon from './Icon.svelte';
  import Switch from './Switch.svelte';
  import Segmented from './Segmented.svelte';
  import ParamField from './ParamField.svelte';
  import EqGraph from './EqGraph.svelte';
  import BandTable from './BandTable.svelte';
  import ConfirmDialog from './ConfirmDialog.svelte';

  interface Props {
    kind: PipelineKind;
    effect: EffectInstance;
    descriptor: EffectDescriptor | null;
  }

  let { kind, effect, descriptor }: Props = $props();

  const COMMON_SPECS: ParamSpec[] = [
    {
      key: 'input-gain',
      label: 'Input gain',
      group: 'Gain / Mix',
      advanced: false,
      kind: { type: 'float', min: -24, max: 24, default: 0, step: 0.1, unit: 'db', log: false },
    },
    {
      key: 'output-gain',
      label: 'Output gain',
      group: 'Gain / Mix',
      advanced: false,
      kind: { type: 'float', min: -24, max: 24, default: 0, step: 0.1, unit: 'db', log: false },
    },
    {
      key: 'dry',
      label: 'Dry',
      group: 'Gain / Mix',
      advanced: false,
      kind: { type: 'float', min: 0, max: 1, default: 1, step: 0.01, unit: 'none', log: false },
    },
    {
      key: 'wet',
      label: 'Wet',
      group: 'Gain / Mix',
      advanced: false,
      kind: { type: 'float', min: 0, max: 1, default: 1, step: 0.01, unit: 'none', log: false },
    },
  ];

  let channel = $state(0);
  let selectedBand = $state(0);
  let advancedOpen = $state(false);
  let confirmRemove = $state(false);

  const sampleRate = $derived(store.status?.dsp.sample_rate ?? 48000);
  const channelLabels = $derived(descriptor?.bands?.channel_labels ?? []);
  const channelLabel = $derived(channelLabels[channel] ?? channelLabels[0] ?? 'left');
  const bands = $derived(effect.bands?.[channelLabel] ?? []);
  const bandCount = $derived(
    Math.max(0, Math.min(Number(effect.params['num-bands'] ?? bands.length), bands.length)),
  );
  const safeSelected = $derived(Math.min(selectedBand, Math.max(0, bandCount - 1)));

  const groups = $derived.by(() => {
    const all = [...COMMON_SPECS, ...(descriptor?.params ?? [])].filter(
      (spec) => spec.key !== 'bypass' && !spec.advanced,
    );
    const order: string[] = [];
    const map = new Map<string, ParamSpec[]>();
    for (const spec of all) {
      if (!map.has(spec.group)) {
        map.set(spec.group, []);
        order.push(spec.group);
      }
      map.get(spec.group)!.push(spec);
    }
    return order.map((group) => ({ id: group, label: group, params: map.get(group)! }));
  });

  const advanced = $derived(
    (descriptor?.params ?? []).filter((spec) => spec.advanced && spec.key !== 'bypass'),
  );

  const channelOptions = $derived(
    channelLabels.map((label, index) => ({
      value: index,
      label: label.charAt(0).toUpperCase() + label.slice(1),
    })),
  );

  const slotIndex = $derived(String(effect.index + 1).padStart(2, '0'));
  const kindLabel = $derived(effect.kind.replace(/_/g, ' '));
  const latencyLabel = $derived(
    effect.latency_frames === 0 ? '0 frames' : `${effect.latency_frames} frames`,
  );
  const groupLabel = $derived(
    store.catalog?.groups.find((group) => group.id === descriptor?.group)?.label ??
      descriptor?.group ??
      'Effect',
  );
  const kicker = $derived(
    kindLabel.toLowerCase() === effect.name.toLowerCase()
      ? groupLabel
      : `${groupLabel} · ${kindLabel}`,
  );

  function compatClass(compat: EffectInstance['compat']): string {
    if (compat === 'exact') return 'badge badge-ok';
    if (compat === 'approximate') return 'badge badge-signal';
    if (compat === 'substitute') return 'badge badge-alert';
    return 'badge';
  }

  function compatTitle(compat: EffectInstance['compat']): string {
    if (compat === 'exact') return 'Exact match with the EasyEffects baseline.';
    if (compat === 'approximate') return 'Approximate match; some parameters may differ.';
    if (compat === 'substitute') return 'Substitute effect; behaviour differs from the baseline.';
    return 'Measurement-only effect.';
  }

  function patch(spec: ParamSpec, value: number | boolean): void {
    store.patchParam(kind, effect.id, spec.key, value);
  }

  function patchBands(next: typeof bands): void {
    store.patchBands(kind, effect.id, channelLabel, next);
  }

  function setChannel(value: string | number): void {
    channel = Number(value);
    selectedBand = 0;
  }
</script>

<article class="faceplate" aria-label={`${effect.name} faceplate`}>
  <header class="fp-head" title={descriptor?.summary ?? undefined}>
    <span class="fp-index mono" aria-hidden="true">{slotIndex}</span>
    <div class="fp-title">
      <span class="fp-kicker micro">{kicker}</span>
      <h2>{effect.name}</h2>
    </div>
    <div class="fp-meta">
      <span class={compatClass(effect.compat)} title={compatTitle(effect.compat)}>
        {effect.compat}
      </span>
      <span class="fp-latency mono" title="Effect latency">{latencyLabel}</span>
      <span class="fp-bypass-label micro">Bypass</span>
      <Switch
        checked={effect.bypass}
        label={`Bypass ${effect.name}`}
        onchange={(checked) => store.setEffectBypass(kind, effect.id, checked)}
      />
      <span class="meta-rule"></span>
      <button
        type="button"
        class="icon-btn"
        aria-label={`Duplicate ${effect.name}`}
        title="Duplicate effect"
        onclick={() => store.duplicateEffect(kind, effect.id)}
      >
        <Icon name="copy" size={16} />
      </button>
      <button
        type="button"
        class="icon-btn"
        aria-label={`Remove ${effect.name}`}
        title="Remove effect"
        onclick={() => (confirmRemove = true)}
      >
        <Icon name="trash" size={16} />
      </button>
    </div>
  </header>

  <div class="fp-body">
    {#if !descriptor}
      <p class="fp-warning">
        No catalog entry for <span class="mono">{effect.kind}</span>. The daemon may be newer than
        this control surface; only the common parameters are shown.
      </p>
    {/if}

    {#if descriptor?.bands}
      <section class="eq-section" aria-label="Equalizer">
        <div class="eq-toolbar">
          <span class="micro">Response</span>
          {#if channelOptions.length > 1}
            <Segmented
              options={channelOptions}
              value={channel}
              ariaLabel="EQ channel"
              onchange={setChannel}
            />
          {/if}
          <span class="spacer"></span>
          <span class="eq-meta mono">
            {bandCount} bands · {formatHz(sampleRate / 2)} Hz Nyquist
          </span>
        </div>

        <EqGraph
          {bands}
          count={bandCount}
          {sampleRate}
          selected={safeSelected}
          {channelLabel}
          onselect={(index) => (selectedBand = index)}
          onchange={patchBands}
        />

        <BandTable
          {bands}
          count={bandCount}
          fields={descriptor.bands.fields}
          selected={safeSelected}
          {channelLabel}
          onselect={(index) => (selectedBand = index)}
          onchange={patchBands}
        />
      </section>
    {/if}

    {#each groups as group (group.id)}
      <section class="param-group" aria-label={group.label}>
        <div class="group-head">
          <span class="micro">{group.label}</span>
          <span class="group-rule"></span>
        </div>
        <div class="param-row">
          {#each group.params as spec (spec.key)}
            <ParamField
              {spec}
              value={effect.params[spec.key] ?? spec.kind.default}
              onchange={(value) => patch(spec, value)}
            />
          {/each}
        </div>
      </section>
    {/each}

    {#if advanced.length}
      <section class="advanced">
        <button
          type="button"
          class="advanced-toggle"
          aria-expanded={advancedOpen}
          onclick={() => (advancedOpen = !advancedOpen)}
        >
          <Icon name={advancedOpen ? 'chevron-down' : 'chevron-right'} size={14} />
          <span>Advanced</span>
          <span class="micro">{advanced.length} parameters</span>
        </button>
        {#if advancedOpen}
          <div class="param-row advanced-row">
            {#each advanced as spec (spec.key)}
              <ParamField
                {spec}
                value={effect.params[spec.key] ?? spec.kind.default}
                onchange={(value) => patch(spec, value)}
              />
            {/each}
          </div>
        {/if}
      </section>
    {/if}
  </div>

  <ConfirmDialog
    open={confirmRemove}
    title="Remove effect"
    message={`Remove ${effect.name} from the ${pipelineLabel(kind).toLowerCase()} chain? This cannot be undone.`}
    confirmLabel="Remove"
    danger
    onconfirm={() => {
      confirmRemove = false;
      void store.removeEffect(kind, effect.id);
    }}
    oncancel={() => (confirmRemove = false)}
  />
</article>

<style>
  .faceplate {
    display: flex;
    flex-direction: column;
    min-height: 100%;
    min-width: 0;
    width: 100%;
    background: var(--panel);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    box-shadow: var(--shadow-inset);
    animation: fp-in 120ms ease-out;
  }

  .fp-head {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--rule);
    flex-wrap: wrap;
  }

  .fp-index {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    flex: none;
    background: var(--panel-sunken);
    border: 1px solid var(--rule-strong);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
    font-size: 16px;
    font-weight: 500;
    color: var(--ink-2);
    letter-spacing: 0.02em;
  }

  .fp-title {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .fp-kicker {
    font-size: 9px;
    line-height: 1.2;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    line-height: 1.15;
    letter-spacing: 0;
    color: var(--ink);
  }

  .fp-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
    flex-wrap: wrap;
  }

  .fp-latency {
    font-size: 10px;
    color: var(--ink-3);
    white-space: nowrap;
  }

  .fp-bypass-label {
    font-size: 9px;
    margin-left: 4px;
  }

  .meta-rule {
    width: 1px;
    height: 18px;
    background: var(--rule);
    margin: 0 2px;
  }

  .fp-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 12px;
  }

  .fp-warning {
    margin: 0;
    padding: 8px 10px;
    background: color-mix(in srgb, var(--signal) 6%, var(--panel));
    border: 1px solid color-mix(in srgb, var(--signal) 40%, var(--rule));
    border-left: 2px solid var(--signal);
    border-radius: var(--r);
    font-size: 12px;
    line-height: 1.45;
    color: var(--ink-2);
  }

  .eq-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .eq-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .eq-meta {
    font-size: 9px;
    color: var(--ink-3);
    letter-spacing: 0.02em;
  }

  .param-group {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .group-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .group-rule {
    flex: 1;
    height: 1px;
    background: var(--rule);
  }

  .param-row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 14px 18px;
  }

  .advanced {
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-top: 1px solid var(--rule);
    padding-top: 12px;
  }

  .advanced-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
    height: 26px;
    padding: 0 6px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r);
    color: var(--ink-2);
    font-size: 12px;
    font-weight: 500;
  }

  .advanced-toggle:hover {
    background: var(--panel-sunken);
    border-color: var(--rule);
    color: var(--ink);
  }

  .advanced-toggle .micro {
    margin-left: 4px;
    font-size: 9px;
  }

  .advanced-row {
    padding: 10px;
    background: var(--panel-sunken);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
  }

  @keyframes fp-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @media (max-width: 899px) {
    .fp-head {
      padding: 10px 12px;
      gap: 10px;
    }

    .fp-index {
      width: 36px;
      height: 36px;
      font-size: 16px;
    }

    h2 {
      font-size: 16px;
    }

    .fp-meta {
      width: 100%;
      margin-left: 0;
      padding-top: 4px;
      border-top: 1px solid var(--rule);
    }

    .fp-body {
      padding: 12px;
      gap: 14px;
    }
  }
</style>
