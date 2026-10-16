import { t } from '../lib/i18n';
import { sketchOf } from '../lib/desktop/arduino';
import { baseName } from '../lib/desktop/filePaths';
import type { DesktopController } from './desktopController.svelte';

/**
 * Opens a sketch file in the Arduino editor: the window that already has its sketch, else a new
 * one. Each sketch gets its own window, keyed by its folder.
 */
export function openInArduino(desktop: DesktopController, path: string): string {
  desktop.noteRecent(path);
  const s = sketchOf(path);
  const id = desktop.open('arduino', {
    key: s ? `arduino:${s.folder}` : `arduino:${path}`,
    title: t('files.windowTitle.arduino', { name: s?.name ?? baseName(path) }),
    params: { path },
  });
  // A window that was already open picks this up and shows the file's tab.
  desktop.update(id, { params: { path } });
  desktop.focus(id);
  return id;
}
