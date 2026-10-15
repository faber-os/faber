<script lang="ts">
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody, PositionReading } from '../lib/state/types';
  import {
    fixWords,
    hasFix,
    latitudeWords,
    longitudeWords,
    mapLink,
  } from '../lib/viewers/position';
  import { formatLength } from '../lib/viewers/units';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';

  /**
   * A `position@1` signal (a GPS): coordinates, fix and satellites. No map tiles: when this
   * computer is online it links out to OpenStreetMap instead.
   */
  let { win }: WindowProps = $props();

  const MAX_HZ = 5;

  const core = useCore();

  const address = $derived(win.params.address ?? '');
  const partName = $derived(win.params.part ?? address.split('/')[0] ?? '');
  const part = $derived(core.parts?.find((p) => p.name === partName) ?? null);
  const unit = $derived(core.settings?.units.length ?? 'metric');

  let latest = $state.raw<PositionReading | null>(null);
  let error = $state.raw<ErrorBody | null>(null);
  let online = $state(typeof navigator === 'undefined' ? false : navigator.onLine);

  $effect(() => {
    return core.watchSignal(address, MAX_HZ, {
      onReading: (r) => {
        if (r.capability !== 'position@1') return;
        latest = r.reading;
        error = null;
      },
      onError: (e) => (error = e),
    });
  });

  $effect(() => {
    const update = () => (online = navigator.onLine);
    window.addEventListener('online', update);
    window.addEventListener('offline', update);
    return () => {
      window.removeEventListener('online', update);
      window.removeEventListener('offline', update);
    };
  });
</script>

<div class="viewer">
  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {:else if !latest}
    <div class="pad quiet" role="status">
      <p><RichText text={t('position.waiting', { address })} /></p>
    </div>
  {:else}
    {#if !hasFix(latest.fix)}
      <div class="pad">
        <ErrorNotice
          tone="warn"
          error={{ message: t('position.noFix'), hint: t('position.noFix.hint') }}
        />
      </div>
    {/if}
    <dl class="facts" class:stale={!hasFix(latest.fix)} data-testid="position">
      <div>
        <dt>{t('position.latitude')}</dt>
        <dd class="num big">{latitudeWords(latest.latitudeDeg)}</dd>
      </div>
      <div>
        <dt>{t('position.longitude')}</dt>
        <dd class="num big">{longitudeWords(latest.longitudeDeg)}</dd>
      </div>
      <div>
        <dt>{t('position.fix')}</dt>
        <dd>{fixWords(latest.fix)}</dd>
      </div>
      <div>
        <dt>{t('position.satellites')}</dt>
        <dd class="num">{latest.satellites}</dd>
      </div>
      {#if latest.altitudeM !== undefined}
        <div>
          <dt>{t('position.altitude')}</dt>
          <dd class="num">{formatLength(latest.altitudeM, unit)}</dd>
        </div>
      {/if}
      {#if latest.horizontalAccuracyM !== undefined}
        <div>
          <dt>{t('position.accuracy')}</dt>
          <dd class="num">
            {t('position.accuracyValue', { value: formatLength(latest.horizontalAccuracyM, unit) })}
          </dd>
        </div>
      {/if}
    </dl>
    {#if hasFix(latest.fix)}
      <p class="pad map">
        {#if online}
          <a
            class="btn"
            href={mapLink(latest.latitudeDeg, latest.longitudeDeg)}
            target="_blank"
            rel="noopener noreferrer">{t('position.openMap')}</a
          >
        {:else}
          <span class="muted">{t('position.offline')}</span>
        {/if}
      </p>
    {/if}
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
  .facts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(10rem, 1fr));
    gap: var(--space-3) var(--space-4);
    margin: 0;
    padding: var(--space-4);
  }
  .stale {
    opacity: 0.55;
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
  .big {
    font-size: var(--text-lg);
    font-weight: 700;
  }
  .map {
    padding-top: 0;
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
