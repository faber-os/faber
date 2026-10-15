import { splitPath } from './filePaths';

/**
 * Removable disks (USB sticks, SD cards) are file roots like `disk-e:/` (docs/api.md, "Files").
 * They have no Trash: deleting there is for good.
 */

export interface RootLike {
  id: string;
  label: string;
  path: string;
}

export function isDiskRoot(id: string): boolean {
  return id.startsWith('disk-');
}

/** The connected disks, in the order the core lists them. */
export function diskRoots<T extends RootLike>(roots: readonly T[]): T[] {
  return roots.filter((r) => isDiskRoot(r.id));
}

export function isOnDisk(path: string): boolean {
  return isDiskRoot(splitPath(path).root);
}

/**
 * A `files-changed` that names a root other than `home:/` (a disk plugged in or pulled out):
 * the list of roots must be fetched again.
 */
export function rootsChanged(folders: readonly string[]): boolean {
  return folders.some((f) => /^[a-z0-9-]+:\/$/.test(f) && f !== 'home:/');
}
