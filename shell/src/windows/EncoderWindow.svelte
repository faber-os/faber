<script lang="ts">
  // The viewer of an encoder@1 signal: its count, its speed (rad/s when it knows its counts per
  // turn, else counts/s) and a plot of the speed.
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { EncoderReading, ErrorBody } from '../lib/state/types';
  import { History } from '../lib/viewers/signals';
  import { polyline, valueRange } from '../lib/viewers/plotter';
  import { formatCountRate, formatTurnRate } from '../lib/viewers/units';
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

  let latest = $state<EncoderReading | null>(null);
  let line = $state('');
  let error = $state<ErrorBody | null>(null);
  const history = new History(PLOT_S * 1_000_000);

  const speed = (r: EncoderReading) =>
    r.velocityRadS !== undefined ? formatTurnRate(r.velocityRadS) : formatCountRate(r.countsPerS);

  $effect(() =>
    core.watchSignal(address, 15, {
      onReading: (r) => {
        if (r.capability !== 'encoder@1') return;
        history.push({ tUs: r.reading.tUs, value: r.reading.countsPerS, ok: true });
        latest = r.reading;
        error = null;
        if (visible) {
          const now = history.latest?.tUs ?? 0;
          const range = valueRange([history.samples]);
          line = polyline(history.samples, now, history.windowUs, range, PLOT_W, PLOT_H);
        }
      },
      onError: (e) => (error = e),
    }),
  );
</script>

<div class="viewer">
  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {:else if !latest}
    <p class="pad quiet" role="status"><RichText text={t('signal.waiting', { address })} /></p>
  {:else}
    <div class="now">
      <p class="value" data-testid="encoder-count">{t('encoder.count', { count: latest.count })}</p>
      <p class="sub" data-testid="encoder-speed">{speed(latest)}</p>
    </div>
    <figure class="plot">
      <svg viewBox="0 0 {PLOT_W} {PLOT_H}" preserveAspectRatio="none" aria-hidden="true">
        <polyline points={line} vector-effect="non-scaling-stroke" />
      </svg>
      <figcaption>{t('encoder.plot', { seconds: PLOT_S })}</figcaption>
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
    font-variant-numeric: tabular-nums;
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
