<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { ErrorBody } from '../lib/state/types';
  import DismissButton from './DismissButton.svelte';

  let {
    error,
    tone = 'danger',
    actions,
    ondismiss,
  }: {
    error: Pick<ErrorBody, 'message' | 'hint'> & { code?: string };
    tone?: 'danger' | 'warn' | 'ok';
    actions?: Snippet;
    /** Shows a × for a notice about a lasting problem (lib/desktop/dismissals). */
    ondismiss?: () => void;
  } = $props();
</script>

<div class="notice {tone}" class:dismissible={ondismiss} role="alert">
  {#if ondismiss}<span class="close"><DismissButton {ondismiss} /></span>{/if}
  <p class="message">{error.message}</p>
  {#if error.hint}<p class="hint">{error.hint}</p>{/if}
  {#if error.code || actions}
    <div class="foot">
      {#if error.code}<code class="code selectable">{error.code}</code>{/if}
      {#if actions}<div class="actions">{@render actions()}</div>{/if}
    </div>
  {/if}
</div>

<style>
  .notice {
    display: grid;
    gap: 2px;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--danger-soft);
    color: var(--text);
    user-select: text;
  }
  .dismissible {
    position: relative;
    padding-right: calc(var(--space-4) + 32px);
  }
  .close {
    position: absolute;
    top: var(--space-2);
    right: var(--space-2);
  }
  .warn {
    background: var(--warn-soft);
  }
  .ok {
    background: var(--ok-soft);
  }
  .message {
    font-weight: 650;
    font-size: var(--text-md);
  }
  .danger .message {
    color: var(--danger-text);
  }
  .hint {
    font-size: var(--text-sm);
    color: var(--text);
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-2);
  }
  .code {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
  }
</style>
