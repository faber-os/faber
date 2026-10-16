<script lang="ts">
  import RoleLock from '../components/RoleLock.svelte';
  import { untrack } from 'svelte';
  import { useCore } from '../lib/state/context';
  import type { AppInfo, ErrorBody } from '../lib/state/types';
  import type { Result } from '../lib/state/result';
  import { initialPicks, picksChanged, rolesRequest, type RolePicks } from '../lib/state/appRoles';
  import { useDesktop } from '../components/desktopController.svelte';
  import { openAppWindow } from '../components/appWindow';
  import { hasViewer, openDerivedViewer } from '../components/driveWindow';
  import AppBadge from '../components/AppBadge.svelte';
  import Dialog from '../components/Dialog.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import RolePicker from '../components/RolePicker.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';
  import { t } from '../lib/i18n';

  /**
   * Apps (docs/apps.md): every installed app with its state in words, the parts that fill its
   * roles (changed here), on and off, its window, its derived signals, and Uninstall, which is
   * refused while it is on so nothing it drives stops by surprise. A tool of its own rather than a
   * Settings section: apps are what a beginner installs first (principles.md, step 0), and this is
   * work with live state (like Parts), not a preference.
   */
  let { win }: WindowProps = $props();

  const core = useCore();
  // Turning apps on and off, their parts and removing them: an admin's (docs/auth.md).
  const lock = $derived(core.auth.lock('admin'));
  const desktop = useDesktop();

  let error = $state.raw<{ app: string; error: ErrorBody } | null>(null);
  let busy = $state<string | null>(null);
  let confirming = $state.raw<AppInfo | null>(null);
  /** Role picks being edited, by app; absent: the app's own parts. */
  let editing = $state<Record<string, RolePicks>>({});

  const apps = $derived(core.apps);
  const parts = $derived(core.parts ?? []);
  /** Installed app packages the core couldn't load, with why. */
  const broken = $derived(
    (core.packages ?? []).filter(
      (p) => p.kind === 'app' && p.problem && !apps?.some((a) => a.id === p.id),
    ),
  );

  $effect(() => untrack(() => void core.loadPackages()));

  function picksOf(app: AppInfo): RolePicks {
    return editing[app.id] ?? initialPicks(app.roles, parts);
  }

  async function run(app: AppInfo, action: () => Promise<Result<unknown>>) {
    busy = app.id;
    error = null;
    const r = await action();
    busy = null;
    if (!r.ok) error = { app: app.id, error: r.error };
    return r.ok;
  }

  function setOn(app: AppInfo, on: boolean) {
    void run(app, () => core.setAppEnabled(app.id, on));
  }

  async function saveRoles(app: AppInfo) {
    const picks = picksOf(app);
    const saved = await run(app, () => core.setAppRoles(app.id, rolesRequest(app.roles, picks)));
    if (saved) {
      const { [app.id]: _done, ...rest } = editing;
      editing = rest;
    }
  }

  async function uninstall() {
    const app = confirming;
    confirming = null;
    if (app) await run(app, () => core.uninstallPackage(app.id));
  }
</script>

<div class="apps" data-testid="apps-window">
  {#if apps === null}
    <p class="muted">{t('apps.loading')}</p>
  {:else if apps.length === 0}
    <div class="empty">
      <Icon name="grid" size={32} />
      <p>{t('apps.empty')}</p>
    </div>
  {:else}
    <ul class="list" aria-label={t('apps.title')}>
      {#each apps as app (app.id)}
        {@const picks = picksOf(app)}
        <li class="app" data-app={app.id}>
          <header class="head">
            <span class="icon" aria-hidden="true">
              {#if app.icon}
                <img src={app.icon} alt="" width="32" height="32" referrerpolicy="no-referrer" />
              {:else}
                <Icon name="grid" size={24} />
              {/if}
            </span>
            <div class="name">
              <h3>{app.name}</h3>
              <p class="muted small">
                {t('apps.meta', { version: app.version })}{#if app.publisher}
                  · {t('settings.connectors.publisher', { publisher: app.publisher })}{/if}
              </p>
            </div>
            <StateBadge kind={app.state.kind} />
          </header>

          {#if app.state.message}
            <p class="state" class:problem={app.enabled && app.state.kind !== 'running'}>
              {app.state.message}
              {#if app.state.hint}<span class="muted">{app.state.hint}</span>{/if}
            </p>
          {/if}

          {#if error?.app === app.id}<ErrorNotice error={error.error} />{/if}

          <div class="controls">
            <label class="toggle">
              <input
                class="switch"
                type="checkbox"
                role="switch"
                checked={app.enabled}
                disabled={busy === app.id || lock !== null}
                title={lock ?? undefined}
                aria-label={t('apps.onNamed', { name: app.name })}
                onchange={(e) => {
                  const on = e.currentTarget.checked;
                  // The switch shows what the core says: it flips when the apps topic does.
                  e.currentTarget.checked = app.enabled;
                  setOn(app, on);
                }}
              />
              <span>{app.enabled ? t('apps.on') : t('apps.off')}</span>
            </label>
            {#if app.ui}
              <button
                class="btn"
                aria-label={t('apps.openNamed', { name: app.name })}
                onclick={() => openAppWindow(desktop, app)}
                ><Icon name="open" size={16} />{t('apps.open')}</button
              >
            {/if}
            <span class="spacer"></span>
            <button
              class="btn danger"
              disabled={app.enabled || busy === app.id || lock !== null}
              title={lock ?? undefined}
              aria-label={t('apps.uninstallNamed', { name: app.name })}
              onclick={() => (confirming = app)}>{t('apps.uninstall')}</button
            >
          </div>
          {#if app.enabled}
            <p class="muted small">{t('apps.uninstall.whileOn')}</p>
          {/if}

          {#if app.roles.length > 0}
            <section aria-label={t('apps.roles.named', { name: app.name })}>
              <h4>{t('apps.roles')}</h4>
              <RolePicker
                app={app.name}
                roles={app.roles}
                {parts}
                bind:picks={() => picks, (v) => (editing = { ...editing, [app.id]: v })}
                idPrefix="{win.id}-{app.id}"
              />
              {#if picksChanged(app.roles, picks)}
                <div class="save">
                  <button
                    class="btn primary"
                    disabled={busy === app.id || lock !== null}
                    title={lock ?? undefined}
                    onclick={() => void saveRoles(app)}>{t('apps.roles.save')}</button
                  >
                  <RoleLock reason={lock} />
                  {#if app.enabled}<span class="muted small">{t('apps.roles.restarts')}</span>{/if}
                </div>
              {/if}
            </section>
          {/if}

          {#if app.publishes.length > 0}
            <section aria-label={t('apps.publishes.named', { name: app.name })}>
              <h4>{t('apps.publishes')}</h4>
              <ul class="signals">
                {#each app.publishes as s (s.address)}
                  <li>
                    <code>{s.address}</code>
                    <AppBadge app={app.name} />
                    <span class="muted small">{s.capability}</span>
                    {#if hasViewer(s.capability)}
                      <button
                        class="btn quiet"
                        aria-label={t('parts.list.viewFor', { address: s.address })}
                        onclick={() => openDerivedViewer(desktop, app.name, s)}
                        >{t('parts.list.view')}</button
                      >
                    {/if}
                  </li>
                {/each}
              </ul>
            </section>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if broken.length > 0}
    <h4 class="sub">{t('apps.problems.title')}</h4>
    <ul class="problems">
      {#each broken as p (p.id)}
        <li><ErrorNotice tone="warn" error={{ ...p.problem!, code: `apps/${p.id}` }} /></li>
      {/each}
    </ul>
  {/if}
</div>

{#if confirming}
  <Dialog
    title={t('apps.uninstall.title', { name: confirming.name })}
    tone="danger"
    oncancel={() => (confirming = null)}
  >
    <p>{t('apps.uninstall.body')}</p>
    {#snippet actions()}
      <button class="btn" onclick={() => (confirming = null)}
        >{t('settings.connectors.uninstall.cancel')}</button
      >
      <button class="btn danger solid" onclick={() => void uninstall()}
        >{t('apps.uninstall')}</button
      >
    {/snippet}
  </Dialog>
{/if}

<style>
  /* The scrolling area, and the containing block of what is positioned inside it (visually
     hidden text, such as the role picker's legends): otherwise that hangs below the window's body
     once the list is longer than the window, instead of scrolling with it. */
  .apps {
    position: relative;
    display: grid;
    align-content: start;
    gap: var(--space-4);
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-4);
  }
  .muted {
    color: var(--text-muted);
  }
  .small {
    font-size: var(--text-sm);
  }
  .empty {
    display: grid;
    justify-items: center;
    gap: var(--space-3);
    padding: var(--space-6) var(--space-4);
    color: var(--text-muted);
    text-align: center;
  }
  .list,
  .problems,
  .signals {
    display: grid;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .app {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-4);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--surface-2, var(--surface));
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    background: var(--plate);
    color: var(--app-parts);
  }
  .name {
    flex: 1;
    min-width: 0;
  }
  h3 {
    font-size: var(--text-md);
    font-weight: 700;
  }
  h4 {
    font-size: var(--text-sm);
    font-weight: 700;
    margin-bottom: var(--space-2);
  }
  .state {
    font-size: var(--text-sm);
  }
  .state.problem {
    color: var(--danger);
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
  .spacer {
    flex: 1;
  }
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 600;
  }
  .save {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-top: var(--space-2);
  }
  .signals li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
  .sub {
    font-size: var(--text-md);
  }
</style>
