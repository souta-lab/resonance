<script lang="ts">
  export interface SegmentOption {
    value: string | number;
    label: string;
    title?: string;
  }

  interface Props {
    options: SegmentOption[];
    value: string | number;
    onchange?: (value: string | number) => void;
    ariaLabel: string;
    className?: string;
  }

  let { options, value, onchange, ariaLabel, className = '' }: Props = $props();

  function select(next: string | number): void {
    if (next === value) return;
    onchange?.(next);
  }
</script>

<div class="segmented {className}" role="group" aria-label={ariaLabel}>
  {#each options as option (option.value)}
    <button
      type="button"
      aria-pressed={option.value === value}
      title={option.title ?? option.label}
      onclick={() => select(option.value)}
    >
      {option.label}
    </button>
  {/each}
</div>
