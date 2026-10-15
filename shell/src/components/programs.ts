import { item, type MenuItem } from '../lib/desktop/contextMenu';
import { t } from '../lib/i18n';
import { isPinned, pin, unpin, type Program } from '../lib/desktop/taskbar';
import type { CoreState } from '../lib/state/core.svelte';
import type { Settings, TaskbarSize } from '../lib/state/types';
import type { WindowDef } from './windowDefs';

/** The programs (tools and accessories that have a program key), as the taskbar knows them. */
export function programsOf(defs: readonly WindowDef[]): Program[] {
  return defs.flatMap((d) => (d.program ? [{ key: d.program, kind: d.kind }] : []));
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

/** The item ids `pinItem` uses, run against the core. */
export function runPinItem(core: CoreState, key: string, id: string): boolean {
  if (id === 'pin') void setPinned(core, key, true);
  else if (id === 'unpin') void setPinned(core, key, false);
  else return false;
  return true;
}
