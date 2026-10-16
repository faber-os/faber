<script lang="ts">
  // The viewer of an imu@1 signal: how fast it turns and how it is accelerated on each of its
  // axes (gravity included: lying still it reads about 9.81 m/s² up), and a plot of the turn
  // about z. An IMU without fusion has no orientation, so none is drawn.
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody, ImuReading } from '../lib/state/types';
  import { History } from '../lib/viewers/signals';
  import { polyline, valueRange } from '../lib/viewers/plotter';
  import { formatMetresPerS2, formatTurnRate } from '../lib/viewers/units';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win, visible }: WindowProps = $props();

  const PLOT_S = 10;
  const PLOT_W = 300;
  const PLOT_H = 100;
  const AXES = ['x', 'y', 'z'] as const;

  const core = useCore();
  const address = $derived(win.params.address ?? '');
  const partName = $derived(win.params.part ?? address.split('/')[0] ?? '');
  const part = $derived(core.parts?.find((p) => p.name === partName) ?? null);

  let latest = $state<ImuReading | null>(null);
  let line = $state('');
  let error = $state<ErrorBody | null>(null);
  const history = new History(PLOT_S * 1_000_000);

  $effect(() =>
    core.watchSignal(address, 15, {
      onReading: (r) => {
        if (r.capability !== 'imu@1') return;
        history.push({ tUs: r.reading.tUs, value: r.reading.angularVelocity.z, ok: true });
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
    <table class="axes">
      <thead>
        <tr>
          <th></th>
          {#each AXES as a (a)}<th>{a}</th>{/each}
        </tr>
      </thead>
      <tbody>
        <tr data-testid="imu-turn">
          <th>{t('imu.turn')}</th>
          {#each AXES as a (a)}<td>{formatTurnRate(latest.angularVelocity[a])}</td>{/each}
        </tr>
        <tr data-testid="imu-acceleration">
          <th>{t('imu.acceleration')}</th>
          {#each AXES as a (a)}<td>{formatMetresPerS2(latest.linearAcceleration[a])}</td>{/each}
        </tr>
      </tbody>
    </table>
    <figure class="plot">
      <svg viewBox="0 0 {PLOT_W} {PLOT_H}" preserveAspectRatio="none" aria-hidden="true">
        <polyline points={line} vector-effect="non-scaling-stroke" />
      </svg>
      <figcaption>{t('imu.plot', { seconds: PLOT_S })}</figcaption>
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
  .axes {
    margin: var(--space-4) var(--space-4) 0;
    border-collapse: collapse;
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }
  .axes th,
  .axes td {
    padding: 2px var(--space-2);
    text-align: right;
  }
  .axes tbody th {
    text-align: left;
    color: var(--text-muted);
    font-weight: 500;
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
