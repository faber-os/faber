<script lang="ts">
  import type { ConnectionStatus } from '../lib/state/types';
  import { t } from '../lib/i18n';
  import Wordmark from './Wordmark.svelte';

  /**
   * The loading screen: shown until the core has answered, so nobody sees a half-built desktop.
   * With `mismatch` it stays, because this shell and the core were built from different APIs.
   */
  let {
    status,
    mismatch,
  }: { status: ConnectionStatus; mismatch?: { shell: string; core: string | undefined } } =
    $props();
  let slow = $state(false);

  $effect(() => {
    const t = setTimeout(() => (slow = true), 4000);
    return () => clearTimeout(t);
  });
</script>

<div class="boot" role="status" aria-live="polite">
  <div class="mark"><Wordmark size={56} os /></div>
  {#if mismatch}
    <h1 class="title">{t('boot.mismatch.title')}</h1>
    <p class="line">{t('boot.mismatch.message')}</p>
    <p class="ids">
      {t('boot.mismatch.versions', {
        shell: mismatch.shell,
        core: mismatch.core ?? t('boot.mismatch.coreTooOld'),
      })}
    </p>
  {:else}
    <p class="line">
      {#if status === 'reconnecting' || slow}
        {t('boot.waiting')}
      {:else}
        {t('boot.connecting')}
      {/if}
    </p>
  {/if}
</div>

<style>
  .boot {
    position: fixed;
    inset: 0;
    z-index: 300000;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: var(--space-6);
    padding: var(--space-5);
    background: var(--surface-3);
    text-align: center;
  }
  .mark {
    animation: settle 900ms var(--ease-out);
  }
  /* The lockup settles into place once, as a nut seats. */
  @keyframes settle {
    from {
      transform: translateY(6px);
      opacity: 0;
    }
  }
  .line {
    max-width: 42ch;
    color: var(--text-muted);
  }
  .title {
    margin: 0;
    font-size: var(--text-xl, 1.4rem);
  }
  .ids {
    font-family: var(--font-mono);
    font-size: var(--text-sm, 0.85rem);
    color: var(--text-muted);
  }
</style>
