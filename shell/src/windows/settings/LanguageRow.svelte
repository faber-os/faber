<script lang="ts">
  import { useCore } from '../../lib/state/context';
  import { t } from '../../lib/i18n';
  import type { SectionProps } from './types';

  let { winId, settings, saving, change }: SectionProps = $props();

  const core = useCore();

  // A file dropped into config/translations shows up the next time Settings opens.
  $effect(() => {
    void core.loadTranslations();
  });

  const choices = $derived(core.languages(t('settings.appearance.language.english')));
  const chosen = $derived(settings.appearance.language);
  // A language whose translation has gone since (removed or broken) shows English (D44).
  const shown = $derived(choices.some((c) => c.code === chosen) ? chosen : 'en');

  function label(c: (typeof choices)[number]): string {
    return c.percent === null
      ? c.name
      : t('settings.appearance.language.option', { name: c.name, percent: c.percent });
  }

  function setLanguage(language: string) {
    change((s) => ({ ...s, appearance: { ...s.appearance, language } }));
  }
</script>

<!-- D44: no picker until a second language is installed. -->
{#if choices.length > 1}
  <div class="row">
    <div class="text">
      <label class="name" for="{winId}-language">{t('settings.appearance.language.name')}</label>
      <p class="desc">{t('settings.appearance.language.desc')}</p>
    </div>
    <select
      id="{winId}-language"
      class="input"
      value={shown}
      disabled={saving}
      onchange={(e) => setLanguage(e.currentTarget.value)}
    >
      {#each choices as c (c.code)}
        <option value={c.code}>{label(c)}</option>
      {/each}
    </select>
  </div>
{/if}
{#each core.translationProblems as p (p.code)}
  <p class="desc problem" role="status">
    {t('settings.appearance.language.broken', { problem: p.problem ?? '' })}
  </p>
{/each}

<style>
  .problem {
    color: var(--danger);
  }
</style>
