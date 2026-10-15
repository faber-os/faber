<script lang="ts">
  import { tick } from 'svelte';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody, FileEntry, FileRoot } from '../lib/state/types';
  import { item, separator } from '../lib/desktop/contextMenu';
  import {
    RECORDINGS,
    TRASH,
    breadcrumbs,
    childPath,
    parentPath,
    splitPath,
    stem,
    uniqueName,
  } from '../lib/desktop/filePaths';
  import Dialog from '../components/Dialog.svelte';
  import { useDesktop } from '../components/desktopController.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import {
    download,
    entryMenu,
    openEntry,
    openInNotes,
    openProperties,
  } from '../components/fileOps';
  import { SPECIALS, entryIcon } from '../components/fileLook';
  import { FolderView } from '../components/folderView.svelte';
  import { formatBytes, formatModified } from '../components/format';
  import Icon from '../components/Icon.svelte';
  import { menuTrigger, useMenu } from '../components/menuController.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win }: WindowProps = $props();

  const core = useCore();
  const desktop = useDesktop();
  const menu = useMenu();
  // svelte-ignore state_referenced_locally (the window opens where it was asked to; it then browses on its own)
  const view = new FolderView(core, win.params.path ?? 'home:/Desktop');

  let mode = $state<'list' | 'grid'>('list');
  let selected = $state<string | null>(null);
  let renaming = $state<string | null>(null);
  let renameValue = $state('');
  let actionError = $state<ErrorBody | null>(null);
  let notice = $state<string | null>(null);
  let confirm = $state<{ kind: 'delete'; entry: FileEntry } | { kind: 'empty' } | null>(null);
  let dropping = $state(false);
  let busy = $state(false);
  let roots = $state.raw<FileRoot[]>([]);
  let picker: HTMLInputElement | undefined = $state();
  let listEl: HTMLElement | undefined = $state();
  let lastClick = { path: '', at: 0 };

  $effect(() => view.start());

  // Whole-machine access shows "This computer"; ask for the roots again whenever it changes.
  const machineOn = $derived(core.settings?.developer.fullMachineFiles ?? false);
  $effect(() => {
    void machineOn;
    void core.getRoots().then((r) => {
      if (r.ok) roots = r.value;
    });
  });

  const entries = $derived(view.listing?.entries ?? []);
  const root = $derived(splitPath(view.path).root);
  const machine = $derived(roots.find((r) => r.id === 'machine'));
  const rootLabel = $derived(
    root === 'home' ? 'Home' : (roots.find((r) => r.id === root)?.label ?? root),
  );
  const crumbs = $derived(breadcrumbs(view.path, rootLabel));
  const inTrash = $derived(view.path === TRASH);
  const inRecordings = $derived(view.path === RECORDINGS);
  const inSystem = $derived(view.path === 'home:/System');
  const here = $derived(SPECIALS.find((s) => s.path === view.path));
  const current = $derived(entries.find((e) => e.path === selected) ?? null);

  // The title follows the folder, so the taskbar tells two Files windows apart.
  $effect(() => {
    const name = crumbs.at(-1)!.label;
    desktop.update(win.id, { title: `${name} · Files` });
  });

  // A new folder clears the selection and any message about the old one.
  let shownPath = '';
  $effect(() => {
    if (view.path === shownPath) return;
    shownPath = view.path;
    selected = null;
    renaming = null;
    notice = null;
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
      'New folder',
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
      'New text file.txt',
    );
    const path = childPath(view.path, name);
    if (await act(() => core.writeFile(path, new Uint8Array()))) {
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
    if (await act(() => core.move(from, to))) selected = to;
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

  async function upload(files: FileList | File[] | null | undefined) {
    if (!files || files.length === 0) return;
    const names = entries.map((e) => e.name);
    const renamed: string[] = [];
    let lastPath: string | null = null;
    busy = true;
    actionError = null;
    for (const f of Array.from(files)) {
      const name = uniqueName(names, f.name);
      names.push(name);
      if (name !== f.name) renamed.push(`${f.name} → ${name}`);
      const path = childPath(view.path, name);
      const r = await core.writeFile(path, f);
      if (!r.ok) {
        actionError = r.error;
        break;
      }
      lastPath = path;
    }
    busy = false;
    notice = renamed.length
      ? `A file with the same name was already here, so the upload got a new name: ${renamed.join(', ')}.`
      : null;
    await view.refresh(true);
    if (lastPath) selected = lastPath;
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

  function runItem(e: FileEntry, id: string) {
    selected = e.path;
    switch (id) {
      case 'open':
        return open(e);
      case 'notes':
        return openInNotes(desktop, e.path);
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
        item('empty', 'Empty Trash', {
          danger: true,
          disabledReason: entries.length === 0 ? 'The Trash is already empty.' : undefined,
        }),
        separator(),
        item('refresh', 'Refresh'),
      ];
    }
    return [
      item('new-folder', 'New folder'),
      item('new-text', 'New text file'),
      item('upload', 'Upload files…'),
      separator(),
      item('view', mode === 'list' ? 'Show as icons' : 'Show as list'),
      item('refresh', 'Refresh'),
    ];
  }

  function runBackground(id: string) {
    if (id === 'new-folder') void newFolder();
    else if (id === 'new-text') void newTextFile();
    else if (id === 'upload') picker?.click();
    else if (id === 'empty') confirm = { kind: 'empty' };
    else if (id === 'view') mode = mode === 'list' ? 'grid' : 'list';
    else if (id === 'refresh') void view.refresh();
  }

  function buildMenu(target: HTMLElement) {
    const row = target.closest<HTMLElement>('[data-path]');
    const e = row ? entries.find((x) => x.path === row.dataset.path) : undefined;
    if (e) {
      selected = e.path;
      return { entries: entryMenu(e), label: e.name, onSelect: (id: string) => runItem(e, id) };
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

  function listKey(ev: KeyboardEvent) {
    if (renaming) return;
    if (ev.altKey && ev.key === 'ArrowLeft') return (ev.preventDefault(), view.back());
    if (ev.altKey && ev.key === 'ArrowRight') return (ev.preventDefault(), view.forward());
    if ((ev.altKey && ev.key === 'ArrowUp') || ev.key === 'Backspace')
      return (ev.preventDefault(), view.up());
    const i = entries.findIndex((e) => e.path === selected);
    const step: Record<string, number> = {
      ArrowDown: 1,
      ArrowUp: -1,
      ArrowRight: mode === 'grid' ? 1 : 0,
      ArrowLeft: mode === 'grid' ? -1 : 0,
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
    } else if (current && ev.key === 'Enter') {
      ev.preventDefault();
      open(current);
    } else if (current && ev.key === 'F2' && !inTrash && !current.special) {
      ev.preventDefault();
      void startRename(current.path, current.name);
    } else if (current && ev.key === 'Delete' && !current.special) {
      ev.preventDefault();
      runItem(current, inTrash ? 'delete' : 'trash');
    }
  }

  function dragOver(e: DragEvent) {
    if (inTrash || !e.dataTransfer?.types.includes('Files')) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    dropping = true;
  }

  function drop(e: DragEvent) {
    if (!dropping) return;
    e.preventDefault();
    dropping = false;
    void upload(e.dataTransfer?.files);
  }
</script>

<div class="files" class:busy>
  <div class="toolbar">
    <div class="nav">
      <button
        class="btn quiet icon"
        aria-label="Back"
        title="Back (Alt+Left)"
        disabled={!view.canBack}
        onclick={() => view.back()}><Icon name="back" /></button
      >
      <button
        class="btn quiet icon"
        aria-label="Forward"
        title="Forward (Alt+Right)"
        disabled={!view.canForward}
        onclick={() => view.forward()}><Icon name="forward" /></button
      >
      <button
        class="btn quiet icon"
        aria-label="Up one folder"
        title="Up one folder (Alt+Up)"
        disabled={!view.canUp}
        onclick={() => view.up()}><Icon name="up" /></button
      >
    </div>
    <nav class="crumbs" aria-label="Folder path">
      <ol>
        {#each crumbs as c, i (c.path)}
          <li>
            {#if i < crumbs.length - 1}
              <button class="crumb" onclick={() => view.go(c.path)}>{c.label}</button>
              <span class="slash" aria-hidden="true">/</span>
            {:else}
              <span class="crumb here" aria-current="location">{c.label}</span>
            {/if}
          </li>
        {/each}
      </ol>
    </nav>
    <div class="segmented" role="radiogroup" aria-label="Show as">
      <label title="List"
        ><input type="radio" name="{win.id}-mode" value="list" bind:group={mode} /><Icon
          name="list"
          size={17}
        /><span class="visually-hidden">List</span></label
      >
      <label title="Icons"
        ><input type="radio" name="{win.id}-mode" value="grid" bind:group={mode} /><Icon
          name="grid"
          size={17}
        /><span class="visually-hidden">Icons</span></label
      >
    </div>
  </div>

  <div class="main">
    <nav class="side" aria-label="Places">
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
        {#if machineOn && machine}
          <li class="gap">
            <button
              class="place"
              class:on={root === 'machine'}
              onclick={() => view.go(machine.path)}
            >
              <Icon name="computer" size={18} /><span>{machine.label}</span>
            </button>
          </li>
        {/if}
      </ul>
    </nav>

    <section
      class="pane"
      class:dropping
      aria-label="Contents of {crumbs.at(-1)?.label}"
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

      <div class="actions">
        {#if inTrash}
          <p class="explain">
            Files here can be restored. Emptying the Trash deletes them for good.
          </p>
          <button
            class="btn danger"
            disabled={entries.length === 0 || busy}
            onclick={() => (confirm = { kind: 'empty' })}>Empty Trash</button
          >
        {:else}
          {#if inSystem}
            <p class="explain">
              <code>parts.toml</code> here is your machine config. Saving it applies it at once; a broken
              file is refused with the line that is wrong.
            </p>
          {:else}
            <span class="spacer"></span>
          {/if}
          <button class="btn" onclick={newFolder} disabled={busy}>
            <Icon name="plus" size={17} /> New folder
          </button>
          <button class="btn" onclick={() => picker?.click()} disabled={busy}>
            <Icon name="upload" size={17} /> Upload
          </button>
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
        {/if}
      </div>

      {#if view.error}
        <div class="pad">
          <ErrorNotice error={view.error}>
            {#snippet actions()}
              <button class="btn" onclick={() => view.refresh()}>Try again</button>
            {/snippet}
          </ErrorNotice>
        </div>
      {/if}
      {#if actionError}
        <div class="pad">
          <ErrorNotice error={actionError}>
            {#snippet actions()}
              <button class="btn quiet" onclick={() => (actionError = null)}>Dismiss</button>
            {/snippet}
          </ErrorNotice>
        </div>
      {/if}
      {#if notice}
        <p class="pad notice" role="status">{notice}</p>
      {/if}

      <!-- svelte-ignore a11y_no_static_element_interactions (a click on empty space clears the selection) -->
      <div
        class="scroll"
        use:menuTrigger={{ menu, build: buildMenu }}
        onpointerdown={(e) => {
          if (e.target === e.currentTarget) selected = null;
        }}
      >
        {#if view.listing === null && view.loading}
          <p class="empty muted">Loading…</p>
        {:else if view.listing && entries.length === 0}
          {#if inRecordings}
            <div class="empty">
              <h3>No recordings yet</h3>
              <p>
                Record a port with the serial monitor's Record button and it appears here. Every
                recording is also a replay port, so you can add a part on it as if the hardware were
                plugged in.
              </p>
              <p>
                No hardware yet? Faber comes with one sample: a short recording of an LD19 LiDAR in
                a room. Adding it copies it here as <code>ld19-sample.fbcap</code>.
              </p>
              <button class="btn primary" onclick={addSamples} disabled={busy}>
                Add sample recordings
              </button>
            </div>
          {:else if inTrash}
            <p class="empty muted">The Trash is empty.</p>
          {:else}
            <p class="empty muted">
              This folder is empty.{#if !inSystem}
                Drop files here from your computer to upload them.{/if}
            </p>
          {/if}
        {:else if view.listing}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions (keys act on the selected option) -->
          <ul
            bind:this={listEl}
            class={mode}
            role="listbox"
            aria-label="Files and folders"
            tabindex={selected ? -1 : 0}
            onkeydown={listKey}
          >
            {#if mode === 'list'}
              <li class="head" role="presentation" aria-hidden="true">
                <span>Name</span><span>{inTrash ? 'From' : 'Size'}</span><span>Modified</span>
              </li>
            {/if}
            {#each entries as e (e.path)}
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
                    ><Icon name={entryIcon(e)} size={mode === 'grid' ? 34 : 18} /></span
                  >
                  {#if renaming === e.path}
                    <input
                      class="input rename"
                      aria-label="New name for {e.name}"
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
                {#if mode === 'list'}
                  <span class="meta num">
                    {#if inTrash}{e.originalPath ?? ''}{:else if e.kind === 'file'}{formatBytes(
                        e.size,
                      )}{/if}
                  </span>
                  <span class="meta num">{formatModified(e.modifiedMs)}</span>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if dropping}
          <div class="drop" aria-hidden="true">
            <Icon name="upload" size={28} />
            <p>Drop to upload to {crumbs.at(-1)?.label}</p>
          </div>
        {/if}
      </div>

      <p class="status" aria-live="polite">
        {#if view.listing}
          <span class="num">{entries.length} {entries.length === 1 ? 'item' : 'items'}</span
          >{#if current}
            <span class="sep" aria-hidden="true">·</span> {current.name}{/if}
        {/if}
        {#if here}<span class="where">{here.label}</span>{/if}
      </p>
    </section>
  </div>

  {#if confirm?.kind === 'delete'}
    {@const e = confirm.entry}
    <Dialog title="Delete “{e.name}” for good?" tone="danger" oncancel={() => (confirm = null)}>
      <p>It can't be restored afterwards.</p>
      {#snippet actions()}
        <button class="btn" onclick={() => (confirm = null)}>Cancel</button>
        <button
          class="btn danger solid"
          onclick={() => {
            const path = e.path;
            confirm = null;
            void act(() => core.deleteForGood([path]));
          }}>Delete for good</button
        >
      {/snippet}
    </Dialog>
  {:else if confirm?.kind === 'empty'}
    <Dialog title="Empty the Trash?" tone="danger" oncancel={() => (confirm = null)}>
      <p>
        All <span class="num">{entries.length}</span>
        {entries.length === 1 ? 'item' : 'items'} in the Trash will be deleted for good. They can't be
        restored afterwards.
      </p>
      {#snippet actions()}
        <button class="btn" onclick={() => (confirm = null)}>Cancel</button>
        <button
          class="btn danger solid"
          onclick={() => {
            confirm = null;
            void act(() => core.emptyTrash());
          }}>Empty Trash</button
        >
      {/snippet}
    </Dialog>
  {/if}
</div>

<style>
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
    background: var(--chrome);
  }
  .nav {
    display: flex;
  }
  .crumbs {
    flex: 1;
    min-width: 0;
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
    min-height: var(--touch);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    font-size: var(--text-md);
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
    color: var(--text-faint);
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
    width: 176px;
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
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
    font-size: var(--text-md);
    text-align: left;
  }
  .place:hover {
    background: var(--hover);
    color: var(--text);
  }
  .place.on {
    background: var(--selected);
    color: var(--text);
    font-weight: 650;
  }
  .place.on :global(svg) {
    color: var(--accent-ink);
  }
  @container (max-width: 560px) {
    .side {
      width: auto;
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
    color: var(--danger);
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
  .spacer {
    flex: 1;
  }
  .pad {
    padding: var(--space-3) var(--space-3) 0;
  }
  .notice {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .scroll {
    position: relative;
    flex: 1;
    min-height: 0;
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

  ul.list,
  ul.grid {
    margin: 0;
    padding: var(--space-1) var(--space-2) var(--space-3);
    list-style: none;
    outline: none;
  }
  .list .head,
  .list .entry {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 7.5rem 11rem;
    align-items: center;
    gap: var(--space-3);
    padding: 0 var(--space-2);
  }
  .list .head {
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
  @container (max-width: 620px) {
    .list .head,
    .list .entry {
      grid-template-columns: minmax(0, 1fr) 6rem;
    }
    .list .head span:last-child,
    .list .entry .meta:last-child {
      display: none;
    }
  }
  .entry {
    min-height: var(--touch);
    border-radius: var(--radius-sm);
    cursor: default;
    outline: none;
  }
  .list .entry:nth-child(even) {
    background: var(--surface-2);
  }
  .entry[aria-selected='true'],
  .list .entry[aria-selected='true'] {
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
    color: var(--text-muted);
  }
  .entry[aria-selected='true'] .ico {
    color: var(--accent-ink);
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

  ul.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: var(--space-1);
  }
  .grid .entry {
    padding: var(--space-2) var(--space-1);
  }
  .grid .name {
    flex-direction: column;
    text-align: center;
  }
  .grid .label {
    white-space: normal;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    font-size: var(--text-sm);
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
  .sep {
    color: var(--text-faint);
  }
</style>
