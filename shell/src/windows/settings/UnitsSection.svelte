<script lang="ts">
  import type { Units } from '../../lib/state/types';
  import ChoiceRow from './ChoiceRow.svelte';
  import type { SectionProps } from './types';

  let { winId, settings, saving, change }: SectionProps = $props();

  function set(patch: Partial<Units>) {
    change((s) => ({ ...s, units: { ...s.units, ...patch } }));
  }
</script>

<section id="{winId}-units" aria-labelledby="{winId}-units-h">
  <h3 id="{winId}-units-h">Units</h3>
  <p class="intro">How Faber shows measurements, e.g. the rings in the laser-scan viewer.</p>
  <ChoiceRow
    id="{winId}-length"
    label="Lengths"
    description="Metres and centimetres, or feet and inches."
    options={[
      ['metric', 'Metric (m, cm)'],
      ['imperial', 'Imperial (ft, in)'],
    ]}
    value={settings.units.length}
    disabled={saving}
    onchange={(v) => set({ length: v })}
  />
</section>
