<script lang="ts">
  import { tick } from 'svelte';
  import { useCore } from '../lib/state/context';
  import { t } from '../lib/i18n';
  import type { DesktopLayout, FileEntry } from '../lib/state/types';
  import { onDesktop, removeFromDesktop } from '../lib/desktop/desktopPrograms';
  import { item, separator } from '../lib/desktop/contextMenu';
  import { DESKTOP, childPath, isAffected, stem, uniqueName } from '../lib/desktop/filePaths';
  import {
    ICON_GRID,
    cellToPoint,
    gridSize,
    iconsInRect,
    moveIcon,
    moveIcons,
    placeIcons,
    positionsForSettings,
    type Placement,
    type Rect,
  } from '../lib/desktop/iconGrid';
  import { clickSelect } from '../lib/desktop/selection';
  import { useDesktop } from './desktopController.svelte';
  import AppIcon from './AppIcon.svelte';
  import {
    download,
    entryHeader,
    entryMenu,
    openEntry,
    openInNotes,
    openProperties,
  } from './fileOps';
  import FileIcon from './FileIcon.svelte';
  import { isPackageFile } from '../lib/desktop/packages';
  import { openDroppedPackage } from './installWindow';
  import { fileLook } from './fileLook';
  import { menuTrigger, useMenu } from './menuController.svelte';
  import { desktopItem, pinItem, runDesktopItem, runPinItem } from './programs';
  import type { WindowDef } from './windowDefs';

  let { defs }: { defs: WindowDef[] } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const menu = useMenu();
  const DRAG_START_PX = 5;

  interface DesktopIcon {
    key: string;
    label: string;
    def?: WindowDef;
    entry?: FileEntry;
  }

  let files = $state.raw<FileEntry[]>([]);
  /** The icon keyboard keys act from (and the anchor of a Shift+click). */
  let selected = $state<string | null>(null);
  /** Every selected icon: Ctrl/Shift+click and the rubber band select several. */
  let picked = $state.raw<string[]>([]);
  /** The rubber band being drawn, in pixels from the desktop's top-left. */
  let band = $state.raw<Rect | null>(null);
  let renaming = $state<string | null>(null);
  let renameValue = $state('');
  /** The layout just saved, shown until the core's answer arrives. */
  let pending = $state.raw<DesktopLayout | null>(null);
  let drag = $state<{ keys: string[]; dx: number; dy: number } | null>(null);
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

  const layout = $derived<DesktopLayout>(
    pending ?? core.settings?.desktop ?? { icons: [], removed: [] },
  );
  // Every program, unless the user removed it: one added in a later version shows up by itself.
  const shown = $derived(
    new Set(
      onDesktop(
        defs.flatMap((d) => (d.program ? [d.program] : [])),
        layout,
      ),
    ),
  );
  const icons = $derived<DesktopIcon[]>([
    ...defs.flatMap((d) =>
      d.program && shown.has(d.program) ? [{ key: d.program, label: d.title, def: d }] : [],
    ),
    ...files.map((e) => ({ key: e.path, label: e.name, entry: e })),
  ]);
  const saved = $derived(layout.icons);
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
    pending = { ...layout, icons };
    if (!settings) return;
    const r = await core.saveSettings({ ...settings, desktop: { ...layout, icons } });
    if (!r.ok) {
      pending = null;
      core.lastError = r.error;
    }
  }

  /** Just this icon selected (or none). */
  function selectOnly(key: string | null) {
    selected = key;
    picked = key === null ? [] : [key];
  }

  /** The selected icons, in keyboard order. */
  function pickedIcons(): DesktopIcon[] {
    return ordered.filter((i) => picked.includes(i.key));
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
    // Dragging a selected icon moves every selected icon with it.
    const group = picked.includes(i.key) ? picked : [i.key];
    el.setPointerCapture?.(e.pointerId);
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - sx;
      const dy = ev.clientY - sy;
      if (!moved && Math.hypot(dx, dy) < DRAG_START_PX) return;
      moved = true;
      drag = { keys: group, dx, dy };
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
      if (d.keys.length > 1) {
        selected = i.key;
        void save(moveIcons(placement, d.keys, { dx: d.dx, dy: d.dy }, desktop.area, ICON_GRID));
        return;
      }
      const from = cellToPoint(placement.get(i.key)!, ICON_GRID);
      selectOnly(i.key);
      void save(
        moveIcon(placement, i.key, { x: from.x + d.dx, y: from.y + d.dy }, desktop.area, ICON_GRID),
      );
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', end);
    el.addEventListener('pointercancel', end);
  }

  function click(e: MouseEvent, i: DesktopIcon) {
    if (justDragged) return;
    const ctrl = e.ctrlKey || e.metaKey;
    if (ctrl || e.shiftKey) {
      const next = clickSelect(
        { keys: picked, anchor: selected },
        i.key,
        { ctrl, shift: e.shiftKey },
        ordered.map((x) => x.key),
      );
      picked = next.keys;
      selected = next.anchor;
      lastClick = { key: '', at: 0 };
      return;
    }
    const now = performance.now();
    // Select on the first click or tap, open on a second one soon after (touch included).
    if (lastClick.key === i.key && now - lastClick.at < 450) {
      lastClick = { key: '', at: 0 };
      open(i);
    } else {
      lastClick = { key: i.key, at: now };
      selectOnly(i.key);
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
      selectOnly((cur ? neighbour(cur.key, dir) : ordered[0])?.key ?? selected);
      void focusSelected();
    } else if (cur?.entry && e.key === 'Enter' && e.altKey) {
      e.preventDefault();
      openProperties(desktop, cur.entry);
    } else if (cur && e.key === 'Enter') {
      e.preventDefault();
      const all = pickedIcons();
      for (const x of all.length > 1 ? all : [cur]) open(x);
    } else if (cur?.entry && e.key === 'F2') {
      e.preventDefault();
      void startRename(cur.entry);
    } else if (cur && e.key === 'Delete') {
      // Programs stay: only files go to the Trash.
      const entries = pickedIcons().flatMap((x) => (x.entry ? [x.entry] : []));
      if (entries.length === 0) return;
      e.preventDefault();
      void trashAll(entries);
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
    selectOnly(e.path);
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
        selectOnly(to);
        desktop.renameRecent(from, to);
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
    pending = { ...settings.desktop, icons };
    await core.saveSettings({ ...settings, desktop: { ...settings.desktop, icons } });
  }

  async function trash(e: FileEntry) {
    // Move to Trash on one of several selected files moves all of them.
    const all = pickedIcons().flatMap((x) => (x.entry ? [x.entry] : []));
    await trashAll(all.length > 1 && all.some((x) => x.path === e.path) ? all : [e]);
  }

  async function trashAll(entries: FileEntry[]) {
    const r = await core.trash(entries.map((x) => x.path));
    if (!r.ok) core.lastError = r.error;
    selectOnly(null);
    await load();
  }

  /** Where a pointer event is, in pixels from the desktop's top-left. */
  function local(e: PointerEvent): { x: number; y: number } {
    const box = root?.getBoundingClientRect();
    return { x: e.clientX - (box?.left ?? 0), y: e.clientY - (box?.top ?? 0) };
  }

  /** A rubber band from an empty spot selects the icons it is drawn over (Ctrl adds to them). */
  function bandStart(e: PointerEvent) {
    if (e.target !== e.currentTarget || e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    const start = local(e);
    const base = e.ctrlKey || e.metaKey || e.shiftKey ? picked : [];
    if (base.length === 0) selectOnly(null);
    el.setPointerCapture?.(e.pointerId);
    const move = (ev: PointerEvent) => {
      const p = local(ev);
      const rect = { x: start.x, y: start.y, w: p.x - start.x, h: p.y - start.y };
      if (!band && Math.hypot(rect.w, rect.h) < DRAG_START_PX) return;
      band = rect;
      const inside = iconsInRect(placement, rect, ICON_GRID);
      picked = [...base, ...inside.filter((k) => !base.includes(k))];
      selected = picked[0] ?? null;
    };
    const end = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', end);
      el.removeEventListener('pointercancel', end);
      band = null;
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', end);
    el.addEventListener('pointercancel', end);
  }

  async function create(kind: 'folder' | 'text') {
    const name = uniqueName(
      files.map((f) => f.name),
      kind === 'folder' ? t('desktop.newFolder.name') : t('desktop.newTextFile.name'),
    );
    const path = childPath(DESKTOP, name);
    const r =
      kind === 'folder'
        ? await core.createFolder(path)
        : await core.writeFile(path, new Uint8Array(), 'create');
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
    // Right-clicking an icon outside the selection selects just it.
    if (i && !picked.includes(i.key)) selectOnly(i.key);
    if (i?.def) {
      const d = i.def;
      return {
        label: i.label,
        header: { title: d.title, detail: d.description, icon: d.icon },
        entries: [
          item('open', t('menu.open'), { icon: 'open', shortcut: 'Enter' }),
          pinItem(core, i.key),
          desktopItem(core, i.key),
        ],
        onSelect: (id: string) => {
          if (id === 'desktop-remove') {
            pending = removeFromDesktop(layout, i.key);
            selectOnly(null);
          }
          if (!runPinItem(core, i.key, id) && !runDesktopItem(core, i.key, id)) open(i);
        },
      };
    }
    if (i?.entry) {
      const e = i.entry;
      return {
        label: i.label,
        header: entryHeader(e),
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
    selectOnly(null);
    return {
      label: t('desktop.label'),
      entries: [
        item('folder', t('desktop.menu.newFolder'), { icon: 'folder' }),
        item('text', t('desktop.menu.newTextFile'), { icon: 'fileText' }),
        separator(),
        item('line-up', t('desktop.menu.lineUp'), { icon: 'grid' }),
        item('wallpaper', t('desktop.menu.wallpaper'), { icon: 'desktop' }),
        item('refresh', t('desktop.menu.refresh'), { icon: 'refresh' }),
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

  /** Files dragged in from this computer: connector packages dropped here open Install. */
  function dragOver(e: DragEvent) {
    if (!e.dataTransfer?.types.includes('Files')) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  }

  function drop(e: DragEvent) {
    if (!e.dataTransfer?.types.includes('Files')) return;
    e.preventDefault();
    for (const f of Array.from(e.dataTransfer.files)) {
      if (isPackageFile(f.name)) openDroppedPackage(desktop, f);
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions (keys act on the selected icon; drops install packages) -->
<ul
  bind:this={root}
  class="icons"
  role="listbox"
  aria-label={t('desktop.label')}
  tabindex={selected ? -1 : 0}
  style:height="{desktop.area.height}px"
  use:menuTrigger={{ menu, build }}
  aria-multiselectable="true"
  onpointerdown={bandStart}
  onkeydown={keydown}
  ondragover={dragOver}
  ondrop={drop}
>
  {#each icons as i (i.key)}
    {@const cell = placement.get(i.key)!}
    {@const p = cellToPoint(cell, ICON_GRID)}
    {@const dragging = drag?.keys.includes(i.key) ?? false}
    <li
      class="icon"
      class:dragging
      role="option"
      aria-selected={picked.includes(i.key)}
      tabindex={selected === i.key ? 0 : -1}
      data-key={i.key}
      style:left="{p.x}px"
      style:top="{p.y}px"
      style:width="{ICON_GRID.cellW}px"
      style:transform={dragging ? `translate(${drag!.dx}px, ${drag!.dy}px)` : undefined}
      onpointerdown={(e) => pointerDown(e, i)}
      onclick={(e) => click(e, i)}
      onkeydown={() => {}}
    >
      {#if i.def}
        <span class="plate"><AppIcon icon={i.def.icon} colour={i.def.colour} size={52} /></span>
      {:else if i.entry}
        {@const look = fileLook(i.entry)}
        <span class="plate file"><FileIcon type={look.type} emblem={look.emblem} size={52} /></span>
      {/if}
      {#if renaming === i.key}
        <input
          class="input rename"
          aria-label={t('desktop.rename.label', { name: i.label })}
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
  {#if band}
    <li
      class="band"
      role="presentation"
      data-testid="rubber-band"
      style:left="{Math.min(band.x, band.x + band.w)}px"
      style:top="{Math.min(band.y, band.y + band.h)}px"
      style:width="{Math.abs(band.w)}px"
      style:height="{Math.abs(band.h)}px"
    ></li>
  {/if}
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
    border-radius: 14px;
  }
  .plate.file {
    filter: drop-shadow(0 1px 2px oklch(0.2 0.01 250 / 0.3));
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
    box-shadow: 0 0 0 2px var(--accent);
  }
  .icon:focus-visible .plate {
    outline: 2px solid var(--focus-ring);
    outline-offset: 3px;
  }
  .band {
    position: absolute;
    border: 1px solid var(--accent);
    border-radius: 2px;
    background: var(--selected);
    pointer-events: none;
  }
  .rename {
    width: 100%;
    min-height: 30px;
    padding: 0 4px;
    font-size: var(--text-sm);
    text-align: center;
  }
</style>
