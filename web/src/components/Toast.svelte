<script lang="ts">
  import type { ToastItem } from '../lib/store.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    toast: ToastItem;
    ondismiss: (id: number) => void;
  }

  let { toast, ondismiss }: Props = $props();
</script>

<div class="toast" class:ok={toast.kind === 'ok'} class:error={toast.kind === 'error'} role="status">
  <span
    class="led"
    class:led-on={toast.kind === 'ok'}
    class:led-alert={toast.kind === 'error'}
    class:led-signal={toast.kind === 'info'}
  ></span>
  <span class="toast-msg">{toast.message}</span>
  <button
    type="button"
    class="icon-btn toast-close"
    aria-label="Dismiss notification"
    onclick={() => ondismiss(toast.id)}
  >
    <Icon name="close" size={12} />
  </button>
</div>

<style>
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    width: 320px;
    max-width: calc(100vw - 24px);
    padding: 8px 8px 8px 10px;
    background: var(--panel);
    border: 1px solid var(--rule-strong);
    border-left: 2px solid var(--ink-3);
    border-radius: var(--r);
    box-shadow: var(--shadow-raised);
    animation: toast-in 120ms ease-out;
  }

  .toast.ok {
    border-left-color: var(--ok);
  }

  .toast.error {
    border-left-color: var(--alert);
  }

  .toast .led {
    margin-top: 4px;
  }

  .toast-msg {
    flex: 1;
    font-size: 12px;
    line-height: 1.4;
    color: var(--ink);
    overflow-wrap: anywhere;
  }

  .toast-close {
    width: 20px;
    height: 20px;
    flex: none;
  }

  @keyframes toast-in {
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
