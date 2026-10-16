<script lang="ts">
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody, GuidedBoard, PartTemplate } from '../lib/state/types';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import type { WindowProps } from '../components/windowDefs';
  import WiringChecklist from './guided/WiringChecklist.svelte';

  /**
   * A guided board's wiring checklist in its own window (docs/wiring.md §3, step 4), so it can
   * stay open beside the board while wiring. Its ticks are the same as the wizard's.
   */
  let { win }: WindowProps = $props();

  const core = useCore();
  let board = $state.raw<GuidedBoard | null>(null);
  let templates = $state.raw<PartTemplate[]>([]);
  let error = $state.raw<ErrorBody | null>(null);
  const name = $derived(win.params.plan ?? '');

  async function load() {
    if (!name) return;
    error = null;
    const [r, tpl] = await Promise.all([core.plan(name), core.partTemplates()]);
    if (r.ok) board = r.value;
    else error = r.error;
    if (tpl.ok) templates = tpl.value;
  }

  $effect(() => {
    void name;
    void load();
  });
</script>

<div class="wiring">
  <div class="toolbar">
    <button class="btn quiet" onclick={load} title={t('guided.wiring.reload')}
      ><Icon name="refresh" size={16} /> {t('guided.wiring.reload')}</button
    >
  </div>
  <div class="scroll">
    {#if error}
      <ErrorNotice {error} />
    {:else if board}
      <WiringChecklist {board} {templates} />
    {:else}
      <p class="muted">{t('guided.loading')}</p>
    {/if}
  </div>
</div>

<style>
  .wiring {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .toolbar {
    display: flex;
    justify-content: flex-end;
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-3) var(--space-4) var(--space-4);
  }
  .muted {
    color: var(--text-muted);
  }
</style>
