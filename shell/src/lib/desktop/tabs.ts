/**
 * Tabs in a window's title bar (Notes, Files): plain data in, plain data out. A tab has an id
 * that never changes, a title, and a `key` saying what it shows (a file path for Notes, a folder
 * for Files) so opening the same thing twice goes to the tab that already has it.
 */

export interface Tab {
  id: string;
  title: string;
  key: string;
  /** Unsaved changes (Notes): drawn as a dot, and closing asks first. */
  dirty: boolean;
}

export interface TabsState {
  tabs: Tab[];
  activeId: string | null;
  nextId: number;
}

export function emptyTabs(): TabsState {
  return { tabs: [], activeId: null, nextId: 1 };
}

/** A new tab right after the active one (or at the end), made active. */
export function addTab(
  state: TabsState,
  tab: { title: string; key: string; dirty?: boolean },
): { state: TabsState; id: string } {
  const id = `t${state.nextId}`;
  const at = state.tabs.findIndex((t) => t.id === state.activeId);
  const tabs = [...state.tabs];
  tabs.splice(at === -1 ? tabs.length : at + 1, 0, {
    id,
    title: tab.title,
    key: tab.key,
    dirty: tab.dirty ?? false,
  });
  return { state: { tabs, activeId: id, nextId: state.nextId + 1 }, id };
}

/** Close a tab. If it was active, the one to its right takes over (or to its left at the end). */
export function closeTab(state: TabsState, id: string): TabsState {
  const at = state.tabs.findIndex((t) => t.id === id);
  if (at === -1) return state;
  const tabs = state.tabs.filter((t) => t.id !== id);
  let activeId = state.activeId;
  if (activeId === id) activeId = (tabs[at] ?? tabs[at - 1])?.id ?? null;
  return { ...state, tabs, activeId };
}

export function activateTab(state: TabsState, id: string): TabsState {
  if (!state.tabs.some((t) => t.id === id) || state.activeId === id) return state;
  return { ...state, activeId: id };
}

export function updateTab(
  state: TabsState,
  id: string,
  patch: Partial<Pick<Tab, 'title' | 'key' | 'dirty'>>,
): TabsState {
  const t = state.tabs.find((x) => x.id === id);
  if (!t) return state;
  const same =
    (patch.title === undefined || patch.title === t.title) &&
    (patch.key === undefined || patch.key === t.key) &&
    (patch.dirty === undefined || patch.dirty === t.dirty);
  if (same) return state;
  return { ...state, tabs: state.tabs.map((x) => (x.id === id ? { ...x, ...patch } : x)) };
}

export function findTab(state: TabsState, key: string): Tab | undefined {
  return state.tabs.find((t) => t.key === key);
}

export function activeTab(state: TabsState): Tab | undefined {
  return state.tabs.find((t) => t.id === state.activeId);
}

/** Ctrl+Tab and Ctrl+Shift+Tab: the next or previous tab, wrapping round. */
export function stepTab(state: TabsState, dir: 1 | -1): TabsState {
  const n = state.tabs.length;
  if (n < 2) return state;
  const at = state.tabs.findIndex((t) => t.id === state.activeId);
  const next = state.tabs[(at + dir + n) % n]!;
  return { ...state, activeId: next.id };
}

/** The tabs with unsaved changes, e.g. to ask about before the whole window closes. */
export function dirtyTabs(state: TabsState): Tab[] {
  return state.tabs.filter((t) => t.dirty);
}

export interface TabbedWindow {
  id: string;
  kind: string;
  z: number;
  minimised: boolean;
}

/**
 * Where a file opened in a tabbed program (Notes) goes: the tab that already shows it, else a
 * new tab in that program's frontmost window, else (null) a new window.
 */
export function tabTarget(
  windows: readonly TabbedWindow[],
  kind: string,
  tabsOf: (windowId: string) => TabsState | undefined,
  key: string,
): { windowId: string; tabId?: string } | null {
  const own = windows.filter((w) => w.kind === kind);
  for (const w of own) {
    const t = tabsOf(w.id);
    const hit = t && findTab(t, key);
    if (hit) return { windowId: w.id, tabId: hit.id };
  }
  const front = [...own].sort((a, b) => b.z - a.z)[0];
  return front ? { windowId: front.id } : null;
}
