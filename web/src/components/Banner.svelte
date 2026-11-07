<script lang="ts">
  import Icon from './Icon.svelte';

  interface Props {
    message: string;
    code?: string;
    kind?: 'error' | 'warning' | 'info';
    retryLabel?: string;
    onretry?: () => void;
  }

  let { message, code, kind = 'error', retryLabel = 'Retry', onretry }: Props = $props();
</script>

<div class="banner" class:warning={kind === 'warning'} class:info={kind === 'info'} role={kind === 'error' ? 'alert' : 'status'}>
  <span class="banner-icon" aria-hidden="true">
    <Icon name={kind === 'info' ? 'refresh' : 'alert'} size={16} />
  </span>
  <div class="banner-copy">
    <span class="banner-msg">{message}</span>
    {#if code}<span class="banner-code mono">{code}</span>{/if}
  </div>
  {#if onretry}
    <button type="button" class="btn" onclick={onretry}>
      <Icon name="refresh" size={13} />
      {retryLabel}
    </button>
  {/if}
</div>

<style>
  .banner {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    background: color-mix(in srgb, var(--alert) 6%, var(--panel));
    border: 1px solid color-mix(in srgb, var(--alert) 45%, var(--rule));
    border-left: 2px solid var(--alert);
    border-radius: var(--r);
  }

  .banner.warning {
    background: color-mix(in srgb, var(--signal) 6%, var(--panel));
    border-color: color-mix(in srgb, var(--signal) 45%, var(--rule));
    border-left-color: var(--signal);
  }

  .banner.info {
    background: var(--panel);
    border-color: var(--rule);
    border-left-color: var(--trace);
  }

  .banner-icon {
    color: var(--alert);
    margin-top: 1px;
    flex: none;
  }

  .warning .banner-icon {
    color: var(--signal);
  }

  .info .banner-icon {
    color: var(--trace);
  }

  .banner-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }

  .banner-msg {
    font-size: 12px;
    line-height: 1.45;
    color: var(--ink);
    overflow-wrap: anywhere;
  }

  .banner-code {
    font-size: 9px;
    letter-spacing: 0.04em;
    color: var(--ink-3);
    text-transform: uppercase;
  }

  .btn {
    flex: none;
  }
</style>
