<script lang="ts">
  import type { Units } from '../../lib/state/types';
  import ChoiceRow from './ChoiceRow.svelte';
  import type { SectionProps } from './types';
  import { t } from '../../lib/i18n';

  let { winId, settings, saving, change }: SectionProps = $props();

  function set(patch: Partial<Units>) {
    change((s) => ({ ...s, units: { ...s.units, ...patch } }));
  }
</script>

<section id="{winId}-units" aria-labelledby="{winId}-units-h">
  <h3 id="{winId}-units-h">{t('settings.units.title')}</h3>
  <p class="intro">{t('settings.units.intro')}</p>
  <ChoiceRow
    id="{winId}-length"
    label={t('settings.units.length.name')}
    description={t('settings.units.length.desc')}
    options={[
      ['metric', t('settings.units.length.metric')],
      ['imperial', t('settings.units.length.imperial')],
    ]}
    value={settings.units.length}
    disabled={saving}
    onchange={(v) => set({ length: v })}
  />
</section>
