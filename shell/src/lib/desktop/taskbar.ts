/**
 * What the taskbar shows, as plain data. Pinned programs always have a button, in the order they
 * were pinned, marked when they have windows open. Windows of programs that are not pinned get a
 * button each after them, in the order they were opened.
 *
 * Programs are named by their key (`tool:<id>`, `accessory:<id>`, or `app:<id>` for an installed
 * app's window; docs/api.md), the same keys Settings.taskbar.pinned stores.
 */

export interface Program {
  /** `tool:parts`, `accessory:files`, `app:wall-follower` … */
  key: string;
  /** The window kind it opens. */
  kind: string;
  /**
   * Only the windows of its kind with this key are its own: every app's window is of kind `app`,
   * keyed by the app's id.
   */
  windowKey?: string;
}

export interface TaskWindow {
  id: string;
  kind: string;
  /** What the window shows (an app's id for an app window). */
  key?: string;
  minimised: boolean;
}

export type TaskItem =
  | {
      type: 'program';
      key: string;
      kind: string;
      /** Its open windows, in the order they were opened. */
      windows: string[];
      /** One of its windows is in front. */
      active: boolean;
    }
  | { type: 'window'; id: string; kind: string; active: boolean };

export function taskbarItems(
  pinned: readonly string[],
  programs: readonly Program[],
  windows: readonly TaskWindow[],
  focusedId: string | null,
): TaskItem[] {
  const byKey = new Map(programs.map((p) => [p.key, p]));
  const isActive = (w: TaskWindow) => w.id === focusedId && !w.minimised;
  const items: TaskItem[] = [];
  const claimed = new Set<string>();
  for (const key of new Set(pinned)) {
    const p = byKey.get(key);
    if (!p) continue; // A program this shell doesn't have (an older or newer Faber, or an app gone).
    const own = windows.filter(
      (w) => w.kind === p.kind && (p.windowKey === undefined || w.key === p.windowKey),
    );
    for (const w of own) claimed.add(w.id);
    items.push({
      type: 'program',
      key,
      kind: p.kind,
      windows: own.map((w) => w.id),
      active: own.some(isActive),
    });
  }
  for (const w of windows) {
    if (claimed.has(w.id)) continue;
    items.push({ type: 'window', id: w.id, kind: w.kind, active: isActive(w) });
  }
  return items;
}

/** `open` names the program (its key) as well as its window kind: an app opens its own window. */
export type TaskClick = { do: 'open'; kind: string; key: string } | { do: 'activate'; id: string };

/**
 * A click on a taskbar button: a pinned program with nothing open starts it; one with a window
 * brings it forward (or minimises it when it already is in front); with several windows, each
 * click moves to the next of them.
 */
export function taskClick(item: TaskItem, focusedId: string | null): TaskClick {
  if (item.type === 'window') return { do: 'activate', id: item.id };
  const ws = item.windows;
  if (ws.length === 0) return { do: 'open', kind: item.kind, key: item.key };
  if (ws.length === 1) return { do: 'activate', id: ws[0]! };
  const at = focusedId === null ? -1 : ws.indexOf(focusedId);
  if (at === -1 || !item.active) return { do: 'activate', id: ws.at(-1)! };
  return { do: 'activate', id: ws[(at + 1) % ws.length]! };
}

export function isPinned(pinned: readonly string[], key: string): boolean {
  return pinned.includes(key);
}

/** Pinning adds at the end; pinning twice changes nothing. */
export function pin(pinned: readonly string[], key: string): string[] {
  return pinned.includes(key) ? [...pinned] : [...pinned, key];
}

export function unpin(pinned: readonly string[], key: string): string[] {
  return pinned.filter((k) => k !== key);
}

export function togglePin(pinned: readonly string[], key: string): string[] {
  return isPinned(pinned, key) ? unpin(pinned, key) : pin(pinned, key);
}

/** Taskbar heights in px: Small (icons only) and Large (bigger, with labels, for touch). */
export const TASKBAR_HEIGHTS = { small: 52, large: 64 } as const;

export type TaskbarSize = keyof typeof TASKBAR_HEIGHTS;

export function taskbarHeight(size: string | undefined): number {
  return size === 'large' ? TASKBAR_HEIGHTS.large : TASKBAR_HEIGHTS.small;
}
