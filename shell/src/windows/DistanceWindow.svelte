<script lang="ts">
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import { distanceInfo } from '../lib/state/partLimits';
  import type { DistanceReading, DistanceStatus, ErrorBody } from '../lib/state/types';
  import { History, barFraction, plotRuns } from '../lib/viewers/signals';
  import { formatLength } from '../lib/viewers/units';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win, visible }: WindowProps = $props();

  const MAX_HZ = 15;
  const PLOT_S = 10;
  const PLOT_W = 300;
  const PLOT_H = 100;

  const core = useCore();

  const address = $derived(win.params.address ?? '');
  const partName = $derived(win.params.part ?? address.split('/')[0] ?? '');
  const part = $derived(core.parts?.find((p) => p.name === partName) ?? null);
  const info = $derived(part ? distanceInfo(part) : null);
  const maxM = $derived(info?.maxM ?? 4);
  const unit = $derived(core.settings?.units.length ?? 'metric');

  let latest = $state<DistanceReading | null>(null);
  let runs = $state<string[]>([]);
  let error = $state<ErrorBody | null>(null);
  const history = new History(PLOT_S * 1_000_000);

  $effect(() => {
    const stop = core.watchSignal(address, MAX_HZ, {
      onReading: (r) => {
        if (r.capability !== 'distance@1') return;
        history.push({
          tUs: r.reading.tUs,
          value: r.reading.meters,
          ok: r.reading.status === 'ok',
        });
        latest = r.reading;
        error = null;
        if (visible) redraw();
      },
      onError: (e) => (error = e),
    });
    return stop;
  });

  // Redraw when the window comes back or the range changes.
  $effect(() => {
    void maxM;
    if (visible) redraw();
  });

  function redraw() {
    const now = history.latest?.tUs ?? 0;
    runs = plotRuns(history.samples, now, history.windowUs, maxM, PLOT_W, PLOT_H);
  }

  const STATUS: Record<DistanceStatus, () => string> = {
    ok: () => t('distance.status.ok'),
    'too-close': () => t('distance.status.tooClose'),
    'too-far': () => t('distance.status.tooFar'),
    'no-target': () => t('distance.status.noTarget'),
    error: () => t('distance.status.error'),
  };
</script>

<div class="viewer">
  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {:else if !latest}
    <div class="pad quiet" role="status">
      <p><RichText text={t('distance.waiting', { address })} /></p>
      {#if part && part.state.kind !== 'running'}
        <p class="sub">
          {part.state.message
            ? t('scan.partStateWithMessage', {
                state: part.state.kind,
                message: part.state.message,
              })
            : t('scan.partState', { state: part.state.kind })}
        </p>
      {/if}
    </div>
  {/if}

  {#if latest}
    <div class="now">
      <p class="value" class:off={latest.status !== 'ok'} data-testid="distance-value">
        {formatLength(latest.meters, unit)}
      </p>
      <p class="status" class:off={latest.status !== 'ok'}>{STATUS[latest.status]()}</p>
    </div>
    <div
      class="bar"
      role="meter"
      aria-label={t('distance.bar.label')}
      aria-valuemin={0}
      aria-valuemax={maxM}
      aria-valuenow={latest.meters}
      aria-valuetext={formatLength(latest.meters, unit)}
    >
      <span class="fill" style:width="{barFraction(latest.meters, maxM) * 100}%"></span>
      {#if info}
        <span class="min" style:left="{barFraction(info.minM, maxM) * 100}%"></span>
      {/if}
    </div>
    <figure class="plot">
      <svg
        viewBox="0 0 {PLOT_W} {PLOT_H}"
        preserveAspectRatio="none"
        data-testid="distance-plot"
        aria-hidden="true"
      >
        {#each runs as points, i (i)}
          <polyline {points} vector-effect="non-scaling-stroke" />
        {/each}
      </svg>
      <figcaption>{t('distance.plot.label', { seconds: PLOT_S })}</figcaption>
    </figure>
  {/if}

  <dl class="stats">
    {#if info}
      <div>
        <dt class="visually-hidden">{t('distance.bar.label')}</dt>
        <dd>
          {t('distance.range', {
            min: formatLength(info.minM, unit),
            max: formatLength(info.maxM, unit),
          })}
        </dd>
      </div>
    {/if}
    <div class="grow">
      <dt class="visually-hidden">{t('scan.stats.signal')}</dt>
      <dd class="addr"><code>{address}</code></dd>
    </div>
    {#if part}
      <div>
        <dt class="visually-hidden">{t('scan.stats.partState')}</dt>
        <dd><StateBadge kind={part.state.kind} /></dd>
      </div>
    {/if}
  </dl>
</div>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    background: var(--surface);
  }
  .pad {
    padding: var(--space-4);
  }
  .quiet {
    display: grid;
    gap: var(--space-1);
    font-size: var(--text-sm);
  }
  .sub {
    color: var(--text-muted);
  }
  .now {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-4) var(--space-4) var(--space-2);
  }
  .value {
    font-size: var(--text-xl);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .status {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .off {
    color: var(--warn-text);
  }
  .bar {
    position: relative;
    height: 16px;
    margin: 0 var(--space-4);
    border-radius: 8px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .fill {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: var(--accent);
    transition: width 60ms linear;
  }
  .min {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--text-faint);
  }
  .plot {
    flex: 1;
    min-height: 90px;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: var(--space-3) var(--space-4);
  }
  .plot svg {
    flex: 1;
    width: 100%;
    min-height: 60px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  polyline {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
  }
  figcaption {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .stats {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin: auto 0 0;
    padding: var(--space-2) var(--space-4);
    min-height: 44px;
    border-top: 1px solid var(--line);
    background: var(--surface-2);
    font-size: var(--text-sm);
  }
  .stats div {
    display: flex;
    align-items: baseline;
  }
  .stats .grow {
    flex: 1;
    min-width: 0;
    justify-content: flex-end;
  }
  dd {
    margin: 0;
  }
  .addr {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
  }
</style>
