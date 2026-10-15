<script lang="ts">
  import type { ConnectionStatus } from '../lib/state/types';
  import Logo from './Logo.svelte';

  /** The loading screen: shown until the core has answered, so nobody sees a half-built desktop. */
  let { status }: { status: ConnectionStatus } = $props();
  let slow = $state(false);

  $effect(() => {
    const t = setTimeout(() => (slow = true), 4000);
    return () => clearTimeout(t);
  });
</script>

<div class="boot" role="status" aria-live="polite">
  <div class="mark">
    <span class="nut"><Logo size={96} title="Faber" /></span>
    <p class="name" aria-hidden="true">Faber</p>
  </div>
  <p class="line">
    {#if status === 'reconnecting' || slow}
      Still waiting for the Faber core. It may be starting up; if this stays, check that faber-core
      is running.
    {:else}
      Connecting to the core…
    {/if}
  </p>
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
    display: grid;
    justify-items: center;
    gap: var(--space-3);
  }
  .nut {
    color: var(--accent);
    animation: settle 900ms var(--ease-out);
  }
  /* The nut turns one flat (60°) as it seats, once. */
  @keyframes settle {
    from {
      transform: rotate(-60deg);
      opacity: 0;
    }
  }
  .name {
    font-size: 2rem;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .line {
    max-width: 42ch;
    color: var(--text-muted);
  }
</style>
