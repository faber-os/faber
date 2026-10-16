<script lang="ts">
  import { machineNameProblem, tidyMachineName } from '../../lib/desktop/machineName';
  import type { SectionProps } from './types';
  import RichText from '../../components/RichText.svelte';
  import { t } from '../../lib/i18n';

  let { winId, settings, saving, change }: SectionProps = $props();

  let draft = $state('');
  // A change from elsewhere (another browser, a restore) replaces the draft.
  $effect(() => {
    draft = settings.machine.name;
  });
  const problem = $derived(machineNameProblem(draft));
  const dirty = $derived(draft !== settings.machine.name);

  function save() {
    draft = tidyMachineName(draft);
    if (machineNameProblem(draft) || draft === settings.machine.name) return;
    const name = draft;
    change((s) => ({ ...s, machine: { ...s.machine, name } }));
  }
</script>

<section id="{winId}-machine" aria-labelledby="{winId}-machine-h">
  <h3 id="{winId}-machine-h">{t('settings.machine.title')}</h3>
  <form
    class="row"
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}
  >
    <div class="text">
      <label class="name" for="{winId}-machine-name">{t('settings.machine.name.label')}</label>
      <p class="desc">
        <RichText
          text={t('settings.machine.name.desc', {
            address: `${(problem ? settings.machine.name : draft) || 'faber'}.local`,
          })}
        />
      </p>
      {#if problem && dirty}<p class="problem" role="alert">{problem}</p>{/if}
    </div>
    <div class="controls">
      <input
        id="{winId}-machine-name"
        class="input"
        bind:value={draft}
        onblur={save}
        disabled={saving}
        maxlength="63"
        autocomplete="off"
        spellcheck="false"
        aria-invalid={problem !== null && dirty}
      />
    </div>
  </form>
</section>

<style>
  .controls {
    flex: 0 1 14rem;
    min-width: 0;
  }
  .controls .input {
    width: 100%;
  }
  .problem {
    font-size: var(--text-sm);
    color: var(--danger-text);
  }
</style>
