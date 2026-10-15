<script lang="ts">
  import type { ConnectionStatus } from '../lib/state/types';

  let { status }: { status: ConnectionStatus } = $props();

  const LOOK: Record<ConnectionStatus, { text: string; detail: string; tone: string }> = {
    connected: { text: 'Connected', detail: 'Live connection to the Faber core.', tone: 'ok' },
    connecting: {
      text: 'Connecting…',
      detail: 'Opening the connection to the Faber core.',
      tone: 'wait',
    },
    reconnecting: {
      text: 'Reconnecting…',
      detail:
        'Lost the connection to the Faber core. Trying again on its own; check that faber-core is running.',
      tone: 'warn',
    },
    closed: {
      text: 'Not connected',
      detail: 'The shell is not connected to the Faber core.',
      tone: 'off',
    },
  };
  const look = $derived(LOOK[status]);
</script>

<div
  class="conn {look.tone}"
  role="status"
  aria-live="polite"
  title={look.detail}
  data-status={status}
>
  <span class="dot" aria-hidden="true"></span>
  <span class="text">{look.text}</span>
</div>

<style>
  .conn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: var(--touch);
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    font-weight: 560;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--neutral);
  }
  .ok .dot {
    background: var(--ok);
    box-shadow: 0 0 0 3px var(--ok-soft);
  }
  .warn {
    color: var(--warn);
  }
  .warn .dot,
  .wait .dot {
    background: var(--warn);
    animation: blink 1.2s ease-in-out infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.3;
    }
  }
  @media (max-width: 720px) {
    .ok .text {
      display: none;
    }
  }
</style>
