<script lang="ts">
  import { useCore } from '../lib/state/context';
  import type { AppInfo, AppPackagePreview, ErrorBody } from '../lib/state/types';
  import { allFilled, initialPicks, rolesRequest, type RolePicks } from '../lib/state/appRoles';
  import { useDesktop } from '../components/desktopController.svelte';
  import { openAppWindow } from '../components/appWindow';
  import AppBadge from '../components/AppBadge.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import Markdown from '../components/Markdown.svelte';
  import RolePicker from '../components/RolePicker.svelte';
  import type { WindowProps } from '../components/windowDefs';
  import { t } from '../lib/i18n';

  /**
   * Installing an app (docs/apps.md §1): what it is, what it needs and what it may drive, with a
   * part picked for each role, what it publishes, and the "not checked" warning, all before
   * anything is installed. Install, then the parts are saved (`PUT …/roles`), then, if asked, it
   * is turned on.
   */
  let {
    win,
    preview,
    file,
    name,
  }: WindowProps & { preview: AppPackagePreview; file: Blob; name: string } = $props();

  const core = useCore();
  const desktop = useDesktop();

  const app = $derived(preview.app);
  const parts = $derived(core.parts ?? []);
  // Where the pickers start (the only part that fits a role is suggested); the user changes them.
  // svelte-ignore state_referenced_locally
  let picks = $state<RolePicks>(initialPicks(preview.app.roles, core.parts ?? []));
  let turnOn = $state(false);
  let phase = $state<'ready' | 'installing' | 'done'>('ready');
  let error = $state.raw<ErrorBody | null>(null);
  let installed = $state.raw<AppInfo | null>(null);

  const filled = $derived(allFilled(app.roles, picks));
  const replaces = $derived(
    core.packages === null ? preview.replaces : core.packages.find((p) => p.id === app.id)?.version,
  );

  // The parts may arrive after the preview: suggest from them once, never over a choice.
  // svelte-ignore state_referenced_locally
  let seeded = core.parts !== null;
  $effect(() => {
    if (seeded || core.parts === null) return;
    seeded = true;
    picks = initialPicks(app.roles, core.parts);
  });

  $effect(() => {
    if (!filled) turnOn = false;
  });

  async function install() {
    phase = 'installing';
    error = null;
    const r = await core.installPackage(file, name);
    if (!r.ok) {
      error = r.error;
      phase = 'ready';
      return;
    }
    phase = 'done';
    const roles = rolesRequest(app.roles, picks);
    if (Object.keys(roles).length > 0) {
      const set = await core.setAppRoles(app.id, roles);
      if (!set.ok) {
        error = set.error;
        return;
      }
      installed = set.value;
    }
    if (turnOn) {
      const on = await core.setAppEnabled(app.id, true);
      if (!on.ok) error = on.error;
      else installed = on.value;
    }
  }

  function openWindow() {
    openAppWindow(desktop, installed ?? app);
    desktop.close(win.id);
  }

  function openApps() {
    desktop.open('apps');
    desktop.close(win.id);
  }

  function runs(a: AppInfo): string {
    if (!a.logic) return t('install.app.runs.uiOnly');
    return t('install.app.runs.logic', { hz: a.tickHz ?? 10 });
  }
</script>

{#if phase === 'done'}
  <div class="done" role="status">
    <Icon name="check" size={28} />
    <div>
      <h3>{t('install.done.title', { name: app.name })}</h3>
      <p>
        {turnOn && !error
          ? t('install.app.done.on')
          : filled
            ? t('install.app.done.off')
            : t('install.app.done.needsParts')}
      </p>
    </div>
  </div>
  {#if error}<ErrorNotice {error} />{/if}
  <div class="actions">
    <button class="btn" onclick={() => desktop.close(win.id)}>{t('install.done.close')}</button>
    <button class="btn" onclick={openApps}>{t('install.app.done.apps')}</button>
    {#if app.ui}
      <button class="btn primary" onclick={openWindow}>{t('install.app.done.open')}</button>
    {/if}
  </div>
{:else}
  {#if error}<ErrorNotice {error} />{/if}
  <header class="head">
    <span class="badge" aria-hidden="true"><Icon name="grid" size={26} /></span>
    <div>
      <h3 class="title">{app.name}</h3>
      <p class="id"><code>{app.id}</code> · {t('install.app.kind')}</p>
    </div>
  </header>

  <dl class="facts">
    <div>
      <dt>{t('install.facts.version')}</dt>
      <dd class="num">{app.version}</dd>
    </div>
    <div>
      <dt>{t('install.facts.publisher')}</dt>
      <dd>{app.publisher ?? t('install.facts.publisherUnknown')}</dd>
    </div>
    <div>
      <dt>{t('install.app.facts.runs')}</dt>
      <dd>{runs(app)}</dd>
    </div>
    <div>
      <dt>{t('install.app.facts.window')}</dt>
      <dd>{app.ui ? t('install.app.window.yes') : t('install.app.window.no')}</dd>
    </div>
  </dl>

  {#if replaces}
    <p class="note">{t('install.app.replaces', { version: replaces })}</p>
  {/if}

  <section aria-labelledby="{win.id}-needs">
    <h4 id="{win.id}-needs">{t('install.app.needs.title')}</h4>
    {#if app.roles.length === 0}
      <p class="muted">{t('install.app.needs.none')}</p>
    {:else}
      <p class="muted intro">{t('install.app.needs.intro', { app: app.name })}</p>
      <RolePicker app={app.name} roles={app.roles} {parts} bind:picks idPrefix={win.id} />
    {/if}
    <p class="muted">{t('install.app.nothingElse')}</p>
  </section>

  {#if app.publishes.length > 0}
    <section aria-labelledby="{win.id}-publishes">
      <h4 id="{win.id}-publishes">{t('install.app.publishes.title')}</h4>
      <ul class="publishes">
        {#each app.publishes as s (s.address)}
          <li>
            <code>{s.address}</code>
            <AppBadge app={app.name} />
            <span class="muted">{s.capability}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  <div class="warning" role="note">
    <Icon name="warning" />
    <div>
      <p class="strong">{t('install.notChecked.title')}</p>
      <p>{t('install.app.notChecked.body')}</p>
    </div>
  </div>

  <section class="readme" aria-labelledby="{win.id}-readme">
    <h4 id="{win.id}-readme">{t('install.app.readme')}</h4>
    {#if preview.readme.trim()}
      <Markdown text={preview.readme} headingLevel={5} />
    {:else}
      <p class="muted">{t('install.readme.empty')}</p>
    {/if}
  </section>

  <label class="turn-on">
    <input type="checkbox" bind:checked={turnOn} disabled={!filled} />
    <span>{t('install.app.turnOn')}</span>
  </label>
  {#if !filled && app.roles.length > 0}
    <p class="muted small">{t('install.app.turnOn.needsParts')}</p>
  {/if}

  <div class="actions">
    <button class="btn" onclick={() => desktop.close(win.id)}>{t('install.cancel')}</button>
    <button class="btn primary" disabled={phase === 'installing'} onclick={install}>
      {phase === 'installing'
        ? t('install.installing')
        : replaces
          ? t('install.update')
          : t('install.install')}
    </button>
  </div>
{/if}

<style>
  .muted {
    color: var(--text-muted);
  }
  .small,
  .intro {
    font-size: var(--text-sm);
  }
  .intro {
    margin-bottom: var(--space-2);
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
  section {
    display: grid;
    gap: var(--space-2);
  }
  .publishes {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding-left: 1.2em;
  }
  .publishes li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
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
  .turn-on {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 600;
  }
  .done {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    color: var(--ok);
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
