import { t } from '../lib/i18n';
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
    desktop.open('notes', {
      key: path,
      title: t('files.windowTitle.notes', { name: baseName(path) }),
      params: { path },
    });
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
  desktop.open('properties', {
    key: entry.path,
    title: t('files.windowTitle.properties', { name: entry.name }),
    params,
  });
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
  return baseName(path)
    ? t('files.windowTitle.folder', { name: baseName(path) })
    : t('files.windowTitle.plain');
}

/** The top of an entry's right-click menu: its picture, name, size and folder. */
export function entryHeader(entry: FileEntry): MenuHeader {
  const folder = baseName(parentPath(entry.path) ?? '') || t('files.home');
  const detail =
    entry.kind === 'file'
      ? t('files.menu.header.file', { size: formatBytes(entry.size), folder })
      : t('files.menu.header.folder', { folder });
  return { title: entry.name, detail, icon: fileLook(entry).type, file: true };
}

/**
 * The right-click menu of one file or folder (also used on desktop icons for files). Disks have
 * no Trash, so there it is "Delete for good…", which asks first.
 */
export function entryMenu(entry: FileEntry): (MenuEntry | false)[] {
  if (isInTrash(entry.path)) {
    return [
      item('restore', t('files.menu.restore'), { icon: 'undo' }),
      item('properties', t('files.menu.properties'), { icon: 'info', shortcut: 'Alt+Enter' }),
      separator(),
      item('delete', t('files.menu.deleteForGood'), {
        danger: true,
        shortcut: 'Delete',
        icon: 'trash',
      }),
    ];
  }
  const fixed = entry.special ? t('files.menu.fixedFolder', { name: entry.name }) : undefined;
  const isFile = entry.kind === 'file';
  const replay = replayPortFor(entry.path);
  const disk = isOnDisk(entry.path);
  return [
    item('open', t('files.menu.open'), { icon: 'open', shortcut: 'Enter' }),
    isFile && !replay && item('notes', t('files.menu.openWithNotes'), { icon: 'notes' }),
    separator(),
    item('rename', t('files.menu.rename'), {
      shortcut: 'F2',
      disabledReason: fixed,
      icon: 'rename',
    }),
    isFile && item('download', t('files.menu.download'), { icon: 'download' }),
    item('properties', t('files.menu.properties'), { icon: 'info', shortcut: 'Alt+Enter' }),
    separator(),
    disk
      ? item('delete', t('files.menu.deleteForGoodAsk'), {
          danger: true,
          shortcut: 'Delete',
          icon: 'trash',
        })
      : item('trash', t('files.menu.moveToTrash'), {
          shortcut: 'Delete',
          disabledReason: fixed,
          danger: true,
          icon: 'trash',
        }),
  ];
}
