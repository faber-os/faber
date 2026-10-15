import { t } from '../lib/i18n';
import { baseName } from '../lib/desktop/filePaths';
import type { DesktopController } from './desktopController.svelte';

/**
 * Opening the Install window for a connector package (docs/packages.md §1): one on the machine
 * (double-clicked in Files or on the desktop), or one dropped from this computer, which is
 * installed straight from the browser without being saved in Files first.
 */

/** Dropped files, by token, until their window closes. Window params can only hold text. */
const dropped = new Map<string, Blob>();
let next = 0;

/** Opens the Install window for a package on the machine. */
export function openPackage(desktop: DesktopController, path: string): void {
  const name = baseName(path);
  desktop.open('install-package', {
    key: `install:${path}`,
    title: t('install.windowTitle', { name }),
    params: { path, name },
  });
}

/** Opens the Install window for a package dropped from this computer. */
export function openDroppedPackage(desktop: DesktopController, file: File): void {
  const token = `drop-${++next}`;
  dropped.set(token, file);
  desktop.open('install-package', {
    key: `install:${token}`,
    title: t('install.windowTitle', { name: file.name }),
    params: { dropped: token, name: file.name },
  });
}

/** The dropped file behind a token; gone after a reload of the page. */
export function droppedPackage(token: string): Blob | undefined {
  return dropped.get(token);
}

export function forgetDropped(token: string): void {
  dropped.delete(token);
}
