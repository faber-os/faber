import { item, separator, type MenuEntry } from '../lib/desktop/contextMenu';
import { baseName, extension, isInTrash, replayPortFor } from '../lib/desktop/filePaths';
import type { CoreState } from '../lib/state/core.svelte';
import type { FileEntry } from '../lib/state/types';
import type { DesktopController } from './desktopController.svelte';

/**
 * What files and folders do when opened, the same on the desktop and in Files: folders open in
 * Files, recordings explain themselves, everything else opens in Notes (which says clearly when a
 * file isn't text).
 */
export function openEntry(desktop: DesktopController, entry: FileEntry): void {
  if (entry.kind === 'folder') openFolder(desktop, entry.path);
  else if (extension(entry.name) === 'fbcap') openProperties(desktop, entry);
  else openInNotes(desktop, entry.path);
}

export function openFolder(desktop: DesktopController, path: string): void {
  desktop.open('files', { key: `files:${path}`, title: titleFor(path), params: { path } });
}

export function openInNotes(desktop: DesktopController, path: string): void {
  desktop.open('notes', { key: path, title: `${baseName(path)} · Notes`, params: { path } });
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

/** The right-click menu of one file or folder (also used on desktop icons for files). */
export function entryMenu(entry: FileEntry): (MenuEntry | false)[] {
  if (isInTrash(entry.path)) {
    return [
      item('restore', 'Restore'),
      item('delete', 'Delete for good', { danger: true, shortcut: 'Delete' }),
      separator(),
      item('properties', 'Properties'),
    ];
  }
  const fixed = entry.special
    ? `${entry.name} is one of Faber's fixed folders, so it can't be renamed or moved.`
    : undefined;
  const isFile = entry.kind === 'file';
  const replay = replayPortFor(entry.path);
  return [
    item('open', 'Open'),
    isFile && !replay && item('notes', 'Open with Notes'),
    separator(),
    item('rename', 'Rename', { shortcut: 'F2', disabledReason: fixed }),
    item('trash', 'Move to Trash', { shortcut: 'Delete', disabledReason: fixed }),
    separator(),
    isFile && item('download', 'Download'),
    item('properties', 'Properties'),
  ];
}
