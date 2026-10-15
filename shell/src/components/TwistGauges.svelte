<script lang="ts">
  import { t } from '../lib/i18n';
  import type { LengthUnit, Twist2d } from '../lib/state/types';
  import { gaugeFraction } from '../lib/viewers/signals';
  import { formatSpeed, formatTurnRate } from '../lib/viewers/units';

  /**
   * A drive base's velocity as three small gauges with zero in the middle: the commanded value as
   * a bar, the measured one as a mark. `max` are the base's limits, so a full bar is top speed.
   */
  let {
    commanded,
    measured = null,
    max,
    unit,
    holonomic = true,
    compact = false,
  }: {
    commanded: Twist2d | null;
    measured?: Twist2d | null;
    max: Twist2d;
    unit: LengthUnit;
    holonomic?: boolean;
    compact?: boolean;
  } = $props();

  const rows = $derived(
    [
      { key: 'vx' as const, label: t('twist.forward'), turn: false },
      holonomic && { key: 'vy' as const, label: t('twist.sideways'), turn: false },
      { key: 'omega' as const, label: t('twist.turn'), turn: true },
    ].filter((r) => r !== false),
  );

  function text(v: number | undefined, turn: boolean): string {
    if (v === undefined) return '–';
    return turn ? formatTurnRate(v) : formatSpeed(v, unit);
  }
</script>

<dl class="gauges" class:compact aria-label={t('twist.label')}>
  {#each rows as r (r.key)}
    {@const c = commanded?.[r.key]}
    {@const m = measured?.[r.key]}
    {@const fc = gaugeFraction(c ?? 0, max[r.key])}
    {@const fm = m === undefined ? null : gaugeFraction(m, max[r.key])}
    <div class="row">
      <dt>{r.label}</dt>
      <dd class="track" aria-hidden="true">
        <span
          class="bar"
          style:left="{50 + Math.min(0, fc) * 50}%"
          style:width="{Math.abs(fc) * 50}%"
        ></span>
        {#if fm !== null}<span class="mark" style:left="{50 + fm * 50}%"></span>{/if}
        <span class="zero"></span>
      </dd>
      <dd class="num" data-testid="twist-commanded-{r.key}" title={t('twist.commanded')}>
        {text(c, r.turn)}
      </dd>
      {#if measured}
        <dd class="num muted" data-testid="twist-measured-{r.key}" title={t('twist.measured')}>
          {text(m, r.turn)}
        </dd>
      {/if}
    </div>
  {/each}
</dl>

<style>
  .gauges {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-sm);
  }
  .row {
    display: grid;
    grid-template-columns: 5.5rem minmax(3rem, 1fr) 5.5rem 5.5rem;
    align-items: center;
    gap: var(--space-2);
  }
  .compact .row {
    grid-template-columns: 4.5rem minmax(2rem, 1fr) 5rem 5rem;
  }
  dt {
    color: var(--text-muted);
  }
  dd {
    margin: 0;
  }
  .track {
    position: relative;
    height: 10px;
    border-radius: 5px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--accent);
  }
  .mark {
    position: absolute;
    top: -2px;
    bottom: -2px;
    width: 3px;
    margin-left: -1.5px;
    background: var(--text);
  }
  .zero {
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--line-strong);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-weight: 650;
    white-space: nowrap;
  }
  .muted {
    color: var(--text-muted);
    font-weight: 500;
  }
</style>
