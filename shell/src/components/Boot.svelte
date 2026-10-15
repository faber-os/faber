<script lang="ts">
  import type { ConnectionStatus } from '../lib/state/types';
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
    <h1 class="title">Restart the Faber core</h1>
    <p class="line">
      This desktop was built for a different version of the Faber core than the one running. Stop
      faber-core and start it again (build it first if you changed the code). This page connects by
      itself once the core is back.
    </p>
    <p class="ids">desktop {mismatch.shell} · core {mismatch.core ?? 'too old to say'}</p>
  {:else}
    <p class="line">
      {#if status === 'reconnecting' || slow}
        Waiting for the Faber core. Is faber-core running?
      {:else}
        Connecting to the core…
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
