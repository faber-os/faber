<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { t, type MessageKey } from '../lib/i18n';
  import { item } from '../lib/desktop/contextMenu';
  import {
    powerActions,
    recentWhen,
    searchLauncher,
    type LaunchPart,
    type LaunchProgram,
    type RecentFile,
  } from '../lib/desktop/launcher';
  import { baseName, parentPath } from '../lib/desktop/filePaths';
  import { fileType } from '../lib/desktop/fileTypes';
  import AppIcon from './AppIcon.svelte';
  import { useDesktop } from './desktopController.svelte';
  import FileIcon from './FileIcon.svelte';
  import { openEntry } from './fileOps';
  import Icon from './Icon.svelte';
  import { menuTrigger, useMenu, type MenuRequest } from './menuController.svelte';
  import { appProgram, pinItem, runPinItem } from './programs';
  import { openAppWindow } from './appWindow';
  import type { AppInfo } from '../lib/state/types';
  import type { WindowDef } from './windowDefs';

  /**
   * The start menu: a search field, every program as a grid of icons, the installed apps that have
   * a window, part viewers, and the files opened lately; the power button at the bottom. Typing
   * searches programs, apps, parts and recent files.
   */
  let { defs, onclose }: { defs: WindowDef[]; onclose: (returnFocus: boolean) => void } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const menu = useMenu();
  let panel: HTMLElement | undefined = $state();
  let search: HTMLInputElement | undefined = $state();
  let query = $state('');

  const GROUP_LABEL: Record<string, Extract<MessageKey, `startMenu.group.${string}`>> = {
    parts: 'startMenu.group.tool',
    tools: 'startMenu.group.tool',
    accessories: 'startMenu.group.accessory',
    faber: 'startMenu.group.faber',
  };

  const launchable = $derived(defs.filter((d) => d.menu !== null));
  const programs = $derived<(LaunchProgram & { def: WindowDef })[]>(
    launchable.map((d) => ({
      key: d.program ?? d.kind,
      kind: d.kind,
      title: d.title,
      description: d.description,
      group: d.menu && GROUP_LABEL[d.menu] ? t(GROUP_LABEL[d.menu]!) : '',
      def: d,
    })),
  );
  const parts = $derived<LaunchPart[]>(
    (core.parts ?? []).map((p) => {
      const scan = p.signals.find((s) => s.capability === 'laser-scan@1');
      return scan ? { name: p.name, viewer: scan.address } : { name: p.name };
    }),
  );
  const viewers = $derived(parts.filter((p) => p.viewer));
  /** Installed apps with a window, by their window's title (or name). */
  const apps = $derived((core.apps ?? []).filter((a) => a.ui));
  const appTitle = (a: AppInfo) => a.window?.title ?? a.name;
  const foundApps = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? apps.filter((a) => appTitle(a).toLowerCase().includes(q)) : [];
  });
  const results = $derived(searchLauncher(query, { programs, parts, files: desktop.recent }));
  const searching = $derived(query.trim() !== '');
  const host = typeof location === 'undefined' ? '' : location.host;

  $effect(() => {
    search?.focus();
  });

  function items(): HTMLElement[] {
    return [...(panel?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
  }

  function onkeydown(e: KeyboardEvent) {
    const all = items();
    const i = all.indexOf(document.activeElement as HTMLElement);
    const inSearch = document.activeElement === search;
    if (e.key === 'Escape') {
      e.preventDefault();
      if (inSearch && query) query = '';
      else onclose(true);
    } else if (e.key === 'ArrowDown' || (e.key === 'ArrowRight' && !inSearch)) {
      e.preventDefault();
      all[inSearch ? 0 : (i + 1) % all.length]?.focus();
    } else if (e.key === 'ArrowUp' || (e.key === 'ArrowLeft' && !inSearch)) {
      e.preventDefault();
      if (i <= 0) search?.focus();
      else all[i - 1]?.focus();
    } else if (e.key === 'Home' && !inSearch) {
      e.preventDefault();
      all[0]?.focus();
    } else if (e.key === 'End' && !inSearch) {
      e.preventDefault();
      all.at(-1)?.focus();
    } else if (e.key === 'Enter' && inSearch) {
      e.preventDefault();
      all[0]?.click();
    } else if (e.key === 'Tab') {
      onclose(false);
    } else if (!inSearch && e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      // Typing anywhere in the menu searches.
      search?.focus();
    }
  }

  function open(kind: string) {
    desktop.open(kind);
    onclose(false);
  }

  function openApp(a: AppInfo) {
    openAppWindow(desktop, a);
    onclose(false);
  }

  function openPart(p: LaunchPart) {
    if (p.viewer) {
      desktop.open('laser-scan', {
        key: p.viewer,
        title: t('startMenu.scanWindowTitle', { part: p.name }),
        params: { address: p.viewer, part: p.name },
      });
    } else desktop.open('parts');
    onclose(false);
  }

  function openRecent(f: RecentFile) {
    onclose(false);
    openEntry(desktop, { kind: 'file', name: f.name, path: f.path, size: 0 });
  }

  function tileMenu(target: HTMLElement): MenuRequest | null {
    const appId = target.closest<HTMLElement>('[data-app]')?.dataset.app;
    const a = apps.find((x) => x.id === appId);
    if (a) {
      const key = appProgram(a.id);
      return {
        label: appTitle(a),
        header: { title: appTitle(a), icon: 'grid' },
        entries: [item('open', t('menu.open'), { icon: 'open' }), pinItem(core, key)],
        onSelect: (id) => {
          if (runPinItem(core, key, id)) return;
          if (id === 'open') openApp(a);
        },
      };
    }
    const kind = target.closest<HTMLElement>('[data-kind]')?.dataset.kind;
    const d = defs.find((x) => x.kind === kind);
    if (!d) return null;
    return {
      label: d.title,
      header: { title: d.title, detail: d.description, icon: d.icon },
      entries: [
        item('open', t('menu.open'), { icon: 'open' }),
        !!d.program && pinItem(core, d.program),
      ],
      onSelect: (id) => {
        if (d.program && runPinItem(core, d.program, id)) return;
        if (id === 'open') open(d.kind);
      },
    };
  }

  function powerMenu(e: MouseEvent) {
    const button = e.currentTarget as HTMLElement;
    const r = button.getBoundingClientRect();
    menu.show(
      {
        label: t('startMenu.power.menu'),
        entries: powerActions(false).map((a) =>
          item(a.id, a.label, {
            icon: a.id === 'restart' ? 'restart' : 'power',
            disabledReason: a.disabledReason,
          }),
        ),
        // Nothing to run yet: both are disabled until the core can do it.
        onSelect: () => {},
      },
      { x: r.left, y: r.top - 4 },
      button,
      e.detail === 0,
    );
  }
</script>

<div
  class="panel"
  id="start-menu"
  role="dialog"
  aria-label={t('startMenu.label')}
  tabindex="-1"
  bind:this={panel}
  {onkeydown}
>
  <label class="search">
    <Icon name="search" size={17} />
    <input
      bind:this={search}
      bind:value={query}
      type="search"
      placeholder={t('startMenu.search.placeholder')}
      aria-label={t('startMenu.search.label')}
      spellcheck="false"
      autocomplete="off"
    />
  </label>

  <div
    class="body"
    role="menu"
    aria-label={t('startMenu.label')}
    use:menuTrigger={{ menu, build: tileMenu }}
  >
    {#if searching}
      {#if results.programs.length + foundApps.length + results.parts.length + results.files.length === 0}
        <p class="nothing">{t('startMenu.search.nothing', { query: query.trim() })}</p>
      {/if}
      {#if results.programs.length}
        <div class="group" role="group" aria-label={t('startMenu.programs')}>
          <p class="heading" aria-hidden="true">{t('startMenu.programs')}</p>
          {#each results.programs as p (p.kind)}
            {@const d = defs.find((x) => x.kind === p.kind)!}
            <button class="row" role="menuitem" data-kind={d.kind} onclick={() => open(d.kind)}>
              <AppIcon icon={d.icon} colour={d.colour} size={32} />
              <span class="text"
                ><span class="name">{d.title}</span><span class="desc">{d.description}</span></span
              >
              <span class="tag">{p.group}</span>
            </button>
          {/each}
        </div>
      {/if}
      {#if foundApps.length}
        <div class="group" role="group" aria-label={t('startMenu.apps')}>
          <p class="heading" aria-hidden="true">{t('startMenu.apps')}</p>
          {#each foundApps as a (a.id)}
            <button class="row" role="menuitem" data-app={a.id} onclick={() => openApp(a)}>
              <AppIcon icon="grid" colour="parts" size={32} src={a.icon ?? null} />
              <span class="text"
                ><span class="name">{appTitle(a)}</span><span class="desc"
                  >{t('startMenu.app.desc', { version: a.version })}</span
                ></span
              >
              <span class="tag">{t('startMenu.group.app')}</span>
            </button>
          {/each}
        </div>
      {/if}
      {#if results.parts.length}
        <div class="group" role="group" aria-label={t('startMenu.parts')}>
          <p class="heading" aria-hidden="true">{t('startMenu.parts')}</p>
          {#each results.parts as p (p.name)}
            <button class="row" role="menuitem" onclick={() => openPart(p)}>
              <AppIcon
                icon={p.viewer ? 'scan' : 'parts'}
                colour={p.viewer ? 'viewer' : 'parts'}
                size={32}
              />
              <span class="text"
                ><span class="name">{p.name}</span><span class="desc mono"
                  >{p.viewer ?? t('startMenu.openInParts')}</span
                ></span
              >
            </button>
          {/each}
        </div>
      {/if}
      {#if results.files.length}
        <div class="group" role="group" aria-label={t('startMenu.recent')}>
          <p class="heading" aria-hidden="true">{t('startMenu.recent')}</p>
          {#each results.files as f (f.path)}
            <button class="row" role="menuitem" onclick={() => openRecent(f)}>
              <FileIcon type={fileType({ kind: 'file', name: f.name })} size={30} />
              <span class="text"
                ><span class="name">{f.name}</span><span class="desc"
                  >{baseName(parentPath(f.path) ?? '') || t('startMenu.recent.home')}</span
                ></span
              >
            </button>
          {/each}
        </div>
      {/if}
    {:else}
      <div class="group" role="group" aria-label={t('startMenu.programs')}>
        <p class="heading" aria-hidden="true">{t('startMenu.programs')}</p>
        <div class="grid">
          {#each programs as p (p.kind)}
            <button
              class="tile"
              role="menuitem"
              data-kind={p.kind}
              title={p.description}
              onclick={() => open(p.kind)}
            >
              <AppIcon icon={p.def.icon} colour={p.def.colour} size={44} />
              <span class="tile-name">{p.title}</span>
            </button>
          {/each}
        </div>
      </div>
      {#if apps.length}
        <div class="group" role="group" aria-label={t('startMenu.apps')}>
          <p class="heading" aria-hidden="true">{t('startMenu.apps')}</p>
          <div class="grid">
            {#each apps as a (a.id)}
              <button
                class="tile"
                role="menuitem"
                data-app={a.id}
                title={appTitle(a)}
                onclick={() => openApp(a)}
              >
                <AppIcon icon="grid" colour="parts" size={44} src={a.icon ?? null} />
                <span class="tile-name">{appTitle(a)}</span>
              </button>
            {/each}
          </div>
        </div>
      {/if}
      {#if viewers.length}
        <div class="group" role="group" aria-label={t('startMenu.viewers')}>
          <p class="heading" aria-hidden="true">{t('startMenu.viewers')}</p>
          {#each viewers as v (v.viewer)}
            <button class="row compact" role="menuitem" onclick={() => openPart(v)}>
              <AppIcon icon="scan" colour="viewer" size={28} />
              <span class="text"
                ><span class="name">{v.name}</span><span class="desc mono">{v.viewer}</span></span
              >
            </button>
          {/each}
        </div>
      {/if}
      <div class="group" role="group" aria-label={t('startMenu.recent')}>
        <p class="heading" aria-hidden="true">{t('startMenu.recent')}</p>
        {#each desktop.recent.slice(0, 4) as f (f.path)}
          <button class="row compact" role="menuitem" onclick={() => openRecent(f)}>
            <FileIcon type={fileType({ kind: 'file', name: f.name })} size={26} />
            <span class="text"><span class="name">{f.name}</span></span>
            <span class="when">{recentWhen(f.at, Date.now(), core.wallClock)}</span>
          </button>
        {:else}
          <p class="nothing">{t('startMenu.recent.empty')}</p>
        {/each}
      </div>
    {/if}
  </div>

  <div class="foot">
    <span class="host"
      ><span class="dot" class:off={core.connection !== 'connected'} aria-hidden="true"
      ></span>{host}</span
    >
    <button
      class="power"
      aria-label={t('startMenu.power.label')}
      aria-haspopup="menu"
      onclick={powerMenu}
    >
      <Icon name="power" size={18} />
    </button>
  </div>
</div>

<style>
  .panel {
    position: absolute;
    left: 8px;
    bottom: calc(var(--taskbar-h) + 8px);
    width: min(440px, calc(100vw - 16px));
    max-height: calc(100vh - var(--taskbar-h) - 16px);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-3);
    border-radius: var(--radius-lg);
    background: var(--menu);
    box-shadow: var(--shadow-pop), var(--highlight);
    animation: rise var(--dur-med) var(--ease-out);
    z-index: 100000;
    outline: none;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--touch);
    padding: 0 var(--space-3);
    border: 1px solid var(--line-strong);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-muted);
  }
  .search:focus-within {
    border-color: var(--focus-ring);
    box-shadow: 0 0 0 1px var(--focus-ring);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
  }
  .search input::-webkit-search-cancel-button {
    display: none;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: var(--space-3);
    margin: 0 calc(var(--space-2) * -1);
    padding: 0 var(--space-2);
  }
  .heading {
    padding: var(--space-1) var(--space-1) var(--space-2);
    font-size: var(--text-sm);
    font-weight: 700;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-1);
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: var(--space-2) var(--space-1);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
  }
  .tile-name {
    font-size: var(--text-xs);
    font-weight: 600;
    text-align: center;
    line-height: 1.25;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    min-height: 48px;
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    text-align: left;
  }
  .row.compact {
    min-height: var(--touch);
  }
  .tile:hover,
  .row:hover,
  .tile:focus-visible,
  .row:focus-visible {
    background: var(--hover);
  }
  .tile:focus-visible,
  .row:focus-visible {
    outline-offset: -2px;
  }
  .text {
    flex: 1;
    display: grid;
    min-width: 0;
  }
  .name {
    font-weight: 620;
    font-size: var(--text-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .desc {
    font-size: var(--text-xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag,
  .when {
    flex: none;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .nothing {
    padding: var(--space-1) var(--space-2);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-top: var(--space-3);
    border-top: 1px solid var(--line);
  }
  .host {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-family: var(--font-data);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
  }
  .dot.off {
    background: var(--warn);
  }
  .power {
    display: grid;
    place-items: center;
    width: var(--touch);
    height: var(--touch);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-muted);
  }
  .power:hover {
    background: var(--hover);
    color: var(--text);
  }
</style>
