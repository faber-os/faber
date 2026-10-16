<script lang="ts">
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody, TemperatureReading } from '../lib/state/types';
  import { History } from '../lib/viewers/signals';
  import { polyline, valueRange } from '../lib/viewers/plotter';
  import { formatCelsius } from '../lib/viewers/units';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win, visible }: WindowProps = $props();

  const PLOT_S = 120;
  const PLOT_W = 300;
  const PLOT_H = 100;

  const core = useCore();
  const address = $derived(win.params.address ?? '');
  const partName = $derived(win.params.part ?? address.split('/')[0] ?? '');
  const part = $derived(core.parts?.find((p) => p.name === partName) ?? null);

  let latest = $state<TemperatureReading | null>(null);
  let line = $state('');
  let range = $state({ min: 0, max: 1 });
  let error = $state<ErrorBody | null>(null);
  const history = new History(PLOT_S * 1_000_000);

  $effect(() =>
    core.watchSignal(address, 5, {
      onReading: (r) => {
        if (r.capability !== 'temperature@1') return;
        history.push({ tUs: r.reading.tUs, value: r.reading.celsius, ok: true });
        latest = r.reading;
        error = null;
        if (visible) redraw();
      },
      onError: (e) => (error = e),
    }),
  );

  function redraw() {
    range = valueRange([history.samples]);
    const now = history.latest?.tUs ?? 0;
    line = polyline(history.samples, now, history.windowUs, range, PLOT_W, PLOT_H);
  }
</script>

<div class="viewer">
  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {:else if !latest}
    <p class="pad quiet" role="status"><RichText text={t('signal.waiting', { address })} /></p>
  {:else}
    <p class="value" data-testid="temperature-value">{formatCelsius(latest.celsius)}</p>
    <figure class="plot">
      <svg viewBox="0 0 {PLOT_W} {PLOT_H}" preserveAspectRatio="none" aria-hidden="true">
        <polyline points={line} vector-effect="non-scaling-stroke" />
      </svg>
      <figcaption>
        {t('signal.plotRange', {
          seconds: PLOT_S,
          min: formatCelsius(range.min),
          max: formatCelsius(range.max),
        })}
      </figcaption>
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
  .value {
    padding: var(--space-4) var(--space-4) var(--space-2);
    font-size: var(--text-xl);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .plot {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: var(--space-2) var(--space-4);
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
