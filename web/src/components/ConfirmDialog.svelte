<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';

  interface Props {
    open: boolean;
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  }

  let {
    open,
    title,
    message,
    confirmLabel = 'Confirm',
    cancelLabel = 'Cancel',
    danger = false,
    onconfirm,
    oncancel,
  }: Props = $props();

  let panelEl = $state<HTMLDivElement | null>(null);
  let cancelEl = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    if (open) {
      void tick().then(() => cancelEl?.focus());
    }
  });

  function onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      oncancel();
      return;
    }
    if (e.key !== 'Tab' || !panelEl) return;
    const focusable = Array.from(
      panelEl.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    );
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

{#if open}
  <div class="dialog-layer">
    <button type="button" class="backdrop" aria-label="Close dialog" onclick={oncancel}></button>
    <div
      class="dialog panel"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-message"
      bind:this={panelEl}
      onkeydown={onKeyDown}
    >
      <header class="dialog-head">
        <span class="dialog-icon" class:alert={danger}>
          <Icon name={danger ? 'alert' : 'check'} size={16} />
        </span>
        <h2 id="confirm-title">{title}</h2>
      </header>
      <p id="confirm-message">{message}</p>
      <div class="dialog-actions">
        <button type="button" class="btn" bind:this={cancelEl} onclick={oncancel}>
          {cancelLabel}
        </button>
        <button
          type="button"
          class="btn"
          class:btn-danger={danger}
          class:btn-primary={!danger}
          onclick={onconfirm}
        >
          {confirmLabel}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog-layer {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    background: rgb(20 18 12 / 0.32);
    border: 0;
    cursor: default;
  }

  .dialog {
    position: relative;
    width: 380px;
    max-width: 100%;
    padding: 16px;
    box-shadow: var(--shadow-raised);
    animation: dialog-in 120ms ease-out;
  }

  .dialog-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }

  .dialog-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    flex: none;
    background: var(--panel-sunken);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    color: var(--ok);
  }

  .dialog-icon.alert {
    color: var(--alert);
  }

  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    line-height: 1.2;
  }

  p {
    margin: 0 0 16px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--ink-2);
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  @keyframes dialog-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
