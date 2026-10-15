<script lang="ts">
  import { tick } from 'svelte';
  import { useCore } from '../lib/state/context';
  import type { FileEntry, IconPosition } from '../lib/state/types';
  import { item, separator } from '../lib/desktop/contextMenu';
  import { DESKTOP, childPath, isAffected, stem, uniqueName } from '../lib/desktop/filePaths';
  import {
    ICON_GRID,
    cellToPoint,
    gridSize,
    moveIcon,
    placeIcons,
    positionsForSettings,
    type Placement,
  } from '../lib/desktop/iconGrid';
  import { useDesktop } from './desktopController.svelte';
  import { download, entryMenu, openEntry, openInNotes, openProperties } from './fileOps';
  import { entryIcon } from './fileLook';
  import Icon, { type IconName } from './Icon.svelte';
  import { menuTrigger, useMenu } from './menuController.svelte';
  import type { WindowDef } from './windowDefs';

  let { defs }: { defs: WindowDef[] } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const menu = useMenu();
  const DRAG_START_PX = 5;

  interface DesktopIcon {
    key: string;
    label: string;
    icon: IconName;
    def?: WindowDef;
    entry?: FileEntry;
  }

  let files = $state.raw<FileEntry[]>([]);
  let selected = $state<string | null>(null);
  let renaming = $state<string | null>(null);
  let renameValue = $state('');
  /** Positions just saved, shown until the core's answer arrives. */
  let pending = $state.raw<IconPosition[] | null>(null);
  let drag = $state<{ key: string; dx: number; dy: number } | null>(null);
  let root: HTMLElement | undefined = $state();
  let lastClick = { key: '', at: 0 };
  let justDragged = false;

  async function load() {
    const r = await core.listFolder(DESKTOP);
    if (r.ok) files = r.value.entries;
  }

  // List the Desktop folder once connected, and again whenever the core says it changed.
  $effect(() => {
    if (core.connection === 'connected') void load();
  });
  $effect(() =>
    core.watchFiles((folders) => {
      if (isAffected(DESKTOP, folders)) void load();
    }),
  );
  $effect(() => {
    void core.settings;
    pending = null;
  });

  const icons = $derived<DesktopIcon[]>([
    ...defs.flatMap((d) => (d.pin ? [{ key: d.pin, label: d.title, icon: d.icon, def: d }] : [])),
    ...files.map((e) => ({ key: e.path, label: e.name, icon: entryIcon(e), entry: e })),
  ]);
  const saved = $derived(pending ?? core.settings?.desktop.icons ?? []);
  const placement = $derived(
    placeIcons(
      icons.map((i) => i.key),
      saved,
      desktop.area,
      ICON_GRID,
    ),
  );
  // Keyboard order: down each column, then the next column, like the auto-placement.
  const ordered = $derived(
    [...icons].sort((a, b) => {
      const ca = placement.get(a.key)!;
      const cb = placement.get(b.key)!;
      return ca.col - cb.col || ca.row - cb.row;
    }),
  );

  async function save(next: Placement) {
    const settings = core.settings;
    const icons = positionsForSettings(next, ICON_GRID);
    pending = icons;
    if (!settings) return;
    const r = await core.saveSettings({ ...settings, desktop: { ...settings.desktop, icons } });
    if (!r.ok) {
      pending = null;
      core.lastError = r.error;
    }
  }

  function open(i: DesktopIcon) {
    if (i.def) desktop.open(i.def.kind);
    else if (i.entry) openEntry(desktop, i.entry);
  }

  function pointerDown(e: PointerEvent, i: DesktopIcon) {
    if (e.button !== 0 || renaming === i.key) return;
    const el = e.currentTarget as HTMLElement;
    const sx = e.clientX;
    const sy = e.clientY;
    let moved = false;
    el.setPointerCapture?.(e.pointerId);
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - sx;
      const dy = ev.clientY - sy;
      if (!moved && Math.hypot(dx, dy) < DRAG_START_PX) return;
      moved = true;
      drag = { key: i.key, dx, dy };
    };
    const end = (ev: PointerEvent) => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', end);
      el.removeEventListener('pointercancel', end);
      const d = drag;
      drag = null;
      if (!moved || !d || ev.type === 'pointercancel') return;
      // The browser still sends a click after a drag; it must not select or open anything.
      justDragged = true;
      setTimeout(() => (justDragged = false), 0);
      const from = cellToPoint(placement.get(i.key)!, ICON_GRID);
      selected = i.key;
      void save(
        moveIcon(placement, i.key, { x: from.x + d.dx, y: from.y + d.dy }, desktop.area, ICON_GRID),
      );
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', end);
    el.addEventListener('pointercancel', end);
  }

  function click(i: DesktopIcon) {
    if (justDragged) return;
    const now = performance.now();
    // Select on the first click or tap, open on a second one soon after (touch included).
    if (lastClick.key === i.key && now - lastClick.at < 450) {
      lastClick = { key: '', at: 0 };
      open(i);
    } else {
      lastClick = { key: i.key, at: now };
      selected = i.key;
    }
  }

  async function focusSelected() {
    await tick();
    const all = root?.querySelectorAll<HTMLElement>('[data-key]') ?? [];
    [...all].find((el) => el.dataset.key === selected)?.focus();
  }

  function keydown(e: KeyboardEvent) {
    if (renaming) return;
    const i = ordered.findIndex((x) => x.key === selected);
    const cur = ordered[i];
    const arrows: Record<string, [number, number]> = {
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
    };
    const dir = arrows[e.key];
    if (dir && cur && (e.ctrlKey || e.metaKey)) {
      // Ctrl+arrow moves the icon one cell, for keyboard users.
      e.preventDefault();
      const c = placement.get(cur.key)!;
      const grid = gridSize(desktop.area, ICON_GRID);
      const col = Math.min(grid.cols - 1, Math.max(0, c.col + dir[0]));
      const row = Math.min(grid.rows - 1, Math.max(0, c.row + dir[1]));
      void save(
        moveIcon(placement, cur.key, cellToPoint({ col, row }, ICON_GRID), desktop.area, ICON_GRID),
      );
    } else if (dir) {
      e.preventDefault();
      selected = (cur ? neighbour(cur.key, dir) : ordered[0])?.key ?? selected;
      void focusSelected();
    } else if (cur && e.key === 'Enter') {
      e.preventDefault();
      open(cur);
    } else if (cur?.entry && e.key === 'F2') {
      e.preventDefault();
      void startRename(cur.entry);
    } else if (cur?.entry && e.key === 'Delete') {
      e.preventDefault();
      void trash(cur.entry);
    }
  }

  function neighbour(key: string, [dc, dr]: [number, number]): DesktopIcon | undefined {
    const c = placement.get(key)!;
    let best: DesktopIcon | undefined;
    let bestD = Infinity;
    for (const x of icons) {
      const p = placement.get(x.key)!;
      const along = dc ? (p.col - c.col) * dc : (p.row - c.row) * dr;
      if (along <= 0) continue;
      const across = dc ? Math.abs(p.row - c.row) : Math.abs(p.col - c.col);
      const d = along + across * 2;
      if (d < bestD) {
        best = x;
        bestD = d;
      }
    }
    return best;
  }

  async function startRename(e: FileEntry) {
    selected = e.path;
    renaming = e.path;
    renameValue = e.name;
    await tick();
    const input = root?.querySelector<HTMLInputElement>('input.rename');
    input?.focus();
    input?.setSelectionRange(0, stem(e.name).length);
  }

  async function commitRename() {
    const from = renaming;
    if (!from) return;
    renaming = null;
    const name = renameValue.trim();
    const to = childPath(DESKTOP, name);
    if (name && to !== from) {
      const r = await core.move(from, to);
      if (!r.ok) core.lastError = r.error;
      else {
        selected = to;
        await moveSavedKey(from, to);
      }
      await load();
    }
    void focusSelected();
  }

  /** A renamed file keeps its place on the desktop. */
  async function moveSavedKey(from: string, to: string) {
    const settings = core.settings;
    if (!settings?.desktop.icons.some((p) => p.key === from)) return;
    const icons = settings.desktop.icons.map((p) => (p.key === from ? { ...p, key: to } : p));
    pending = icons;
    await core.saveSettings({ ...settings, desktop: { ...settings.desktop, icons } });
  }

  async function trash(e: FileEntry) {
    const r = await core.trash([e.path]);
    if (!r.ok) core.lastError = r.error;
    await load();
  }

  async function create(kind: 'folder' | 'text') {
    const name = uniqueName(
      files.map((f) => f.name),
      kind === 'folder' ? 'New folder' : 'New text file.txt',
    );
    const path = childPath(DESKTOP, name);
    const r =
      kind === 'folder'
        ? await core.createFolder(path)
        : await core.writeFile(path, new Uint8Array());
    if (!r.ok) {
      core.lastError = r.error;
      return;
    }
    await load();
    const e = files.find((f) => f.path === path);
    if (e) void startRename(e);
  }

  function build(target: HTMLElement) {
    const key = target.closest<HTMLElement>('[data-key]')?.dataset.key;
    const i = icons.find((x) => x.key === key);
    if (i?.def) {
      selected = i.key;
      return { label: i.label, entries: [item('open', 'Open')], onSelect: () => open(i) };
    }
    if (i?.entry) {
      const e = i.entry;
      selected = i.key;
      return {
        label: i.label,
        entries: entryMenu(e),
        onSelect: (id: string) => {
          if (id === 'open') open(i);
          else if (id === 'notes') openInNotes(desktop, e.path);
          else if (id === 'rename') void startRename(e);
          else if (id === 'trash') void trash(e);
          else if (id === 'download') download(core, e.path);
          else if (id === 'properties') openProperties(desktop, e);
        },
      };
    }
    selected = null;
    return {
      label: 'Desktop',
      entries: [
        item('folder', 'New folder'),
        item('text', 'New text file'),
        separator(),
        item('line-up', 'Line up icons'),
        item('wallpaper', 'Change wallpaper…'),
        item('refresh', 'Refresh'),
      ],
      onSelect: (id: string) => {
        if (id === 'folder' || id === 'text') void create(id);
        else if (id === 'line-up')
          void save(
            placeIcons(
              icons.map((x) => x.key),
              [],
              desktop.area,
              ICON_GRID,
            ),
          );
        else if (id === 'wallpaper') desktop.open('settings');
        else if (id === 'refresh') void load();
      },
    };
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions (keys act on the selected icon) -->
<ul
  bind:this={root}
  class="icons"
  role="listbox"
  aria-label="Desktop"
  tabindex={selected ? -1 : 0}
  style:height="{desktop.area.height}px"
  use:menuTrigger={{ menu, build }}
  onpointerdown={(e) => {
    if (e.target === e.currentTarget) selected = null;
  }}
  onkeydown={keydown}
>
  {#each icons as i (i.key)}
    {@const cell = placement.get(i.key)!}
    {@const p = cellToPoint(cell, ICON_GRID)}
    {@const dragging = drag?.key === i.key}
    <li
      class="icon"
      class:dragging
      role="option"
      aria-selected={selected === i.key}
      tabindex={selected === i.key ? 0 : -1}
      data-key={i.key}
      style:left="{p.x}px"
      style:top="{p.y}px"
      style:width="{ICON_GRID.cellW}px"
      style:transform={dragging ? `translate(${drag!.dx}px, ${drag!.dy}px)` : undefined}
      onpointerdown={(e) => pointerDown(e, i)}
      onclick={() => click(i)}
      onkeydown={() => {}}
    >
      <span class="plate" class:pin={!!i.def}><Icon name={i.icon} size={28} /></span>
      {#if renaming === i.key}
        <input
          class="input rename"
          aria-label="New name for {i.label}"
          bind:value={renameValue}
          onclick={(e) => e.stopPropagation()}
          onpointerdown={(e) => e.stopPropagation()}
          onkeydown={(e) => {
            e.stopPropagation();
            if (e.key === 'Enter') void commitRename();
            else if (e.key === 'Escape') {
              renaming = null;
              void focusSelected();
            }
          }}
          onblur={commitRename}
          spellcheck="false"
          autocomplete="off"
        />
      {:else}
        <span class="label">{i.label}</span>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .icons {
    position: absolute;
    inset: 0 0 auto 0;
    margin: 0;
    padding: 0;
    list-style: none;
    outline: none;
  }
  .icon {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 6px 4px;
    border-radius: var(--radius-md);
    touch-action: none;
    cursor: default;
    outline: none;
  }
  .icon.dragging {
    z-index: 1;
    opacity: 0.85;
  }
  .plate {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border-radius: 13px;
    background: var(--surface);
    color: var(--text);
    box-shadow:
      0 0 0 1px oklch(0.3 0.01 250 / 0.14),
      0 2px 6px -2px oklch(0.2 0.01 250 / 0.3);
  }
  .plate.pin {
    color: var(--accent-ink);
  }
  .label {
    max-width: 100%;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
    font-weight: 600;
    line-height: 1.3;
    text-align: center;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    color: var(--wall-ink);
    text-shadow: var(--wall-ink-shadow);
  }
  .icon[aria-selected='true'] .label {
    background: var(--accent);
    color: var(--accent-text);
    text-shadow: none;
  }
  .icon[aria-selected='true'] .plate {
    box-shadow:
      0 0 0 2px var(--accent),
      0 2px 6px -2px oklch(0.2 0.01 250 / 0.3);
  }
  .icon:focus-visible .plate {
    outline: 2px solid var(--focus-ring);
    outline-offset: 3px;
  }
  .rename {
    width: 100%;
    min-height: 30px;
    padding: 0 4px;
    font-size: var(--text-sm);
    text-align: center;
  }
</style>
