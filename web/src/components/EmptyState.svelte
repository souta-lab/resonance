<script lang="ts">
  import Icon from './Icon.svelte';
  import type { IconName } from '../lib/icons';

  interface Props {
    title: string;
    description: string;
    actionLabel?: string;
    onaction?: () => void;
    icon?: IconName;
    fill?: boolean;
  }

  let {
    title,
    description,
    actionLabel,
    onaction,
    icon = 'rack',
    fill = false,
  }: Props = $props();
</script>

<div class="empty-state" class:fill>
  <div class="empty-inner">
    <div class="empty-icon" aria-hidden="true">
      <Icon name={icon} size={20} />
    </div>
    <div class="empty-copy">
      <h3>{title}</h3>
      <p>{description}</p>
      {#if actionLabel && onaction}
        <button type="button" class="btn" onclick={onaction}>
          <Icon name="plus" size={14} />
          {actionLabel}
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .empty-state {
    display: flex;
    align-items: flex-start;
    padding: 20px;
    background: var(--panel);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    box-shadow: var(--shadow-inset);
  }

  .empty-state.fill {
    height: 100%;
    align-items: center;
    justify-content: center;
  }

  .empty-inner {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    max-width: 380px;
  }

  .empty-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    flex: none;
    background: var(--panel-sunken);
    border: 1px solid var(--rule);
    border-radius: var(--r);
    box-shadow: var(--shadow-sunken);
    color: var(--ink-3);
  }

  .empty-copy {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    line-height: 1.2;
    color: var(--ink);
  }

  p {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--ink-2);
  }

  .btn {
    margin-top: 4px;
  }
</style>
