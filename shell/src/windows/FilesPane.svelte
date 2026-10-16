<script lang="ts">
  import { tick } from 'svelte';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody, FileEntry, FileRoot } from '../lib/state/types';
  import { item, separator } from '../lib/desktop/contextMenu';
  import { diskRoots, isOnDisk, rootsChanged } from '../lib/desktop/disks';
  import {
    RECORDINGS,
    TRASH,
    breadcrumbs,
    childPath,
    parentPath,
    replayPortFor,
    splitPath,
    stem,
    uniqueName,
  } from '../lib/desktop/filePaths';
  import {
    DEFAULT_VIEW,
    VIEW_LABELS,
    ZOOM_STEPS,
    keyZoom,
    pinchZoom,
    wheelZoom,
    type FileView,
  } from '../lib/desktop/fileZoom';
  import { fileTypeLabel } from '../lib/desktop/fileTypes';
  import { isPackageFile } from '../lib/desktop/packages';
  import { openDroppedPackage } from '../components/installWindow';
  import { uploadSummary, type ConflictAnswer } from '../lib/desktop/uploads';
  import { uploadFiles } from '../components/uploadFiles';
  import UploadConflict from '../components/UploadConflict.svelte';
  import { openInArduino } from '../components/arduinoWindow';
  import Dialog from '../components/Dialog.svelte';
  import { useDesktop } from '../components/desktopController.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import FileIcon from '../components/FileIcon.svelte';
  import {
    download,
    entryHeader,
    entryMenu,
    openEntry,
    openInNotes,
    openProperties,
  } from '../components/fileOps';
  import { SPECIALS, fileLook } from '../components/fileLook';
  import { FolderView } from '../components/folderView.svelte';
  import { formatBytes } from '../components/format';
  import { formatDateTime } from '../lib/desktop/wallClock';
  import Icon, { type IconName } from '../components/Icon.svelte';
  import { menuTrigger, useMenu } from '../components/menuController.svelte';
  import RichText from '../components/RichText.svelte';
  import { t } from '../lib/i18n';

  /**
   * One folder being browsed: a tab of a Files window. Details (a list with a panel about the
   * selected file), small icons or large icons; Ctrl and the wheel, Ctrl +/−/0 or a pinch move
   * between them.
   */
  let {
    id,
    start,
    active,
    onstate,
    onopentab,
  }: {
    /** Unique per tab, for element ids. */
    id: string;
    start: string;
    active: boolean;
    /** The folder shown changed: its name for the tab, and its path. */
    onstate: (s: { title: string; key: string }) => void;
    /** Open a folder in a new tab of this window. */
    onopentab: (path: string) => void;
  } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const menu = useMenu();
  // svelte-ignore state_referenced_locally (a tab starts in one folder, then browses on its own)
  const view = new FolderView(core, start);

  let mode = $state<FileView>(DEFAULT_VIEW);
  let selected = $state<string | null>(null);
  let renaming = $state<string | null>(null);
  let renameValue = $state('');
  let actionError = $state<ErrorBody | null>(null);
  let notice = $state<string | null>(null);
  let confirm = $state<{ kind: 'delete'; entry: FileEntry } | { kind: 'empty' } | null>(null);
  let conflict = $state<{
    name: string;
    folder: string;
    remaining: number;
    answer: (a: ConflictAnswer) => void;
  } | null>(null);
  let dropping = $state(false);
  let busy = $state(false);
  let roots = $state.raw<FileRoot[]>([]);
  let picker: HTMLInputElement | undefined = $state();
  let listEl: HTMLElement | undefined = $state();
  let lastClick = { path: '', at: 0 };
  let wheelCarry = 0;
  const touches = new Map<number, { x: number; y: number }>();
  let pinchStart = 0;
  let pinchView: FileView = DEFAULT_VIEW;

  $effect(() => view.start());

  async function loadRoots() {
    const r = await core.getRoots();
    if (r.ok) roots = r.value;
  }

  // Whole-machine access shows "This computer"; ask for the roots again whenever it changes.
  const machineOn = $derived(core.settings?.developer.fullMachineFiles ?? false);
  $effect(() => {
    void machineOn;
    void loadRoots();
  });
  // A disk plugged in or pulled out: the core names its root in files-changed.
  $effect(() =>
    core.watchFiles((folders) => {
      if (rootsChanged(folders)) void loadRoots();
    }),
  );

  // The disk this tab was showing went away: back to the home folder, saying why.
  $effect(() => {
    const e = view.error;
    if (e?.code === 'not-found' && isOnDisk(view.path)) {
      notice = t('files.notice.diskRemoved');
      view.go('home:/Desktop');
    }
  });

  const entries = $derived(view.listing?.entries ?? []);
  const root = $derived(splitPath(view.path).root);
  const machine = $derived(roots.find((r) => r.id === 'machine'));
  const disks = $derived(diskRoots(roots));
  const onDisk = $derived(isOnDisk(view.path));
  const rootLabel = $derived(
    root === 'home' ? t('files.home') : (roots.find((r) => r.id === root)?.label ?? root),
  );
  const crumbs = $derived(breadcrumbs(view.path, rootLabel));
  const inTrash = $derived(view.path === TRASH);
  const inRecordings = $derived(view.path === RECORDINGS);
  const inSystem = $derived(view.path === 'home:/System');
  const current = $derived(entries.find((e) => e.path === selected) ?? null);

  // The tab (and the window's taskbar title) follows the folder.
  $effect(() => {
    onstate({ title: crumbs.at(-1)!.label, key: view.path });
  });

  // A new folder clears the selection and any message about the old one.
  let shownPath = '';
  $effect(() => {
    if (view.path === shownPath) return;
    const first = shownPath === '';
    shownPath = view.path;
    selected = null;
    renaming = null;
    if (!first && notice !== t('files.notice.diskRemoved')) notice = null;
  });

  async function act(fn: () => Promise<{ ok: true } | { ok: false; error: ErrorBody }>) {
    busy = true;
    actionError = null;
    const r = await fn();
    busy = false;
    if (!r.ok) actionError = r.error;
    await view.refresh(true);
    return r.ok;
  }

  function open(e: FileEntry) {
    if (e.kind === 'folder') view.go(e.path);
    else openEntry(desktop, e);
  }

  async function newFolder() {
    const name = uniqueName(
      entries.map((e) => e.name),
      t('files.newName.folder'),
    );
    const path = childPath(view.path, name);
    if (await act(() => core.createFolder(path))) {
      selected = path;
      startRename(path, name);
    }
  }

  async function newTextFile() {
    const name = uniqueName(
      entries.map((e) => e.name),
      t('files.newName.textFile'),
    );
    const path = childPath(view.path, name);
    if (await act(() => core.writeFile(path, new Uint8Array(), 'create'))) {
      selected = path;
      startRename(path, name);
    }
  }

  async function startRename(path: string, name: string) {
    renaming = path;
    renameValue = name;
    await tick();
    const input = listEl?.querySelector<HTMLInputElement>('input.rename');
    input?.focus();
    // Select the name without its extension, as Windows and macOS do.
    input?.setSelectionRange(0, stem(name).length);
  }

  async function commitRename() {
    const from = renaming;
    if (!from) return;
    renaming = null;
    const name = renameValue.trim();
    const parent = parentPath(from);
    if (!parent || name === '' || childPath(parent, name) === from) return focusList();
    const to = childPath(parent, name);
    if (await act(() => core.move(from, to))) {
      selected = to;
      desktop.renameRecent(from, to);
    }
    focusList();
  }

  function cancelRename() {
    renaming = null;
    focusList();
  }

  async function focusList() {
    await tick();
    const el =
      listEl?.querySelector<HTMLElement>('[aria-selected="true"]') ??
      listEl?.querySelector<HTMLElement>('[role="option"]') ??
      listEl;
    el?.focus();
  }

  function askConflict(name: string, remaining: number): Promise<ConflictAnswer> {
    return new Promise((resolve) => {
      conflict = {
        name,
        folder: crumbs.at(-1)?.label ?? '',
        remaining,
        answer: (a) => {
          conflict = null;
          resolve(a);
        },
      };
    });
  }

  /** Upload into this folder. A name that is taken asks first: Replace, Keep both or Skip. */
  async function upload(files: FileList | File[] | null | undefined) {
    if (!files || files.length === 0) return;
    const items = Array.from(files).map((f) => ({ path: f.name, file: f }));
    busy = true;
    actionError = null;
    notice = null;
    const r = await uploadFiles(
      core,
      view.path,
      items,
      entries.map((e) => e.name),
      askConflict,
    );
    busy = false;
    actionError = r.error;
    notice = uploadSummary(r);
    await view.refresh(true);
    if (r.lastPath) selected = r.lastPath;
  }

  async function addSamples() {
    busy = true;
    actionError = null;
    const r = await core.addSamples();
    busy = false;
    // Already there (another browser added it) is fine: the listing shows it.
    if (!r.ok && r.error.code !== 'already-exists') actionError = r.error;
    await view.refresh(true);
  }

  function addPartOn(replay: string) {
    const parts = desktop.open('parts');
    desktop.update(parts, { params: { addPort: replay } });
  }

  function runItem(e: FileEntry, id: string) {
    selected = e.path;
    switch (id) {
      case 'open':
        return open(e);
      case 'open-tab':
        return onopentab(e.path);
      case 'notes':
        return openInNotes(desktop, e.path);
      case 'arduino':
        return openInArduino(desktop, e.path);
      case 'rename':
        return startRename(e.path, e.name);
      case 'trash':
        return act(() => core.trash([e.path]));
      case 'restore':
        return act(() => core.restore([e.path]));
      case 'delete':
        confirm = { kind: 'delete', entry: e };
        return;
      case 'download':
        return download(core, e.path);
      case 'properties':
        return openProperties(desktop, e);
    }
  }

  function backgroundMenu() {
    if (inTrash) {
      return [
        item('refresh', t('files.menu.refresh'), { icon: 'refresh' }),
        separator(),
        item('empty', t('files.menu.emptyTrash'), {
          danger: true,
          icon: 'trash',
          disabledReason: entries.length === 0 ? t('files.menu.trashAlreadyEmpty') : undefined,
        }),
      ];
    }
    return [
      item('new-folder', t('files.menu.newFolder'), { icon: 'folder' }),
      item('new-text', t('files.menu.newTextFile'), { icon: 'fileText' }),
      item('upload', t('files.menu.uploadFiles'), { icon: 'upload' }),
      separator(),
      ...ZOOM_STEPS.map((v) => item(`view:${v}`, VIEW_LABELS[v], { checked: mode === v })),
      separator(),
      item('refresh', t('files.menu.refresh'), { icon: 'refresh' }),
    ];
  }

  function runBackground(choice: string) {
    if (choice === 'new-folder') void newFolder();
    else if (choice === 'new-text') void newTextFile();
    else if (choice === 'upload') picker?.click();
    else if (choice === 'empty') confirm = { kind: 'empty' };
    else if (choice.startsWith('view:')) mode = choice.slice(5) as FileView;
    else if (choice === 'refresh') void view.refresh();
  }

  function buildMenu(target: HTMLElement) {
    const row = target.closest<HTMLElement>('[data-path]');
    const e = row ? entries.find((x) => x.path === row.dataset.path) : undefined;
    if (e) {
      selected = e.path;
      const list = entryMenu(e);
      // Folders can also open in a new tab of this window.
      if (e.kind === 'folder' && !inTrash) {
        list.splice(1, 0, item('open-tab', t('files.menu.openTab'), { icon: 'plus' }));
      }
      return {
        entries: list,
        label: e.name,
        header: entryHeader(e),
        onSelect: (c: string) => runItem(e, c),
      };
    }
    return { entries: backgroundMenu(), label: crumbs.at(-1)!.label, onSelect: runBackground };
  }

  function clickItem(e: FileEntry) {
    const now = performance.now();
    // One tap or click selects; a second one on the same item soon after opens it (touch too).
    if (lastClick.path === e.path && now - lastClick.at < 450) {
      lastClick = { path: '', at: 0 };
      open(e);
    } else {
      lastClick = { path: e.path, at: now };
      selected = e.path;
    }
  }

  /** How many icons fit in a row, for the arrow keys in the icon views. */
  function columns(): number {
    if (mode === 'details' || !listEl) return 1;
    const opts = [...listEl.querySelectorAll<HTMLElement>('[role="option"]')];
    const top = opts[0]?.offsetTop;
    const n = opts.findIndex((o) => o.offsetTop !== top);
    return n <= 0 ? Math.max(1, opts.length) : n;
  }

  function listKey(ev: KeyboardEvent) {
    if (renaming) return;
    if (ev.altKey && ev.key === 'ArrowLeft') return (ev.preventDefault(), view.back());
    if (ev.altKey && ev.key === 'ArrowRight') return (ev.preventDefault(), view.forward());
    if ((ev.altKey && ev.key === 'ArrowUp') || ev.key === 'Backspace')
      return (ev.preventDefault(), view.up());
    const i = entries.findIndex((e) => e.path === selected);
    const cols = columns();
    const step: Record<string, number> = {
      ArrowDown: cols,
      ArrowUp: -cols,
      ArrowRight: mode === 'details' ? 0 : 1,
      ArrowLeft: mode === 'details' ? 0 : -1,
    };
    if (ev.key in step && step[ev.key] !== 0 && entries.length) {
      ev.preventDefault();
      const next = Math.min(entries.length - 1, Math.max(0, i + step[ev.key]!));
      selected = entries[i === -1 ? 0 : next]!.path;
      void focusList();
    } else if (ev.key === 'Home' || ev.key === 'End') {
      ev.preventDefault();
      selected = (ev.key === 'Home' ? entries[0] : entries.at(-1))?.path ?? null;
      void focusList();
    } else if (current && ev.key === 'Enter' && ev.altKey) {
      ev.preventDefault();
      openProperties(desktop, current);
    } else if (current && ev.key === 'Enter') {
      ev.preventDefault();
      open(current);
    } else if (current && ev.key === 'F2' && !inTrash && !current.special) {
      ev.preventDefault();
      void startRename(current.path, current.name);
    } else if (current && ev.key === 'Delete' && !current.special) {
      ev.preventDefault();
      runItem(current, inTrash || onDisk ? 'delete' : 'trash');
    }
  }

  /** Ctrl +, Ctrl − and Ctrl 0 change the view, anywhere in the tab. */
  function paneKey(ev: KeyboardEvent) {
    if (!(ev.ctrlKey || ev.metaKey) || ev.altKey) return;
    if ((ev.target as HTMLElement).closest('input, textarea')) return;
    const next = keyZoom(mode, ev.key);
    if (next === null) return;
    ev.preventDefault();
    mode = next;
  }

  /** Ctrl and the wheel (a trackpad pinch arrives as this too). */
  function onWheel(ev: WheelEvent) {
    if (!(ev.ctrlKey || ev.metaKey)) return;
    ev.preventDefault();
    const r = wheelZoom(mode, wheelCarry, ev.deltaY);
    mode = r.view;
    wheelCarry = r.carried;
  }

  function spread(): number {
    const [a, b] = [...touches.values()];
    return a && b ? Math.hypot(a.x - b.x, a.y - b.y) : 0;
  }

  function touchDown(ev: PointerEvent) {
    if (ev.pointerType !== 'touch') return;
    touches.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
    if (touches.size === 2) {
      pinchStart = spread();
      pinchView = mode;
    }
  }

  function touchMove(ev: PointerEvent) {
    if (!touches.has(ev.pointerId)) return;
    touches.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
    if (touches.size !== 2 || pinchStart === 0) return;
    const next = pinchZoom(pinchView, pinchStart, spread());
    if (next !== pinchView) {
      mode = next;
      // One step per gesture stretch: measure the next step from here.
      pinchView = next;
      pinchStart = spread();
    }
  }

  function touchUp(ev: PointerEvent) {
    touches.delete(ev.pointerId);
    if (touches.size < 2) pinchStart = 0;
  }

  function dragOver(e: DragEvent) {
    if (inTrash || !e.dataTransfer?.types.includes('Files')) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    dropping = true;
  }

  /** Dropped connector packages open the Install window; other files are uploaded here. */
  function drop(e: DragEvent) {
    if (!dropping) return;
    e.preventDefault();
    dropping = false;
    const files = Array.from(e.dataTransfer?.files ?? []);
    for (const f of files.filter((f) => isPackageFile(f.name))) openDroppedPackage(desktop, f);
    void upload(files.filter((f) => !isPackageFile(f.name)));
  }

  const VIEW_ICONS: Record<FileView, IconName> = {
    details: 'details',
    small: 'iconsSmall',
    large: 'grid',
  };
  const iconSize = $derived(mode === 'large' ? 64 : mode === 'small' ? 40 : 22);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (Ctrl +/−/0 change the view) -->
<div class="files" class:busy hidden={!active} onkeydown={paneKey}>
  <div class="toolbar">
    <div class="nav">
      <button
        class="btn quiet icon"
        aria-label={t('files.toolbar.back')}
        title={t('files.toolbar.backHint')}
        disabled={!view.canBack}
        onclick={() => view.back()}><Icon name="back" /></button
      >
      <button
        class="btn quiet icon"
        aria-label={t('files.toolbar.forward')}
        title={t('files.toolbar.forwardHint')}
        disabled={!view.canForward}
        onclick={() => view.forward()}><Icon name="forward" /></button
      >
      <button
        class="btn quiet icon"
        aria-label={t('files.toolbar.up')}
        title={t('files.toolbar.upHint')}
        disabled={!view.canUp}
        onclick={() => view.up()}><Icon name="up" /></button
      >
    </div>
    <nav class="crumbs" aria-label={t('files.toolbar.path')}>
      <ol>
        {#each crumbs as c, i (c.path)}
          <li>
            {#if i < crumbs.length - 1}
              <button class="crumb" onclick={() => view.go(c.path)}>{c.label}</button>
              <span class="slash" aria-hidden="true"><Icon name="chevron" size={14} /></span>
            {:else}
              <span class="crumb here" aria-current="location">{c.label}</span>
            {/if}
          </li>
        {/each}
      </ol>
    </nav>
    {#if !inTrash}
      <button
        class="btn tool"
        onclick={newFolder}
        disabled={busy}
        title={t('files.toolbar.newFolder')}
      >
        <Icon name="plus" size={17} /><span class="tool-label">{t('files.toolbar.newFolder')}</span>
      </button>
      <button
        class="btn tool"
        onclick={() => picker?.click()}
        disabled={busy}
        title={t('files.toolbar.upload')}
      >
        <Icon name="upload" size={17} /><span class="tool-label">{t('files.toolbar.upload')}</span>
      </button>
    {/if}
    <input
      bind:this={picker}
      type="file"
      multiple
      hidden
      onchange={(e) => {
        const input = e.currentTarget as HTMLInputElement;
        void upload(input.files).then(() => (input.value = ''));
      }}
    />
    <div
      class="segmented"
      role="radiogroup"
      aria-label={t('files.toolbar.showAs')}
      title={t('files.toolbar.showAsHint')}
    >
      {#each ZOOM_STEPS as v (v)}
        <label title={VIEW_LABELS[v]}
          ><input type="radio" name="{id}-mode" value={v} bind:group={mode} /><Icon
            name={VIEW_ICONS[v]}
            size={17}
          /><span class="visually-hidden">{VIEW_LABELS[v]}</span></label
        >
      {/each}
    </div>
  </div>

  <div class="main">
    <nav class="side" aria-label={t('files.side.places')}>
      <ul>
        {#each SPECIALS as s (s.id)}
          <li>
            <button
              class="place"
              class:on={view.path === s.path}
              aria-current={view.path === s.path ? 'location' : undefined}
              onclick={() => view.go(s.path)}
            >
              <Icon name={s.icon} size={18} /><span>{s.label}</span>
            </button>
          </li>
        {/each}
      </ul>
      {#if disks.length}
        <p class="side-head" id="{id}-disks">{t('files.side.disks')}</p>
        <ul aria-labelledby="{id}-disks">
          {#each disks as d (d.id)}
            <li>
              <button
                class="place"
                class:on={root === d.id}
                aria-current={view.path === d.path ? 'location' : undefined}
                onclick={() => view.go(d.path)}
              >
                <Icon name={/sd/i.test(d.label) ? 'sd' : 'disk'} size={18} /><span>{d.label}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      {#if machineOn && machine}
        <ul class="gap">
          <li>
            <button
              class="place"
              class:on={root === 'machine'}
              onclick={() => view.go(machine.path)}
            >
              <Icon name="computer" size={18} /><span>{machine.label}</span>
            </button>
          </li>
        </ul>
      {/if}
    </nav>

    <section
      class="pane"
      class:dropping
      aria-label={t('files.pane.contentsOf', { folder: crumbs.at(-1)?.label ?? '' })}
      ondragover={dragOver}
      ondragleave={(e) => {
        if (!(e.currentTarget as Element).contains(e.relatedTarget as Node)) dropping = false;
      }}
      ondrop={drop}
    >
      {#if root === 'machine' && machine?.warning}
        <div class="banner danger" role="note">
          <Icon name="warning" size={18} />
          <p>{machine.warning}</p>
        </div>
      {/if}

      {#if inTrash}
        <div class="actions">
          <p class="explain">{t('files.explain.trash')}</p>
          <button
            class="btn danger"
            disabled={entries.length === 0 || busy}
            onclick={() => (confirm = { kind: 'empty' })}>{t('files.menu.emptyTrash')}</button
          >
        </div>
      {:else if inSystem}
        <div class="actions">
          <p class="explain"><RichText text={t('files.explain.system')} /></p>
        </div>
      {:else if onDisk}
        <div class="actions">
          <p class="explain">{t('files.explain.disk')}</p>
        </div>
      {/if}

      {#if view.error && !(view.error.code === 'not-found' && onDisk)}
        <div class="pad">
          <ErrorNotice error={view.error}>
            {#snippet actions()}
              <button class="btn" onclick={() => view.refresh()}
                >{t('files.action.tryAgain')}</button
              >
            {/snippet}
          </ErrorNotice>
        </div>
      {/if}
      {#if actionError}
        <div class="pad">
          <ErrorNotice error={actionError}>
            {#snippet actions()}
              <button class="btn quiet" onclick={() => (actionError = null)}
                >{t('files.action.dismiss')}</button
              >
            {/snippet}
          </ErrorNotice>
        </div>
      {/if}
      {#if notice}
        <p class="pad notice" role="status">{notice}</p>
      {/if}

      <div class="content">
        <!-- svelte-ignore a11y_no_static_element_interactions (a click on empty space clears the selection) -->
        <div
          class="scroll"
          use:menuTrigger={{ menu, build: buildMenu }}
          onpointerdown={(e) => {
            touchDown(e);
            if (e.target === e.currentTarget) selected = null;
          }}
          onpointermove={touchMove}
          onpointerup={touchUp}
          onpointercancel={touchUp}
          onwheel={onWheel}
        >
          {#if view.listing === null && view.loading}
            <p class="empty muted">{t('files.pane.loading')}</p>
          {:else if view.listing && entries.length === 0}
            {#if inRecordings}
              <div class="empty">
                <h3>{t('files.empty.recordings.title')}</h3>
                <p>{t('files.empty.recordings.body')}</p>
                <p><RichText text={t('files.empty.recordings.sample')} /></p>
                <button class="btn primary" onclick={addSamples} disabled={busy}>
                  {t('files.empty.recordings.addSamples')}
                </button>
              </div>
            {:else if inTrash}
              <p class="empty muted">{t('files.empty.trash')}</p>
            {:else}
              <p class="empty muted">
                {inSystem ? t('files.empty.folder') : t('files.empty.folderDrop')}
              </p>
            {/if}
          {:else if view.listing}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions (keys act on the selected option) -->
            <ul
              bind:this={listEl}
              class="view-{mode}"
              role="listbox"
              aria-label={t('files.pane.list')}
              tabindex={selected ? -1 : 0}
              onkeydown={listKey}
            >
              {#if mode === 'details'}
                <li class="head" role="presentation" aria-hidden="true">
                  <span>{t('files.column.name')}</span><span
                    >{inTrash ? t('files.column.from') : t('files.column.size')}</span
                  ><span>{t('files.column.modified')}</span>
                </li>
              {/if}
              {#each entries as e (e.path)}
                {@const look = fileLook(e)}
                <li
                  class="entry"
                  role="option"
                  aria-selected={selected === e.path}
                  tabindex={selected === e.path ? 0 : -1}
                  data-path={e.path}
                  onclick={() => clickItem(e)}
                  onkeydown={() => {}}
                >
                  <span class="name">
                    <span class="ico"
                      ><FileIcon type={look.type} emblem={look.emblem} size={iconSize} /></span
                    >
                    {#if renaming === e.path}
                      <input
                        class="input rename"
                        aria-label={t('files.pane.renameInput', { name: e.name })}
                        bind:value={renameValue}
                        onclick={(ev) => ev.stopPropagation()}
                        onkeydown={(ev) => {
                          ev.stopPropagation();
                          if (ev.key === 'Enter') void commitRename();
                          else if (ev.key === 'Escape') cancelRename();
                        }}
                        onblur={commitRename}
                        spellcheck="false"
                        autocomplete="off"
                      />
                    {:else}
                      <span class="label">{e.name}</span>
                    {/if}
                  </span>
                  {#if mode === 'details'}
                    <span class="meta num">
                      {#if inTrash}{e.originalPath ?? ''}{:else if e.kind === 'file'}{formatBytes(
                          e.size,
                        )}{/if}
                    </span>
                    <span class="meta num">{formatDateTime(e.modifiedMs, core.wallClock)}</span>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
          {#if dropping}
            <div class="drop" aria-hidden="true">
              <Icon name="upload" size={28} />
              <p>{t('files.pane.dropHere', { folder: crumbs.at(-1)?.label ?? '' })}</p>
            </div>
          {/if}
        </div>

        {#if mode === 'details' && view.listing}
          <aside class="details" aria-label={t('files.details.label')}>
            {#if current}
              {@const look = fileLook(current)}
              {@const replay = replayPortFor(current.path)}
              <div class="preview">
                <FileIcon type={look.type} emblem={look.emblem} size={72} />
              </div>
              <p class="d-name selectable">{current.name}</p>
              <dl class="facts">
                <dt>{t('files.details.kind')}</dt>
                <dd>{fileTypeLabel(current)}</dd>
                {#if current.kind === 'file'}
                  <dt>{t('files.details.size')}</dt>
                  <dd class="num">{formatBytes(current.size)}</dd>
                {/if}
                {#if current.modifiedMs !== undefined}
                  <dt>{t('files.details.modified')}</dt>
                  <dd class="num">{formatDateTime(current.modifiedMs, core.wallClock)}</dd>
                {/if}
                {#if replay}
                  <dt>{t('files.details.port')}</dt>
                  <dd><code class="selectable">{replay}</code></dd>
                {/if}
              </dl>
              <div class="d-actions">
                {#if replay}
                  <button class="btn primary" onclick={() => addPartOn(replay)}
                    >{t('files.details.addPart')}</button
                  >
                {:else if !inTrash}
                  <button class="btn" onclick={() => open(current)}
                    >{t('files.details.open')}</button
                  >
                {/if}
                {#if current.kind === 'file'}
                  <button class="btn" onclick={() => download(core, current.path)}
                    >{t('files.details.download')}</button
                  >
                {/if}
              </div>
            {:else}
              <div class="preview">
                <FileIcon type={onDisk ? 'disk' : 'folder'} size={72} />
              </div>
              <p class="d-name">{crumbs.at(-1)?.label}</p>
              <p class="muted small">
                {t('files.details.folderSummary', { count: entries.length })}
              </p>
            {/if}
          </aside>
        {/if}
      </div>

      <p class="status" aria-live="polite">
        {#if view.listing}
          <span class="num"
            >{current
              ? t('files.status.itemsSelected', { count: entries.length })
              : t('files.status.items', { count: entries.length })}</span
          >
        {/if}
        <span class="where">{VIEW_LABELS[mode]}</span>
      </p>
    </section>
  </div>

  {#if conflict}
    <UploadConflict {...conflict} />
  {:else if confirm?.kind === 'delete'}
    {@const e = confirm.entry}
    <Dialog
      title={t('files.dialog.deleteForGood.title', { name: e.name })}
      tone="danger"
      oncancel={() => (confirm = null)}
    >
      <p>
        {isOnDisk(e.path)
          ? t('files.dialog.deleteForGood.bodyDisk')
          : t('files.dialog.deleteForGood.body')}
      </p>
      {#snippet actions()}
        <button class="btn" onclick={() => (confirm = null)}>{t('files.action.cancel')}</button>
        <button
          class="btn danger solid"
          onclick={() => {
            const path = e.path;
            confirm = null;
            void act(() => core.deleteForGood([path]));
          }}>{t('files.dialog.deleteForGood.confirm')}</button
        >
      {/snippet}
    </Dialog>
  {:else if confirm?.kind === 'empty'}
    <Dialog
      title={t('files.dialog.emptyTrash.title')}
      tone="danger"
      oncancel={() => (confirm = null)}
    >
      <p>
        {t('files.dialog.emptyTrash.body', { count: entries.length })}
      </p>
      {#snippet actions()}
        <button class="btn" onclick={() => (confirm = null)}>{t('files.action.cancel')}</button>
        <button
          class="btn danger solid"
          onclick={() => {
            confirm = null;
            void act(() => core.emptyTrash());
          }}>{t('files.dialog.emptyTrash.confirm')}</button
        >
      {/snippet}
    </Dialog>
  {/if}
</div>

<style>
  .files[hidden] {
    display: none;
  }
  .files {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
    container-type: inline-size;
  }
  .files.busy {
    cursor: progress;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line);
    background: var(--surface);
  }
  .nav {
    display: flex;
  }
  .crumbs {
    flex: 1;
    min-width: 0;
    height: 36px;
    display: flex;
    align-items: center;
    padding: 0 var(--space-1);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-2);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .crumbs ol {
    display: flex;
    align-items: center;
    margin: 0;
    padding: 0;
    list-style: none;
    white-space: nowrap;
  }
  .crumbs li {
    display: flex;
    align-items: center;
  }
  .crumb {
    min-height: 30px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  button.crumb:hover {
    background: var(--hover);
    color: var(--text);
  }
  .crumb.here {
    display: inline-flex;
    align-items: center;
    font-weight: 700;
    color: var(--text);
  }
  .slash {
    display: inline-flex;
    color: var(--text-faint);
  }
  .tool {
    flex: none;
  }
  @container (max-width: 720px) {
    .tool-label {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip: rect(0 0 0 0);
    }
    .tool {
      padding: 0 var(--space-2);
    }
  }
  .segmented label {
    padding: 0 var(--space-2);
  }

  .main {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .side {
    flex: none;
    width: 184px;
    padding: var(--space-2);
    border-right: 1px solid var(--line);
    background: var(--surface-2);
    overflow-y: auto;
  }
  .side ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 1px;
  }
  .side-head {
    margin-top: var(--space-3);
    padding: var(--space-2) var(--space-2) var(--space-1);
    border-top: 1px solid var(--line);
    font-size: var(--text-xs);
    font-weight: 700;
    color: var(--text-muted);
  }
  .side .gap {
    margin-top: var(--space-3);
    padding-top: var(--space-3);
    border-top: 1px solid var(--line);
  }
  .place {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: var(--touch);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: none;
    color: var(--text-muted);
    font-size: var(--text-sm);
    text-align: left;
  }
  .place span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .place:hover {
    background: var(--hover);
    color: var(--text);
  }
  .place.on {
    background: var(--selected);
    color: var(--text);
    font-weight: 700;
  }
  .place.on :global(svg) {
    color: var(--accent-ink);
  }
  @container (max-width: 560px) {
    .side {
      width: auto;
    }
    .side-head {
      font-size: 0;
      padding: 0;
    }
    .place span {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip: rect(0 0 0 0);
    }
    .place {
      width: var(--touch);
      justify-content: center;
      padding: 0;
    }
  }

  .pane {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .banner {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    padding: var(--space-3) var(--space-4);
    font-size: var(--text-sm);
    font-weight: 600;
    user-select: text;
  }
  .banner.danger {
    background: var(--danger-soft);
    color: var(--danger-text);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line);
  }
  .explain {
    flex: 1;
    min-width: 0;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .pad {
    padding: var(--space-3) var(--space-3) 0;
  }
  .notice {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .content {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .scroll {
    position: relative;
    flex: 1;
    min-width: 0;
    overflow: auto;
  }
  .empty {
    display: grid;
    justify-items: start;
    gap: var(--space-3);
    max-width: 56ch;
    padding: var(--space-5);
  }
  .empty h3 {
    font-size: var(--text-xl);
    font-weight: 700;
  }
  .empty p {
    color: var(--text-muted);
  }
  .muted {
    color: var(--text-muted);
  }
  .small {
    font-size: var(--text-sm);
  }

  ul[role='listbox'] {
    margin: 0;
    padding: var(--space-1) var(--space-2) var(--space-3);
    list-style: none;
    outline: none;
  }
  .view-details .head,
  .view-details .entry {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 6.5rem 10.5rem;
    align-items: center;
    gap: var(--space-3);
    padding: 0 var(--space-2);
  }
  .view-details .head {
    position: sticky;
    top: 0;
    z-index: 1;
    height: 30px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
    font-size: var(--text-xs);
    font-weight: 650;
    color: var(--text-muted);
  }
  @container (max-width: 820px) {
    .view-details .head,
    .view-details .entry {
      grid-template-columns: minmax(0, 1fr) 5.5rem;
    }
    .view-details .head span:last-child,
    .view-details .entry .meta:last-child {
      display: none;
    }
  }
  .entry {
    min-height: var(--touch);
    border-radius: var(--radius-md);
    cursor: default;
    outline: none;
  }
  .view-details .entry {
    border-radius: var(--radius-sm);
    min-height: 40px;
  }
  .entry:hover {
    background: var(--hover);
  }
  .entry[aria-selected='true'] {
    background: var(--selected);
  }
  .entry:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .ico {
    display: inline-flex;
    flex: none;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .rename {
    flex: 1;
    min-height: 32px;
    padding: 0 var(--space-2);
  }

  .view-small,
  .view-large {
    display: grid;
    gap: var(--space-1);
    align-content: start;
  }
  .view-small {
    grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  }
  .view-large {
    grid-template-columns: repeat(auto-fill, minmax(128px, 1fr));
    gap: var(--space-2);
    padding: var(--space-3) !important;
  }
  .view-small .entry,
  .view-large .entry {
    padding: var(--space-2) var(--space-1);
  }
  .view-small .name,
  .view-large .name {
    flex-direction: column;
    text-align: center;
    gap: 6px;
  }
  .view-small .label,
  .view-large .label {
    white-space: normal;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    font-size: var(--text-sm);
  }

  .details {
    flex: none;
    width: 240px;
    padding: var(--space-4);
    border-left: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    overflow-y: auto;
  }
  @container (max-width: 760px) {
    .details {
      display: none;
    }
  }
  .preview {
    display: grid;
    place-items: center;
    height: 112px;
    border-radius: var(--radius-lg);
    background: var(--surface-2);
  }
  .d-name {
    font-weight: 700;
    overflow-wrap: anywhere;
  }
  .facts {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px var(--space-3);
    margin: 0;
    font-size: var(--text-sm);
  }
  .facts dt {
    color: var(--text-muted);
  }
  .facts dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .d-actions {
    display: grid;
    gap: var(--space-2);
  }

  .drop {
    position: absolute;
    inset: var(--space-2);
    display: grid;
    place-content: center;
    justify-items: center;
    gap: var(--space-2);
    border: 2px dashed var(--accent);
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-weight: 650;
    pointer-events: none;
  }
  .status {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 28px;
    padding: 0 var(--space-3);
    border-top: 1px solid var(--line);
    background: var(--surface-2);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .status .where {
    margin-left: auto;
  }
  .ack {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin-top: var(--space-3);
    font-weight: 600;
  }
</style>
