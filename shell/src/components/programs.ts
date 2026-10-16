import { item, type MenuItem } from '../lib/desktop/contextMenu';
import { t } from '../lib/i18n';
import { isPinned, pin, unpin, type Program } from '../lib/desktop/taskbar';
import { addToDesktop, removeFromDesktop } from '../lib/desktop/desktopPrograms';
import type { CoreState } from '../lib/state/core.svelte';
import type { AppInfo, Settings, TaskbarSize } from '../lib/state/types';
import type { WindowDef } from './windowDefs';

/** The programs (tools and accessories that have a program key), as the taskbar knows them. */
export function programsOf(defs: readonly WindowDef[]): Program[] {
  return defs.flatMap((d) => (d.program ? [{ key: d.program, kind: d.kind }] : []));
}

/** An app's program key: `app:<id>`, as Settings.taskbar.pinned stores it. */
export function appProgram(id: string): string {
  return `app:${id}`;
}

/** The installed apps with a window, as programs: each its own `app` window, keyed by its id. */
export function appPrograms(apps: readonly AppInfo[] | null): Program[] {
  return (apps ?? []).flatMap((a) =>
    a.ui ? [{ key: appProgram(a.id), kind: 'app', windowKey: a.id }] : [],
  );
}

/** The app a program key names, if it is an installed app's. */
export function appOf(core: CoreState, key: string): AppInfo | null {
  if (!key.startsWith('app:')) return null;
  return core.apps?.find((a) => appProgram(a.id) === key) ?? null;
}

export function pinnedKeys(core: CoreState): string[] {
  return core.settings?.taskbar.pinned ?? [];
}

/** The Pin to taskbar / Unpin from taskbar item for a program's menus. */
export function pinItem(core: CoreState, key: string): MenuItem {
  return isPinned(pinnedKeys(core), key)
    ? item('unpin', t('programs.menu.unpin'), { icon: 'unpin' })
    : item('pin', t('programs.menu.pin'), { icon: 'pin' });
}

async function saveTaskbar(
  core: CoreState,
  patch: (t: Settings['taskbar']) => Settings['taskbar'],
) {
  const s = core.settings;
  if (!s) return;
  const r = await core.saveSettings({ ...s, taskbar: patch(s.taskbar) });
  if (!r.ok) core.lastError = r.error;
}

/** Pin or unpin, saved in the core's settings so every screen shows the same taskbar. */
export function setPinned(core: CoreState, key: string, on: boolean): Promise<void> {
  return saveTaskbar(core, (t) => ({
    ...t,
    pinned: on ? pin(t.pinned, key) : unpin(t.pinned, key),
  }));
}

export function setTaskbarSize(core: CoreState, size: TaskbarSize): Promise<void> {
  return saveTaskbar(core, (t) => ({ ...t, size }));
}

/** Whether a program has an icon on the desktop (every program has, unless it was removed). */
export function isOnDesktop(core: CoreState, key: string): boolean {
  return !(core.settings?.desktop.removed ?? []).includes(key);
}

/** The Remove from desktop / Add to desktop item for a program's menus. */
export function desktopItem(core: CoreState, key: string): MenuItem {
  return isOnDesktop(core, key)
    ? item('desktop-remove', t('programs.menu.removeFromDesktop'), { icon: 'desktop' })
    : item('desktop-add', t('programs.menu.addToDesktop'), { icon: 'desktop' });
}

/** Removes or adds back a program's desktop icon, for every screen. */
export async function setOnDesktop(core: CoreState, key: string, on: boolean): Promise<void> {
  const s = core.settings;
  if (!s) return;
  const desktop = on ? addToDesktop(s.desktop, key) : removeFromDesktop(s.desktop, key);
  const r = await core.saveSettings({ ...s, desktop });
  if (!r.ok) core.lastError = r.error;
}

/** The item ids `desktopItem` uses, run against the core. */
export function runDesktopItem(core: CoreState, key: string, id: string): boolean {
  if (id === 'desktop-remove') void setOnDesktop(core, key, false);
  else if (id === 'desktop-add') void setOnDesktop(core, key, true);
  else return false;
  return true;
}

/** The item ids `pinItem` uses, run against the core. */
export function runPinItem(core: CoreState, key: string, id: string): boolean {
  if (id === 'pin') void setPinned(core, key, true);
  else if (id === 'unpin') void setPinned(core, key, false);
  else return false;
  return true;
}
