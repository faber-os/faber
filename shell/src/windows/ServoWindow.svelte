<script lang="ts">
  // The viewer of a servo@1 signal: the angle it was last told (most servos give no feedback),
  // on a dial between its smallest and largest angle, or that it is released (no pulses).
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody } from '../lib/state/types';
  import { formatAngle } from '../lib/viewers/units';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win }: WindowProps = $props();

  const core = useCore();
  const address = $derived(win.params.address ?? '');
  const partName = $derived(win.params.part ?? address.split('/')[0] ?? '');
  const part = $derived(core.parts?.find((p) => p.name === partName) ?? null);
  const info = $derived.by(() => {
    const i = part?.signals.find((s) => s.address === address)?.info;
    return i?.capability === 'servo@1' ? i.info : null;
  });
  const min = $derived(info?.minAngleRad ?? 0);
  const max = $derived(info?.maxAngleRad ?? Math.PI);

  // undefined: no reading yet; null: released.
  let angle = $state<number | null | undefined>(undefined);
  let error = $state<ErrorBody | null>(null);

  $effect(() =>
    core.watchSignal(address, 15, {
      onReading: (r) => {
        if (r.capability !== 'servo@1') return;
        angle = r.reading.angleRad ?? null;
        error = null;
      },
      onError: (e) => (error = e),
    }),
  );

  // The dial: a half circle from the smallest angle (left) to the largest (right).
  const share = $derived(
    angle === null || angle === undefined || max <= min
      ? 0.5
      : Math.min(1, Math.max(0, (angle - min) / (max - min))),
  );
  const needle = $derived({
    x: 60 - 45 * Math.cos(share * Math.PI),
    y: 60 - 45 * Math.sin(share * Math.PI),
  });
</script>

<div class="viewer">
  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {:else if angle === undefined}
    <p class="pad quiet" role="status"><RichText text={t('signal.waiting', { address })} /></p>
  {:else}
    <div class="now">
      <p class="value" data-testid="servo-value">
        {angle === null ? t('servo.released') : formatAngle(angle)}
      </p>
      <p class="sub">{angle === null ? t('servo.releasedHint') : t('servo.commanded')}</p>
    </div>
    <svg class="dial" viewBox="0 0 120 70" aria-hidden="true">
      <path d="M15 60a45 45 0 0 1 90 0" class="arc" />
      {#if angle !== null}
        <line x1="60" y1="60" x2={needle.x} y2={needle.y} class="needle" />
      {/if}
      <circle cx="60" cy="60" r="3" class="hub" />
    </svg>
    <p class="range">{t('servo.range', { min: formatAngle(min), max: formatAngle(max) })}</p>
  {/if}
  <div class="stats">
    <code>{address}</code>
    {#if part}<StateBadge kind={part.state.kind} />{/if}
  </div>
</div>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .pad {
    padding: var(--space-4);
  }
  .quiet {
    font-size: var(--text-sm);
  }
  .now {
    padding: var(--space-4) var(--space-4) var(--space-2);
  }
  .value {
    font-size: var(--text-xl);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .sub,
  .range {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .range {
    padding: 0 var(--space-4);
  }
  .dial {
    flex: 1;
    min-height: 80px;
    max-height: 200px;
    margin: var(--space-2) var(--space-4);
  }
  .arc {
    fill: none;
    stroke: var(--surface-3);
    stroke-width: 8;
    stroke-linecap: round;
  }
  .needle {
    stroke: var(--accent);
    stroke-width: 3;
    stroke-linecap: round;
  }
  .hub {
    fill: var(--accent);
  }
  .stats {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: auto;
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--line);
    background: var(--surface-2);
    font-size: var(--text-sm);
  }
</style>
