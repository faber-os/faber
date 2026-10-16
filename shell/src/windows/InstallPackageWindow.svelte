<script lang="ts">
  import { untrack } from 'svelte';
  import { useCore } from '../lib/state/context';
  import type { AppPackagePreview, ErrorBody, PackagePreview } from '../lib/state/types';
  import { kindWords, touchWords } from '../lib/desktop/packages';
  import { useDesktop } from '../components/desktopController.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import Markdown from '../components/Markdown.svelte';
  import InstallApp from './InstallApp.svelte';
  import { droppedPackage, forgetDropped } from '../components/installWindow';
  import type { WindowProps } from '../components/windowDefs';
  import { t } from '../lib/i18n';

  /**
   * Install a package (docs/packages.md §1): what it is, who made it, what it can touch and its
   * README, checked in full by the core before anything is installed. Nothing is checked by the
   * Faber registry yet (Phase 7), and the window says so. An app's package shows its own pane
   * (InstallApp), with a part picked for each of its roles.
   */
  let { win, visible }: WindowProps = $props();

  const core = useCore();
  const desktop = useDesktop();

  const name = $derived(win.params.name ?? '');
  let file = $state.raw<Blob | null>(null);
  let preview = $state.raw<PackagePreview | null>(null);
  let appPreview = $state.raw<AppPackagePreview | null>(null);
  let error = $state.raw<ErrorBody | null>(null);
  let phase = $state<'loading' | 'ready' | 'installing' | 'done' | 'missing'>('loading');
  /**
   * The version installed now. The packages topic keeps it current, also when another tab
   * installs or uninstalls this connector while the window is open.
   */
  const replaces = $derived(
    preview === null
      ? undefined
      : core.packages === null
        ? preview.replaces
        : core.packages.find((p) => p.id === preview?.connector.id)?.version,
  );

  $effect(() => {
    untrack(() => void load());
    const token = win.params.dropped;
    return () => {
      if (token) forgetDropped(token);
    };
  });

  async function load() {
    phase = 'loading';
    error = null;
    preview = null;
    appPreview = null;
    let blob: Blob | undefined;
    if (win.params.path) {
      const r = await core.downloadFile(win.params.path);
      if (!r.ok) {
        error = r.error;
        phase = 'ready';
        return;
      }
      blob = r.value;
    } else if (win.params.dropped) blob = droppedPackage(win.params.dropped);
    if (!blob) {
      phase = 'missing';
      return;
    }
    file = blob;
    const p = await core.previewPackage(blob, name);
    if (!p.ok) error = p.error;
    else if ('app' in p.value) appPreview = p.value;
    else preview = p.value;
    phase = 'ready';
  }

  async function install() {
    if (!file) return;
    phase = 'installing';
    error = null;
    const r = await core.installPackage(file, name);
    if (r.ok) phase = 'done';
    else {
      error = r.error;
      phase = 'ready';
    }
  }

  function addPart() {
    if (!preview) return;
    const id = desktop.open('parts');
    desktop.update(id, { params: { addConnector: preview.connector.id } });
    desktop.close(win.id);
  }
</script>

<div class="install" data-testid="install-package">
  {#if phase === 'loading'}
    <p class="muted" role="status">{t('install.loading')}</p>
  {:else if phase === 'missing'}
    <p class="muted">{t('install.noFile')}</p>
  {:else if appPreview && file}
    <InstallApp {win} {visible} preview={appPreview} {file} {name} />
  {:else if phase === 'done' && preview}
    <div class="done" role="status">
      <Icon name="check" size={28} />
      <div>
        <h3>{t('install.done.title', { name: preview.connector.name })}</h3>
        <p>{t('install.done.body')}</p>
      </div>
    </div>
    <div class="actions">
      <button class="btn" onclick={() => desktop.close(win.id)}>{t('install.done.close')}</button>
      <button class="btn primary" onclick={addPart}>{t('install.done.addPart')}</button>
    </div>
  {:else}
    {#if error}
      <ErrorNotice {error}>
        {#snippet actions()}
          {#if !preview}
            <button class="btn" onclick={() => void load()}>{t('install.retry')}</button>
          {/if}
        {/snippet}
      </ErrorNotice>
    {/if}
    {#if preview}
      {@const c = preview.connector}
      <header class="head">
        <span class="badge" aria-hidden="true"><Icon name="package" size={26} /></span>
        <div>
          <h3 class="title">{c.name}</h3>
          <p class="id"><code>{c.id}</code></p>
        </div>
      </header>

      <dl class="facts">
        <div>
          <dt>{t('install.facts.kind')}</dt>
          <dd>{kindWords(c.kind)}</dd>
        </div>
        <div>
          <dt>{t('install.facts.version')}</dt>
          <dd class="num">{c.version}</dd>
        </div>
        <div>
          <dt>{t('install.facts.publisher')}</dt>
          <dd>{preview.publisher ?? t('install.facts.publisherUnknown')}</dd>
        </div>
        <div>
          <dt>{t('install.facts.provides')}</dt>
          <dd>{c.provides.map((p) => p.capability).join(', ')}</dd>
        </div>
      </dl>

      {#if replaces}
        <p class="note">{t('install.replaces', { version: replaces })}</p>
      {/if}

      <section aria-labelledby="{win.id}-touch">
        <h4 id="{win.id}-touch">{t('install.touch.title')}</h4>
        <ul class="touch">
          <li>{touchWords(c.port)}</li>
          <li class="muted">{t('install.touch.nothingElse')}</li>
        </ul>
      </section>

      <div class="warning" role="note">
        <Icon name="warning" />
        <div>
          <p class="strong">{t('install.notChecked.title')}</p>
          <p>{t('install.notChecked.body')}</p>
        </div>
      </div>

      <section class="readme" aria-labelledby="{win.id}-readme">
        <h4 id="{win.id}-readme">{t('install.readme')}</h4>
        {#if preview.readme.trim()}
          <Markdown text={preview.readme} headingLevel={5} />
        {:else}
          <p class="muted">{t('install.readme.empty')}</p>
        {/if}
      </section>
    {/if}
    <div class="actions">
      <button class="btn" onclick={() => desktop.close(win.id)}>{t('install.cancel')}</button>
      {#if preview}
        <button class="btn primary" disabled={phase === 'installing'} onclick={install}>
          {phase === 'installing'
            ? t('install.installing')
            : preview.replaces
              ? t('install.update')
              : t('install.install')}
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .install {
    display: grid;
    align-content: start;
    gap: var(--space-4);
    height: 100%;
    overflow-y: auto;
    padding: var(--space-5);
    user-select: text;
  }
  .muted {
    color: var(--text-muted);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: var(--radius-md);
    background: var(--accent-soft, var(--hover));
    color: var(--accent);
  }
  .title {
    font-size: var(--text-lg);
    font-weight: 700;
  }
  .id {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
    gap: var(--space-3) var(--space-4);
    margin: 0;
  }
  dt {
    font-size: var(--text-xs);
    font-weight: 700;
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  h4 {
    font-size: var(--text-sm);
    font-weight: 700;
    margin-bottom: var(--space-1);
  }
  .touch {
    margin: 0;
    padding-left: 1.2em;
  }
  .note {
    font-size: var(--text-sm);
  }
  .warning {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--warn-soft);
  }
  .strong {
    font-weight: 700;
  }
  .readme {
    padding-top: var(--space-3);
    border-top: 1px solid var(--line);
  }
  .done {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    color: var(--ok-text);
  }
  .done h3 {
    font-size: var(--text-lg);
    font-weight: 700;
    color: var(--text);
  }
  .done p {
    color: var(--text);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
