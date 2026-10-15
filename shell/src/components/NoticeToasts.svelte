<script lang="ts">
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import { noticeView } from '../lib/state/notices';
  import ErrorNotice from './ErrorNotice.svelte';

  /** Notices from the core (a battery's level changed) as toasts, newest at the bottom. */
  const core = useCore();

  /** How long good news stays (ms); warnings stay until dismissed. */
  const OK_MS = 8000;

  const shown = $derived(core.notices.map((n) => ({ key: n.key, view: noticeView(n.notice) })));

  $effect(() => {
    const timers = shown
      .filter((n) => !n.view.sticky)
      .map((n) => setTimeout(() => core.dismissNotice(n.key), OK_MS));
    return () => timers.forEach(clearTimeout);
  });
</script>

{#if shown.length}
  <section class="notices" aria-label={t('notice.region')}>
    {#each shown as n (n.key)}
      <div class="toast" data-tone={n.view.tone}>
        <ErrorNotice tone={n.view.tone} error={{ message: n.view.message, hint: n.view.hint }}>
          {#snippet actions()}
            <button class="btn quiet" onclick={() => core.dismissNotice(n.key)}
              >{t('desktop.toast.dismiss')}</button
            >
          {/snippet}
        </ErrorNotice>
      </div>
    {/each}
  </section>
{/if}

<style>
  .notices {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .toast {
    border-radius: var(--radius-md);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
  }
</style>
