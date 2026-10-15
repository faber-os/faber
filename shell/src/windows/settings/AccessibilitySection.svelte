<script lang="ts">
  import { TEXT_SIZES } from '../../lib/desktop/accessibility';
  import type { Accessibility } from '../../lib/state/types';
  import ChoiceRow from './ChoiceRow.svelte';
  import type { SectionProps } from './types';

  let { winId, settings, saving, change }: SectionProps = $props();

  const FOLLOW = [
    ['system', 'System'],
    ['on', 'On'],
    ['off', 'Off'],
  ] as const;
  const SIZES = TEXT_SIZES.map((s) => [s, `${s} %`] as const);

  function set(patch: Partial<Accessibility>) {
    change((s) => ({ ...s, accessibility: { ...s.accessibility, ...patch } }));
  }
</script>

<section id="{winId}-accessibility" aria-labelledby="{winId}-accessibility-h">
  <h3 id="{winId}-accessibility-h">Accessibility</h3>
  <p class="intro">
    These apply to every screen, the attached one too. System follows what this screen or browser
    asks for.
  </p>
  <ChoiceRow
    id="{winId}-motion"
    label="Reduce motion"
    description="No window animations or sliding."
    options={FOLLOW}
    value={settings.accessibility.reduceMotion}
    disabled={saving}
    onchange={(v) => set({ reduceMotion: v })}
  />
  <ChoiceRow
    id="{winId}-contrast"
    label="Higher contrast"
    description="Stronger text, lines and focus rings, for small screens and bright rooms."
    options={FOLLOW}
    value={settings.accessibility.higherContrast}
    disabled={saving}
    onchange={(v) => set({ higherContrast: v })}
  />
  <ChoiceRow
    id="{winId}-text-size"
    label="Text size"
    description="Larger text everywhere in Faber, e.g. for the 7&quot; touch screen."
    options={SIZES}
    value={settings.accessibility.textSize}
    disabled={saving}
    onchange={(v) => set({ textSize: v })}
  />
</section>
