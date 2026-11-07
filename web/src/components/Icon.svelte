<script lang="ts">
  import { ICON_PATHS, type IconName } from '../lib/icons';

  interface Props {
    name: IconName;
    size?: number;
    stroke?: number;
    className?: string;
  }

  let { name, size = 18, stroke = 1.5, className = '' }: Props = $props();
</script>

<svg
  class={className}
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#each ICON_PATHS[name] as d}
    <path {d} />
  {/each}
</svg>
