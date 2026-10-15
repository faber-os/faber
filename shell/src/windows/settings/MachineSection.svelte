<script lang="ts">
  import { machineNameProblem, tidyMachineName } from '../../lib/desktop/machineName';
  import type { SectionProps } from './types';

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
  <h3 id="{winId}-machine-h">Name</h3>
  <form
    class="row"
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}
  >
    <div class="text">
      <label class="name" for="{winId}-machine-name">Machine name</label>
      <p class="desc">
        Shown in the browser tab, in About and in backup names. On the Pi it is also the network
        address: <code>{(problem ? settings.machine.name : draft) || 'faber'}.local</code>.
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
    color: var(--danger);
  }
</style>
