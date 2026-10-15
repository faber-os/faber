<script lang="ts">
  import { TEXT_SIZES } from '../../lib/desktop/accessibility';
  import type { Accessibility } from '../../lib/state/types';
  import ChoiceRow from './ChoiceRow.svelte';
  import type { SectionProps } from './types';
  import { t } from '../../lib/i18n';

  let { winId, settings, saving, change }: SectionProps = $props();

  const follow = $derived([
    ['system', t('settings.accessibility.follow.system')],
    ['on', t('settings.accessibility.follow.on')],
    ['off', t('settings.accessibility.follow.off')],
  ] as const);
  const sizes = $derived(
    TEXT_SIZES.map((s) => [s, t('settings.accessibility.textSize.option', { size: s })] as const),
  );

  function set(patch: Partial<Accessibility>) {
    change((s) => ({ ...s, accessibility: { ...s.accessibility, ...patch } }));
  }
</script>

<section id="{winId}-accessibility" aria-labelledby="{winId}-accessibility-h">
  <h3 id="{winId}-accessibility-h">{t('settings.accessibility.title')}</h3>
  <p class="intro">{t('settings.accessibility.intro')}</p>
  <ChoiceRow
    id="{winId}-motion"
    label={t('settings.accessibility.motion.name')}
    description={t('settings.accessibility.motion.desc')}
    options={follow}
    value={settings.accessibility.reduceMotion}
    disabled={saving}
    onchange={(v) => set({ reduceMotion: v })}
  />
  <ChoiceRow
    id="{winId}-contrast"
    label={t('settings.accessibility.contrast.name')}
    description={t('settings.accessibility.contrast.desc')}
    options={follow}
    value={settings.accessibility.higherContrast}
    disabled={saving}
    onchange={(v) => set({ higherContrast: v })}
  />
  <ChoiceRow
    id="{winId}-text-size"
    label={t('settings.accessibility.textSize.name')}
    description={t('settings.accessibility.textSize.desc')}
    options={sizes}
    value={settings.accessibility.textSize}
    disabled={saving}
    onchange={(v) => set({ textSize: v })}
  />
</section>
