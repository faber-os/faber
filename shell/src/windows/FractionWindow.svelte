<script lang="ts">
  // The viewer of an analog-in@1 or pwm-out@1 signal: a share of full scale (0–1), its bar and
  // plot, and the volts when the part knows them.
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody } from '../lib/state/types';
  import { History, barFraction, plotRuns } from '../lib/viewers/signals';
  import { formatVolts } from '../lib/viewers/units';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win, visible }: WindowProps = $props();

  const PLOT_S = 10;
  const PLOT_W = 300;
  const PLOT_H = 100;

  const core = useCore();
  const address = $derived(win.params.address ?? '');
  const partName = $derived(win.params.part ?? address.split('/')[0] ?? '');
  const part = $derived(core.parts?.find((p) => p.name === partName) ?? null);

  let fraction = $state<number | null>(null);
  let volts = $state<number | null>(null);
  let duty = $state(false);
  let runs = $state<string[]>([]);
  let error = $state<ErrorBody | null>(null);
  const history = new History(PLOT_S * 1_000_000);

  $effect(() =>
    core.watchSignal(address, 15, {
      onReading: (r) => {
        let value: number;
        if (r.capability === 'analog-in@1') {
          value = r.reading.fraction;
          volts = r.reading.volts ?? null;
          duty = false;
        } else if (r.capability === 'pwm-out@1') {
          value = r.reading.duty;
          volts = null;
          duty = true;
        } else return;
        history.push({ tUs: r.reading.tUs, value, ok: true });
        fraction = value;
        error = null;
        if (visible) {
          const now = history.latest?.tUs ?? 0;
          runs = plotRuns(history.samples, now, history.windowUs, 1, PLOT_W, PLOT_H);
        }
      },
      onError: (e) => (error = e),
    }),
  );

  const percent = $derived(
    fraction === null ? '' : `${Math.round(barFraction(fraction, 1) * 100)} %`,
  );
</script>

<div class="viewer">
  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {:else if fraction === null}
    <p class="pad quiet" role="status"><RichText text={t('signal.waiting', { address })} /></p>
  {:else}
    <div class="now">
      <p class="value" data-testid="fraction-value">{percent}</p>
      <p class="sub">
        {volts !== null
          ? t('fraction.withVolts', { what: t('fraction.ofFullScale'), volts: formatVolts(volts) })
          : duty
            ? t('fraction.duty')
            : t('fraction.ofFullScale')}
      </p>
    </div>
    <div
      class="bar"
      role="meter"
      aria-label={duty ? t('fraction.duty') : t('fraction.ofFullScale')}
      aria-valuemin={0}
      aria-valuemax={1}
      aria-valuenow={fraction}
      aria-valuetext={percent}
    >
      <span class="fill" style:width="{barFraction(fraction, 1) * 100}%"></span>
    </div>
    <figure class="plot">
      <svg viewBox="0 0 {PLOT_W} {PLOT_H}" preserveAspectRatio="none" aria-hidden="true">
        {#each runs as points, i (i)}
          <polyline {points} vector-effect="non-scaling-stroke" />
        {/each}
      </svg>
      <figcaption>{t('distance.plot.label', { seconds: PLOT_S })}</figcaption>
    </figure>
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
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-2);
  }
  .value {
    font-size: var(--text-xl);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .sub {
    color: var(--text-muted);
    font-size: var(--text-sm);
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
  }
  .plot {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: var(--space-3) var(--space-4);
    min-height: 90px;
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
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: auto;
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--line);
    background: var(--surface-2);
    font-size: var(--text-sm);
  }
</style>
