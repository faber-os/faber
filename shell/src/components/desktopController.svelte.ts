import { getContext, setContext, untrack } from 'svelte';
import { loadGeometry, saveGeometry, type StorageLike } from '../lib/desktop/geometryStore';
import {
  closeWindow,
  emptyDesktop,
  ensureSize,
  focusWindow,
  isCompactScreen,
  minimiseWindow,
  openWindow,
  refit,
  setRect,
  toggleMaximise,
  updateWindow,
  type Area,
  type DesktopState,
  type Rect,
  type Size,
  type WindowKind,
  type WindowState,
} from '../lib/desktop/windows';
import { TASKBAR_HEIGHTS } from '../lib/desktop/taskbar';
import {
  hiddenBy,
  loadDismissals,
  saveDismissals,
  type Dismissals,
} from '../lib/desktop/dismissals';
import type { TabsState } from '../lib/desktop/tabs';
import {
  addRecent,
  loadRecent,
  renameRecent,
  saveRecent,
  type RecentFile,
} from '../lib/desktop/launcher';

const SAVE_DELAY_MS = 300;

/**
 * A window's title-bar tabs (Notes, Files): what WindowFrame draws, and what to do when one is
 * picked, closed or added. A window without tabs shows its title as a single tab-shaped label.
 */
export interface WindowTabs {
  state: TabsState;
  activate(tabId: string): void;
  close(tabId: string): void;
  /** Present when the window can open another tab (the + button). */
  add?: () => void;
  /** What + says to screen readers, e.g. "New note". */
  addLabel?: string;
}

export interface OpenOptions {
  key?: string;
  title?: string;
  params?: Record<string, string>;
  /** A size of its own the first time it opens (an app's `[window]` size). */
  size?: Size;
}

/**
 * The live desktop: the window manager's plain state (src/lib/desktop) kept in a rune, plus
 * remembering window geometry between visits.
 */
export class DesktopController {
  state = $state.raw<DesktopState>(emptyDesktop());
  viewport = $state.raw<Area>({ width: 1280, height: 800 });
  /** Small or Large (Settings.taskbar.size). */
  taskbarHeight = $state<number>(TASKBAR_HEIGHTS.small);
  /** Space for windows: the screen minus the taskbar. */
  area = $derived<Area>({
    width: this.viewport.width,
    height: Math.max(0, this.viewport.height - this.taskbarHeight),
  });
  /** Title-bar tabs, by window id, for the windows that have them. */
  tabs = $state.raw<Record<string, WindowTabs>>({});
  /** Files opened lately (this browser only), newest first, for the start menu. */
  recent = $state.raw<RecentFile[]>([]);
  compact = $derived(isCompactScreen(this.viewport));
  /** Notices closed with × (this browser only): see lib/desktop/dismissals. */
  dismissals = $state.raw<Dismissals>({});

  readonly kinds: Record<string, WindowKind>;
  private readonly storage: StorageLike | null;
  private geometry: Record<string, Rect>;
  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly closeGuards = new Map<string, () => boolean>();

  constructor(kinds: Record<string, WindowKind>, storage: StorageLike | null, viewport: Area) {
    this.kinds = kinds;
    this.storage = storage;
    this.geometry = loadGeometry(storage);
    this.recent = loadRecent(storage);
    this.dismissals = loadDismissals(storage);
    this.viewport = viewport;
  }

  /** Whether the notice `id`, now showing `items`, was dismissed and has nothing new. */
  isDismissed(id: string, items: readonly string[]): boolean {
    return hiddenBy(this.dismissals[id], items);
  }

  /** Hide the notice `id` until it shows something that isn't in `items`. */
  dismiss(id: string, items: readonly string[]): void {
    this.dismissals = { ...this.dismissals, [id]: [...items] };
    saveDismissals(this.storage, this.dismissals);
  }

  /** A file was opened: it goes to the top of the recent files. */
  noteRecent(path: string, now = Date.now()): void {
    const name = path.slice(path.lastIndexOf('/') + 1);
    this.recent = addRecent(this.recent, { path, name, at: now });
    saveRecent(this.storage, this.recent);
  }

  /** A file was renamed or moved: its recent entry follows it. */
  renameRecent(from: string, to: string): void {
    if (!this.recent.some((f) => f.path === from)) return;
    this.recent = renameRecent(this.recent, from, to);
    saveRecent(this.storage, this.recent);
  }

  /** What the title bar and taskbar show: the window's own title, or its kind's (translated). */
  titleOf(win: WindowState): string {
    return win.title ?? this.kind(win).title;
  }

  kind(win: WindowState): WindowKind {
    const k = this.kinds[win.kind];
    if (!k) throw new Error(`Unknown window kind "${win.kind}"`);
    return k;
  }

  open(kind: string, opts: OpenOptions = {}): string {
    const k = this.kinds[kind];
    if (!k) throw new Error(`Unknown window kind "${kind}"`);
    const key = opts.key ?? kind;
    const { state, id } = openWindow(
      this.state,
      {
        kind: k,
        key,
        title: opts.title,
        params: opts.params,
        saved: this.geometry[geometryKey(k, key)],
        size: opts.size,
      },
      this.area,
      { compact: this.compact },
    );
    this.state = state;
    return id;
  }

  focus(id: string): void {
    this.state = focusWindow(this.state, id);
  }

  minimise(id: string): void {
    this.state = minimiseWindow(this.state, id);
  }

  toggleMaximise(id: string): void {
    this.state = toggleMaximise(this.state, id);
  }

  /** Close at once, skipping any guard (the guard itself uses this after asking). */
  close(id: string): void {
    this.closeGuards.delete(id);
    this.state = closeWindow(this.state, id);
    if (id in this.tabs) {
      const { [id]: _gone, ...rest } = this.tabs;
      this.tabs = rest;
    }
  }

  /** A window reports its tabs (called from an effect whenever they change). */
  setTabs(id: string, tabs: WindowTabs | null): void {
    untrack(() => {
      if (tabs) this.tabs = { ...this.tabs, [id]: tabs };
      else if (id in this.tabs) {
        const { [id]: _gone, ...rest } = this.tabs;
        this.tabs = rest;
      }
    });
  }

  setTaskbarHeight(h: number): void {
    if (h === this.taskbarHeight) return;
    this.taskbarHeight = h;
    this.setViewport(this.viewport);
  }

  /**
   * What the close button and menu use: a window with unsaved work can say no (its guard returns
   * false and asks the user itself, then calls `close`).
   */
  requestClose(id: string): void {
    const guard = this.closeGuards.get(id);
    if (guard && !guard()) {
      this.focus(id);
      return;
    }
    this.close(id);
  }

  /** Returns the function that removes the guard again. */
  setCloseGuard(id: string, guard: () => boolean): () => void {
    this.closeGuards.set(id, guard);
    return () => {
      if (this.closeGuards.get(id) === guard) this.closeGuards.delete(id);
    };
  }

  /**
   * Change a window's title, key or params. Windows call this from effects (a title that follows
   * the document), so it reads the state untracked and only writes when something changed.
   */
  update(
    id: string,
    patch: { title?: string; key?: string; params?: Record<string, string> },
  ): void {
    untrack(() => {
      const w = this.state.windows.find((x) => x.id === id);
      if (!w) return;
      const same =
        (patch.title === undefined || patch.title === w.title) &&
        (patch.key === undefined || patch.key === w.key) &&
        (patch.params === undefined || sameParams(patch.params, w.params));
      if (!same) this.state = updateWindow(this.state, id, patch);
    });
  }

  /** Taskbar button: bring a window forward, or minimise it if it already is in front. */
  activate(id: string): void {
    const w = this.state.windows.find((x) => x.id === id);
    if (!w) return;
    if (this.state.focusedId === id && !w.minimised) this.minimise(id);
    else this.focus(id);
  }

  setRect(id: string, rect: Rect): void {
    this.state = setRect(this.state, id, rect);
    const w = this.state.windows.find((x) => x.id === id);
    if (!w) return;
    this.geometry = { ...this.geometry, [geometryKey(this.kind(w), w.key)]: rect };
    this.scheduleSave();
  }

  /** Gives a window room for a task (the pin picker) without changing its remembered size. */
  ensureSize(id: string, want: Size): void {
    this.state = ensureSize(this.state, id, want, this.area);
  }

  setViewport(viewport: Area): void {
    this.viewport = viewport;
    this.state = refit(
      this.state,
      this.area,
      (kind) => this.kinds[kind]?.minSize ?? { w: 0, h: 0 },
    );
  }

  /** Write remembered geometry now (also happens on its own shortly after a change). */
  flush(): void {
    if (this.saveTimer !== null) clearTimeout(this.saveTimer);
    this.saveTimer = null;
    saveGeometry(this.storage, this.geometry);
  }

  private scheduleSave(): void {
    if (this.saveTimer !== null) clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => this.flush(), SAVE_DELAY_MS);
  }
}

function sameParams(a: Record<string, string>, b: Record<string, string>): boolean {
  const ka = Object.keys(a);
  return ka.length === Object.keys(b).length && ka.every((k) => a[k] === b[k]);
}

function geometryKey(kind: WindowKind, key: string): string {
  return kind.multiple ? `${kind.kind}:${key}` : kind.kind;
}

const KEY = Symbol('faber-desktop');

export function provideDesktop(d: DesktopController): void {
  setContext(KEY, d);
}

export function useDesktop(): DesktopController {
  const d = getContext<DesktopController | undefined>(KEY);
  if (!d) throw new Error('useDesktop() needs provideDesktop() in a parent component.');
  return d;
}

export const DESKTOP_CONTEXT_KEY = KEY;

/** localStorage, or null where the browser blocks it. */
export function browserStorage(): StorageLike | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}
