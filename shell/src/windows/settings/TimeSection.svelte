<script lang="ts">
  import { useCore } from '../../lib/state/context';
  import { formatClockTime, formatLongDate, timeZones } from '../../lib/desktop/wallClock';
  import ChoiceRow from './ChoiceRow.svelte';
  import type { SectionProps } from './types';
  import RoleLock from '../../components/RoleLock.svelte';
  import { t } from '../../lib/i18n';

  let { winId, settings, saving, change, lock = null }: SectionProps = $props();

  const core = useCore();
  const zones = timeZones();
  const machineZone = $derived(core.system?.timeZone ?? t('settings.time.zone.unknown'));
  const chosen = $derived(settings.time.timeZone);
  // A zone from the settings that this browser can't list still shows, so the picker never lies.
  const listed = $derived(chosen && !zones.includes(chosen) ? [chosen, ...zones] : zones);
  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 15_000);
    return () => clearInterval(t);
  });

  function setZone(timeZone: string) {
    change((s) => ({ ...s, time: { ...s.time, timeZone } }));
  }
</script>

<section id="{winId}-time" aria-labelledby="{winId}-time-h">
  <h3 id="{winId}-time-h">{t('settings.time.title')}</h3>
  <RoleLock reason={lock} />
  <p class="intro">{t('settings.time.intro')}</p>
  <div class="row">
    <div class="text">
      <label class="name" for="{winId}-zone">{t('settings.time.zone.name')}</label>
      <p class="desc">
        {t('settings.time.zone.now', {
          time: formatClockTime(now, core.wallClock),
          date: formatLongDate(now, core.wallClock),
        })}
      </p>
    </div>
    <select
      id="{winId}-zone"
      class="input zone"
      value={chosen}
      disabled={saving}
      onchange={(e) => setZone(e.currentTarget.value)}
    >
      <option value="">{t('settings.time.zone.machine', { zone: machineZone })}</option>
      {#each listed as z (z)}
        <option value={z}>{z.replaceAll('_', ' ')}</option>
      {/each}
    </select>
  </div>
  <ChoiceRow
    id="{winId}-clock"
    label={t('settings.time.clock.name')}
    description={t('settings.time.clock.desc')}
    options={[
      ['24h', t('settings.time.clock.24h')],
      ['12h', t('settings.time.clock.12h')],
    ]}
    value={settings.time.clock}
    disabled={saving}
    onchange={(clock) => change((s) => ({ ...s, time: { ...s.time, clock } }))}
  />
</section>

<style>
  .zone {
    flex: 0 1 18rem;
    min-width: 0;
    max-width: 100%;
  }
</style>
