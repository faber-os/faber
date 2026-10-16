<script lang="ts">
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { BatteryReading, ErrorBody } from '../lib/state/types';
  import { chemistryWords, levelWords, percent, voltageShare } from '../lib/viewers/battery';
  import { formatAmps, formatCelsius, formatVolts } from '../lib/viewers/units';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';

  /**
   * A `battery@1` signal: its level in words, the voltage, how full it is, and what else the
   * battery reports (current, temperature, cells). A battery that reports no charge of its own
   * gets a gauge from its voltage between empty and full.
   */
  let { win }: WindowProps = $props();

  const MAX_HZ = 5;

  const core = useCore();

  const address = $derived(win.params.address ?? '');
  const partName = $derived(win.params.part ?? address.split('/')[0] ?? '');
  const part = $derived(core.parts?.find((p) => p.name === partName) ?? null);
  const signal = $derived(part?.signals.find((s) => s.address === address) ?? null);
  const info = $derived(signal?.info?.capability === 'battery@1' ? signal.info.info : null);
  const volts = $derived(signal?.battery ?? null);

  let latest = $state.raw<BatteryReading | null>(null);
  let error = $state.raw<ErrorBody | null>(null);

  $effect(() => {
    return core.watchSignal(address, MAX_HZ, {
      onReading: (r) => {
        if (r.capability !== 'battery@1') return;
        latest = r.reading;
        error = null;
      },
      onError: (e) => (error = e),
    });
  });

  const share = $derived.by(() => {
    if (!latest) return null;
    if (latest.charge !== undefined) return latest.charge;
    return volts ? voltageShare(latest.volts, volts.empty, volts.full) : null;
  });
</script>

<div class="viewer">
  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {:else if !latest}
    <div class="pad quiet" role="status">
      <p><RichText text={t('battery.waiting', { address })} /></p>
    </div>
  {:else}
    <div class="head" data-level={latest.level}>
      <p class="level" data-testid="battery-level">{levelWords(latest.level)}</p>
      <p class="volts num" data-testid="battery-volts">{formatVolts(latest.volts)}</p>
    </div>
    {#if share !== null}
      <div class="pad gauge">
        <div
          class="bar"
          role="meter"
          aria-label={t('battery.charge.label')}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent(share)}
          data-level={latest.level}
        >
          <span style:width="{percent(share)}%"></span>
        </div>
      </div>
    {/if}
    <dl class="facts" data-testid="battery">
      {#if latest.charge !== undefined}
        <div>
          <dt>{t('battery.charge')}</dt>
          <dd class="num">{t('battery.charge.value', { percent: percent(latest.charge) })}</dd>
        </div>
      {/if}
      {#if latest.amps !== undefined}
        <div>
          <dt>{t('battery.current')}</dt>
          <dd class="num">
            {latest.amps < 0
              ? t('battery.current.discharging', { amps: formatAmps(-latest.amps) })
              : t('battery.current.charging', { amps: formatAmps(latest.amps) })}
          </dd>
        </div>
      {/if}
      {#if latest.celsius !== undefined}
        <div>
          <dt>{t('battery.temperature')}</dt>
          <dd class="num">{formatCelsius(latest.celsius)}</dd>
        </div>
      {/if}
      {#if info}
        <div class="wide">
          <dt>{t('battery.kind')}</dt>
          <dd>
            {info.capacityAh === undefined
              ? t('battery.kind.value', {
                  chemistry: chemistryWords(info.chemistry),
                  cells: info.cellsInSeries,
                })
              : t('battery.kind.capacity', {
                  chemistry: chemistryWords(info.chemistry),
                  cells: info.cellsInSeries,
                  capacity: info.capacityAh,
                })}
          </dd>
          {#if volts}
            <dd class="muted">
              {t('battery.thresholds', {
                low: formatVolts(volts.low),
                critical: formatVolts(volts.critical),
              })}
            </dd>
          {/if}
        </div>
      {/if}
      {#if latest.cellVolts?.length}
        <div class="wide">
          <dt>{t('battery.cells')}</dt>
          <dd>
            <ul class="cells">
              {#each latest.cellVolts as v, i (i)}
                <li>
                  <span class="muted">{t('battery.cell', { index: i + 1 })}</span>
                  <span class="num">{formatVolts(v)}</span>
                </li>
              {/each}
            </ul>
          </dd>
        </div>
      {/if}
    </dl>
  {/if}

  <dl class="stats">
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
    font-size: var(--text-sm);
  }
  .muted {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) 0;
  }
  .level {
    font-weight: 700;
    font-size: var(--text-md);
    padding: 2px var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--ok-soft);
  }
  [data-level='low'] .level {
    background: var(--warn-soft);
  }
  [data-level='critical'] .level {
    background: var(--danger-soft);
    color: var(--danger-text);
  }
  .volts {
    font-size: var(--text-xl);
    font-weight: 700;
  }
  .gauge {
    padding-top: var(--space-3);
    padding-bottom: 0;
  }
  .bar {
    height: 14px;
    border-radius: 7px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--ok);
  }
  .bar[data-level='low'] span {
    background: var(--warn);
  }
  .bar[data-level='critical'] span {
    background: var(--danger);
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(9rem, 1fr));
    gap: var(--space-3) var(--space-4);
    margin: 0;
    padding: var(--space-4);
  }
  .wide {
    grid-column: 1 / -1;
  }
  dt {
    font-size: var(--text-xs);
    font-weight: 700;
    color: var(--text-muted);
  }
  dd {
    margin: 0;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .cells {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(7rem, 1fr));
    gap: var(--space-1) var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .cells li {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
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
  }
  .addr {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
  }
</style>
