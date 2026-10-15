<script lang="ts">
  import { useCore } from '../../lib/state/context';
  import { formatClockTime, formatLongDate, timeZones } from '../../lib/desktop/wallClock';
  import ChoiceRow from './ChoiceRow.svelte';
  import type { SectionProps } from './types';

  let { winId, settings, saving, change }: SectionProps = $props();

  const core = useCore();
  const zones = timeZones();
  const machineZone = $derived(core.system?.timeZone ?? 'not known yet');
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
  <h3 id="{winId}-time-h">Date and time</h3>
  <p class="intro">
    Clocks, file dates and log times show this machine's time in every browser, so a machine in
    another room or country shows its own time.
  </p>
  <div class="row">
    <div class="text">
      <label class="name" for="{winId}-zone">Time zone</label>
      <p class="desc">
        Now {formatClockTime(now, core.wallClock)}, {formatLongDate(now, core.wallClock)}.
      </p>
    </div>
    <select
      id="{winId}-zone"
      class="input zone"
      value={chosen}
      disabled={saving}
      onchange={(e) => setZone(e.currentTarget.value)}
    >
      <option value="">Machine's own ({machineZone})</option>
      {#each listed as z (z)}
        <option value={z}>{z.replaceAll('_', ' ')}</option>
      {/each}
    </select>
  </div>
  <ChoiceRow
    id="{winId}-clock"
    label="Clock"
    description="For the taskbar clock, file dates and log times."
    options={[
      ['24h', '24-hour (14:30)'],
      ['12h', '12-hour (2:30 pm)'],
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
