<script lang="ts">
  import type { ConnectionStatus } from '../lib/state/types';
  import { t, type MessageKey } from '../lib/i18n';

  let { status }: { status: ConnectionStatus } = $props();

  type Key = Extract<MessageKey, `connection.${string}`>;
  const LOOK: Record<ConnectionStatus, { textKey: Key; detailKey: Key; tone: string }> = {
    connected: {
      textKey: 'connection.connected',
      detailKey: 'connection.connected.detail',
      tone: 'ok',
    },
    connecting: {
      textKey: 'connection.connecting',
      detailKey: 'connection.connecting.detail',
      tone: 'wait',
    },
    reconnecting: {
      textKey: 'connection.reconnecting',
      detailKey: 'connection.reconnecting.detail',
      tone: 'warn',
    },
    closed: {
      textKey: 'connection.closed',
      detailKey: 'connection.closed.detail',
      tone: 'off',
    },
  };
  const look = $derived(LOOK[status]);
</script>

<div
  class="conn {look.tone}"
  role="status"
  aria-live="polite"
  title={t(look.detailKey)}
  data-status={status}
>
  <span class="dot" aria-hidden="true"></span>
  <span class="text">{t(look.textKey)}</span>
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
    color: var(--warn-text);
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
