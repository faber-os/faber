<script lang="ts" module>
  import type { PartStateKind } from '../lib/state/types';

  type Tone = 'ok' | 'warn' | 'danger' | 'info' | 'neutral';

  export const STATE_LOOK: Record<PartStateKind, { label: string; tone: Tone }> = {
    running: { label: 'Running', tone: 'ok' },
    starting: { label: 'Starting', tone: 'info' },
    degraded: { label: 'Degraded', tone: 'warn' },
    failed: { label: 'Failed', tone: 'danger' },
    conflict: { label: 'Conflict', tone: 'danger' },
    invalid: { label: 'Invalid', tone: 'danger' },
    stopped: { label: 'Stopped', tone: 'neutral' },
  };
</script>

<script lang="ts">
  let { kind }: { kind: PartStateKind } = $props();
  const look = $derived(STATE_LOOK[kind] ?? { label: kind, tone: 'neutral' as Tone });
</script>

<span class="badge {look.tone}" data-state={kind}>
  <span class="dot" aria-hidden="true"></span>{look.label}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 9px 0 8px;
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
    font-weight: 650;
    letter-spacing: 0.01em;
    white-space: nowrap;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
  }
  .ok {
    color: var(--ok);
    background: var(--ok-soft);
  }
  .warn {
    color: var(--warn);
    background: var(--warn-soft);
  }
  .danger {
    color: var(--danger);
    background: var(--danger-soft);
  }
  .info {
    color: var(--info);
    background: var(--info-soft);
  }
  .info .dot {
    animation: pulse 1.4s ease-in-out infinite;
  }
  .neutral {
    color: var(--neutral);
    background: var(--neutral-soft);
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
</style>
