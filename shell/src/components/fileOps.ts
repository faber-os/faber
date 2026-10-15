import { item, separator, type MenuEntry, type MenuHeader } from '../lib/desktop/contextMenu';
import { isOnDisk } from '../lib/desktop/disks';
import {
  baseName,
  extension,
  isInTrash,
  parentPath,
  replayPortFor,
} from '../lib/desktop/filePaths';
import { tabTarget } from '../lib/desktop/tabs';
import type { CoreState } from '../lib/state/core.svelte';
import type { FileEntry } from '../lib/state/types';
import type { DesktopController } from './desktopController.svelte';
import { fileLook } from './fileLook';
import { formatBytes } from './format';

/**
 * What files and folders do when opened, the same on the desktop and in Files: folders open in
 * Files, recordings explain themselves, everything else opens in Notes (which says clearly when a
 * file isn't text).
 */
export function openEntry(desktop: DesktopController, entry: FileEntry): void {
  if (entry.kind === 'folder') openFolder(desktop, entry.path);
  else if (extension(entry.name) === 'fbcap') {
    desktop.noteRecent(entry.path);
    openProperties(desktop, entry);
  } else openInNotes(desktop, entry.path);
}

export function openFolder(desktop: DesktopController, path: string): void {
  desktop.open('files', { key: `files:${path}`, title: titleFor(path), params: { path } });
}

/**
 * Open a file in Notes: the tab that already has it, else a new tab in the frontmost Notes
 * window, else a new Notes window.
 */
export function openInNotes(desktop: DesktopController, path: string): void {
  desktop.noteRecent(path);
  const target = tabTarget(desktop.state.windows, 'notes', (id) => desktop.tabs[id]?.state, path);
  if (!target) {
    desktop.open('notes', { key: path, title: `${baseName(path)} · Notes`, params: { path } });
    return;
  }
  desktop.focus(target.windowId);
  if (target.tabId) desktop.tabs[target.windowId]?.activate(target.tabId);
  // The window picks this up (NotesWindow) and opens the file in a new tab.
  else desktop.update(target.windowId, { params: { open: path } });
}

export function openProperties(desktop: DesktopController, entry: FileEntry): void {
  const params: Record<string, string> = {
    name: entry.name,
    path: entry.path,
    kind: entry.kind,
    size: String(entry.size),
  };
  if (entry.modifiedMs !== undefined) params.modifiedMs = String(entry.modifiedMs);
  if (entry.originalPath) params.originalPath = entry.originalPath;
  if (entry.special) params.special = entry.special;
  desktop.open('properties', { key: entry.path, title: `${entry.name} · Properties`, params });
}

/** Let the browser save the file, from the core's download address. */
export function download(core: CoreState, path: string): void {
  const a = document.createElement('a');
  a.href = core.downloadUrl(path);
  a.download = baseName(path);
  a.rel = 'noopener';
  document.body.append(a);
  a.click();
  a.remove();
}

function titleFor(path: string): string {
  return baseName(path) ? `${baseName(path)} · Files` : 'Files';
}

/** The top of an entry's right-click menu: its picture, name, size and folder. */
export function entryHeader(entry: FileEntry): MenuHeader {
  const folder = baseName(parentPath(entry.path) ?? '') || 'Home';
  const detail =
    entry.kind === 'file' ? `${formatBytes(entry.size)} · ${folder}` : `Folder · ${folder}`;
  return { title: entry.name, detail, icon: fileLook(entry).type, file: true };
}

/**
 * The right-click menu of one file or folder (also used on desktop icons for files). Disks have
 * no Trash, so there it is "Delete for good…", which asks first.
 */
export function entryMenu(entry: FileEntry): (MenuEntry | false)[] {
  if (isInTrash(entry.path)) {
    return [
      item('restore', 'Restore', { icon: 'undo' }),
      item('properties', 'Properties', { icon: 'info', shortcut: 'Alt+Enter' }),
      separator(),
      item('delete', 'Delete for good', { danger: true, shortcut: 'Delete', icon: 'trash' }),
    ];
  }
  const fixed = entry.special
    ? `${entry.name} is one of Faber's fixed folders, so it can't be renamed or moved.`
    : undefined;
  const isFile = entry.kind === 'file';
  const replay = replayPortFor(entry.path);
  const disk = isOnDisk(entry.path);
  return [
    item('open', 'Open', { icon: 'open', shortcut: 'Enter' }),
    isFile && !replay && item('notes', 'Open with Notes', { icon: 'notes' }),
    separator(),
    item('rename', 'Rename', { shortcut: 'F2', disabledReason: fixed, icon: 'rename' }),
    isFile && item('download', 'Download', { icon: 'download' }),
    item('properties', 'Properties', { icon: 'info', shortcut: 'Alt+Enter' }),
    separator(),
    disk
      ? item('delete', 'Delete for good…', { danger: true, shortcut: 'Delete', icon: 'trash' })
      : item('trash', 'Move to Trash', {
          shortcut: 'Delete',
          disabledReason: fixed,
          danger: true,
          icon: 'trash',
        }),
  ];
}
