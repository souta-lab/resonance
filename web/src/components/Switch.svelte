<script lang="ts">
  interface Props {
    checked?: boolean;
    label: string;
    onchange?: (checked: boolean) => void;
    disabled?: boolean;
    title?: string;
  }

  let { checked = $bindable(false), label, onchange, disabled = false, title }: Props = $props();

  function toggle(): void {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  type="button"
  class="switch"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  title={title ?? label}
  {disabled}
  onclick={toggle}
></button>
