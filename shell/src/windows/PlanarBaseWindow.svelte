<script lang="ts">
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import { baseInfo } from '../lib/state/partLimits';
  import type { ErrorBody, PlanarBaseReading } from '../lib/state/types';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import TwistGauges from '../components/TwistGauges.svelte';
  import { useDesktop } from '../components/desktopController.svelte';
  import { openDrive } from '../components/driveWindow';
  import type { WindowProps } from '../components/windowDefs';

  let { win }: WindowProps = $props();

  const MAX_HZ = 15;

  const core = useCore();
  const desktop = useDesktop();

  const address = $derived(win.params.address ?? '');
  const partName = $derived(win.params.part ?? address.split('/')[0] ?? '');
  const part = $derived(core.parts?.find((p) => p.name === partName) ?? null);
  const info = $derived(part ? baseInfo(part) : null);
  const lease = $derived(core.leaseOn(partName));
  const unit = $derived(core.settings?.units.length ?? 'metric');

  let latest = $state<PlanarBaseReading | null>(null);
  let error = $state<ErrorBody | null>(null);

  $effect(() => {
    const stop = core.watchSignal(address, MAX_HZ, {
      onReading: (r) => {
        if (r.capability !== 'planar-base@1') return;
        latest = r.reading;
        error = null;
      },
      onError: (e) => (error = e),
    });
    return stop;
  });
</script>

<div class="viewer">
  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {:else if !latest}
    <div class="pad quiet" role="status">
      <p><RichText text={t('base.waiting', { address })} /></p>
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

  {#if latest && info}
    <div class="gauges">
      <div class="legend" aria-hidden="true">
        <span class="key bar"></span>{t('twist.commanded')}
        {#if latest.measured}<span class="key mark"></span>{t('twist.measured')}{/if}
      </div>
      <TwistGauges
        commanded={latest.commanded}
        measured={latest.measured ?? null}
        max={info.maxVelocity}
        {unit}
        holonomic={info.holonomic}
      />
      {#if latest.bumped}
        <p class="bumped" role="status"><Icon name="warning" size={16} /> {t('twist.bumped')}</p>
      {/if}
    </div>
  {/if}

  <div class="driver">
    <p>
      {lease ? t('base.drivenBy', { holder: lease.holder }) : t('base.free')}
    </p>
    <button class="btn" onclick={() => openDrive(desktop, partName)}>{t('base.drive')}</button>
  </div>

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
    display: grid;
    gap: var(--space-1);
    font-size: var(--text-sm);
  }
  .sub {
    color: var(--text-muted);
  }
  .gauges {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-4);
  }
  .legend {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .key {
    display: inline-block;
    width: 14px;
    height: 8px;
    border-radius: 2px;
  }
  .key.bar {
    background: var(--accent);
  }
  .key.mark {
    width: 3px;
    height: 12px;
    margin-left: var(--space-2);
    background: var(--text);
  }
  .bumped {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--warn-text);
    font-weight: 600;
    font-size: var(--text-sm);
  }
  .driver {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--line);
    font-size: var(--text-sm);
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
