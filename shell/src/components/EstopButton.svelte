<script lang="ts">
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';

  /**
   * The e-stop on the taskbar (docs/simulation.md, section 9): a red Stop whenever something can
   * move, which engages at once; while engaged, "Stopped" and a Release that asks first. The
   * Pause/Break key engages it from anywhere (Escape never does: it closes menus).
   */
  let { large = false }: { large?: boolean } = $props();

  const core = useCore();
  const engaged = $derived(core.estop?.engaged === true);
  const shown = $derived(engaged || core.movableParts.length > 0);
  const reason = $derived(core.estop?.engagedBy?.reason ?? '');

  let confirming = $state(false);

  function engage(why: string) {
    void core.engageEstop(why);
  }

  async function release() {
    confirming = false;
    const r = await core.releaseEstop();
    if (!r.ok) core.lastError = r.error;
  }

  // Capture phase on the window: no window can stop the key on its way.
  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Pause' && e.code !== 'Pause') return;
      e.preventDefault();
      engage(t('estop.reason.key'));
    };
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  });
</script>

{#if engaged}
  <div class="engaged" class:large>
    <span class="stopped" role="status" title={t('estop.engaged.detail', { reason })}>
      <Icon name="stop" size={16} />
      {t('estop.engaged')}
    </span>
    <button class="release" onclick={() => (confirming = true)}>{t('estop.release')}</button>
  </div>
{:else if shown}
  <button
    class="stop"
    class:large
    title={t('estop.stop.title')}
    onclick={() => engage(t('estop.reason.taskbar'))}
  >
    <Icon name="stop" size={large ? 20 : 16} />
    {t('estop.stop')}
  </button>
{/if}

{#if confirming}
  <div class="sheet">
    <Dialog title={t('estop.release.title')} oncancel={() => (confirming = false)} tone="danger">
      <p>{t('estop.release.body')}</p>
      {#snippet actions()}
        <button class="btn" data-autofocus onclick={() => (confirming = false)}
          >{t('estop.release.cancel')}</button
        >
        <button class="btn danger solid" onclick={release}>{t('estop.release.confirm')}</button>
      {/snippet}
    </Dialog>
  </div>
{/if}

<style>
  .stop {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: 36px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--danger);
    color: var(--surface);
    font-weight: 750;
    letter-spacing: 0.01em;
    white-space: nowrap;
  }
  .stop:hover {
    filter: brightness(1.08);
  }
  .stop.large {
    height: 48px;
    padding: 0 var(--space-4);
    font-size: var(--text-lg);
  }
  .engaged {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .stopped {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: 36px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
    background: var(--danger-soft);
    color: var(--danger);
    font-weight: 750;
    white-space: nowrap;
  }
  .large .stopped {
    height: 48px;
  }
  .release {
    height: 36px;
    padding: 0 var(--space-3);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-md);
    background: var(--surface);
    font-weight: 650;
  }
  .large .release {
    height: 48px;
  }
  /* The question covers the whole desktop, above every window. */
  .sheet {
    position: absolute;
    inset: 0;
    z-index: 100000;
  }
</style>
