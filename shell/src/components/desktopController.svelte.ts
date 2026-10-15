import { getContext, setContext } from 'svelte';
import { loadGeometry, saveGeometry, type StorageLike } from '../lib/desktop/geometryStore';
import {
  closeWindow,
  emptyDesktop,
  focusWindow,
  isCompactScreen,
  minimiseWindow,
  openWindow,
  refit,
  setRect,
  toggleMaximise,
  type Area,
  type DesktopState,
  type Rect,
  type WindowKind,
  type WindowState,
} from '../lib/desktop/windows';

export const TASKBAR_HEIGHT = 56;
const SAVE_DELAY_MS = 300;

export interface OpenOptions {
  key?: string;
  title?: string;
  params?: Record<string, string>;
}

/**
 * The live desktop: the window manager's plain state (src/lib/desktop) kept in a rune, plus
 * remembering window geometry between visits.
 */
export class DesktopController {
  state = $state.raw<DesktopState>(emptyDesktop());
  viewport = $state.raw<Area>({ width: 1280, height: 800 });
  /** Space for windows: the screen minus the taskbar. */
  area = $derived<Area>({
    width: this.viewport.width,
    height: Math.max(0, this.viewport.height - TASKBAR_HEIGHT),
  });
  compact = $derived(isCompactScreen(this.viewport));

  readonly kinds: Record<string, WindowKind>;
  private readonly storage: StorageLike | null;
  private geometry: Record<string, Rect>;
  private saveTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(kinds: Record<string, WindowKind>, storage: StorageLike | null, viewport: Area) {
    this.kinds = kinds;
    this.storage = storage;
    this.geometry = loadGeometry(storage);
    this.viewport = viewport;
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

  close(id: string): void {
    this.state = closeWindow(this.state, id);
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
