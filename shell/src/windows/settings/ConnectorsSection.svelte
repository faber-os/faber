<script lang="ts">
  import { untrack } from 'svelte';
  import { useCore } from '../../lib/state/context';
  import type { ErrorBody, PackageInfo } from '../../lib/state/types';
  import { kindWords, partList } from '../../lib/desktop/packages';
  import Dialog from '../../components/Dialog.svelte';
  import ErrorNotice from '../../components/ErrorNotice.svelte';
  import RichText from '../../components/RichText.svelte';
  import { t } from '../../lib/i18n';

  /**
   * Settings → Connectors (Advanced): the installed connector packages, with Uninstall, and the
   * installed connectors the core couldn't load, each with what to do (docs/packages.md).
   */
  let { winId }: { winId: string } = $props();

  const core = useCore();
  /** Connector packages only: apps have their own tool (Apps). */
  const packages = $derived(core.packages?.filter((p) => p.kind !== 'app') ?? null);

  let error = $state.raw<ErrorBody | null>(null);
  let confirming = $state.raw<PackageInfo | null>(null);
  let removing = $state(false);

  $effect(() =>
    untrack(() => {
      void core.loadPackages().then((r) => {
        if (!r.ok) error = r.error;
      });
      void core.loadConnectors();
    }),
  );

  // Which parts use a package changes with the parts: ask again when they change.
  $effect(() => {
    if (core.parts === null) return;
    untrack(() => void core.loadPackages());
  });

  async function uninstall() {
    if (!confirming) return;
    removing = true;
    error = null;
    const r = await core.uninstallPackage(confirming.id);
    removing = false;
    confirming = null;
    if (!r.ok) error = r.error;
  }
</script>

<section id="{winId}-connectors" aria-labelledby="{winId}-connectors-h">
  <h3 id="{winId}-connectors-h">{t('settings.connectors.title')}</h3>
  <p class="intro">{t('settings.connectors.intro')}</p>
  {#if error}<ErrorNotice {error} />{/if}

  {#if packages === null}
    <p class="desc">{t('settings.connectors.loading')}</p>
  {:else if packages.length === 0}
    <p class="desc">{t('settings.connectors.empty')}</p>
  {:else}
    <ul class="packages" aria-label={t('settings.connectors.title')}>
      {#each packages as p (p.id)}
        <li class="row">
          <div class="text">
            <p class="name">
              {p.name}
              {#if p.publisher}<span class="by"
                  >{t('settings.connectors.publisher', { publisher: p.publisher })}</span
                >{/if}
            </p>
            <p class="desc">
              <RichText
                text={t('settings.connectors.meta', {
                  kind: kindWords(p.kind),
                  version: p.version,
                  source: p.source,
                })}
              />
            </p>
            {#if !p.checked}<p class="desc warn">{t('settings.connectors.notChecked')}</p>{/if}
            {#if p.usedBy.length > 0}
              <p class="desc">
                {t('settings.connectors.usedBy', {
                  count: p.usedBy.length,
                  parts: partList(p.usedBy),
                })}
              </p>
            {/if}
            {#if p.problem}<ErrorNotice error={p.problem} />{/if}
          </div>
          <button
            class="btn danger"
            aria-label={t('settings.connectors.uninstall.named', { name: p.name })}
            disabled={p.usedBy.length > 0 || core.auth.lock('admin') !== null}
            title={core.auth.lock('admin') ?? undefined}
            onclick={() => (confirming = p)}>{t('settings.connectors.uninstall')}</button
          >
        </li>
      {/each}
    </ul>
  {/if}

  {#if core.connectorProblems.length > 0}
    <h4 class="sub">{t('settings.connectors.problems.title')}</h4>
    <p class="desc"><RichText text={t('settings.connectors.problems.intro')} /></p>
    <ul class="problems">
      {#each core.connectorProblems as p (p.folder)}
        <li><ErrorNotice tone="warn" error={{ ...p, code: `connectors/${p.folder}` }} /></li>
      {/each}
    </ul>
  {/if}
</section>

{#if confirming}
  <Dialog
    title={t('settings.connectors.uninstall.title', { name: confirming.name })}
    tone="danger"
    oncancel={() => (confirming = null)}
  >
    <p>{t('settings.connectors.uninstall.body')}</p>
    {#snippet actions()}
      <button class="btn" onclick={() => (confirming = null)}
        >{t('settings.connectors.uninstall.cancel')}</button
      >
      <button class="btn danger solid" disabled={removing} onclick={uninstall}
        >{t('settings.connectors.uninstall.confirm')}</button
      >
    {/snippet}
  </Dialog>
{/if}

<style>
  .packages,
  .problems {
    display: grid;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .by {
    margin-left: var(--space-2);
    font-weight: 400;
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .warn {
    color: var(--warn);
  }
  .sub {
    font-size: var(--text-md);
    font-weight: 700;
  }
</style>
