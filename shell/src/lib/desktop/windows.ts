/**
 * Window manager logic: plain data in, plain data out. The UI keeps a DesktopState and replaces
 * it with what these functions return, so every rule here is testable without a browser.
 *
 * A window's `rect` is always its normal (not maximised) geometry. A maximised window is drawn
 * over the whole desktop area but keeps its rect, so restoring brings it back where it was.
 *
 * `chosen` is the geometry the user asked for (or the window opened with). `rect` is that,
 * fitted into the current screen. A smaller screen squeezes `rect`; a bigger one gives back
 * `chosen` again, so resizing the browser never loses a window's size for good.
 */

export interface Size {
  w: number;
  h: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The space windows may use: the screen minus the taskbar. */
export interface Area {
  width: number;
  height: number;
}

export type Edge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';

export interface WindowKind {
  kind: string;
  title: string;
  /** Can several windows of this kind be open (one per key)? */
  multiple: boolean;
  defaultSize: Size;
  minSize: Size;
}

export interface WindowState {
  id: string;
  kind: string;
  /** Identifies the instance: the kind itself for single windows, e.g. a signal address for viewers. */
  key: string;
  title: string;
  rect: Rect;
  chosen: Rect;
  maximised: boolean;
  minimised: boolean;
  z: number;
  params: Record<string, string>;
}

export interface DesktopState {
  /** In the order they were opened (the taskbar order). */
  windows: WindowState[];
  focusedId: string | null;
  nextZ: number;
  nextId: number;
}

export interface OpenRequest {
  kind: WindowKind;
  key?: string;
  title?: string;
  params?: Record<string, string>;
  /** Geometry remembered from last time. */
  saved?: Rect;
}

const CASCADE_STEP = 28;
const CASCADE_STEPS = 8;

export function emptyDesktop(): DesktopState {
  return { windows: [], focusedId: null, nextZ: 1, nextId: 1 };
}

/** The 7" 1024×600 kiosk screen, and anything smaller in either direction. */
export function isCompactScreen(viewport: Area): boolean {
  return viewport.width <= 1024 || viewport.height <= 600;
}

export function clampRect(r: Rect, min: Size, area: Area): Rect {
  const w = Math.min(Math.max(r.w, min.w), area.width);
  const h = Math.min(Math.max(r.h, min.h), area.height);
  const x = clamp(r.x, 0, area.width - w);
  const y = clamp(r.y, 0, area.height - h);
  return { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) };
}

export function moveRect(start: Rect, dx: number, dy: number, area: Area): Rect {
  return clampRect({ ...start, x: start.x + dx, y: start.y + dy }, { w: 0, h: 0 }, area);
}

/** Resize by dragging `edge` by (dx, dy). The opposite edges stay where they are. */
export function resizeRect(
  start: Rect,
  edge: Edge,
  dx: number,
  dy: number,
  min: Size,
  area: Area,
): Rect {
  let left = start.x;
  let top = start.y;
  let right = start.x + start.w;
  let bottom = start.y + start.h;
  const minW = Math.min(min.w, area.width);
  const minH = Math.min(min.h, area.height);

  if (edge.includes('w')) left = clamp(left + dx, 0, right - minW);
  if (edge.includes('e')) right = clamp(right + dx, left + minW, area.width);
  if (edge.includes('n')) top = clamp(top + dy, 0, bottom - minH);
  if (edge.includes('s')) bottom = clamp(bottom + dy, top + minH, area.height);

  return {
    x: Math.round(left),
    y: Math.round(top),
    w: Math.round(right - left),
    h: Math.round(bottom - top),
  };
}

export function openWindow(
  state: DesktopState,
  req: OpenRequest,
  area: Area,
  opts: { compact?: boolean } = {},
): { state: DesktopState; id: string } {
  const key = req.key ?? req.kind.kind;
  const existing = state.windows.find(
    (w) => w.kind === req.kind.kind && (!req.kind.multiple || w.key === key),
  );
  if (existing) return { state: focusWindow(state, existing.id), id: existing.id };

  const id = `w${state.nextId}`;
  const chosen = req.saved ?? cascadeRect(state, req.kind.defaultSize, area);
  const rect = clampRect(chosen, req.kind.minSize, area);
  const win: WindowState = {
    id,
    kind: req.kind.kind,
    key,
    title: req.title ?? req.kind.title,
    rect,
    chosen,
    maximised: opts.compact === true,
    minimised: false,
    z: state.nextZ,
    params: req.params ?? {},
  };
  return {
    state: {
      windows: [...state.windows, win],
      focusedId: id,
      nextZ: state.nextZ + 1,
      nextId: state.nextId + 1,
    },
    id,
  };
}

export function focusWindow(state: DesktopState, id: string): DesktopState {
  const win = find(state, id);
  if (!win) return state;
  if (state.focusedId === id && !win.minimised && win.z === state.nextZ - 1) return state;
  return {
    ...replace(state, id, { minimised: false, z: state.nextZ }),
    focusedId: id,
    nextZ: state.nextZ + 1,
  };
}

export function minimiseWindow(state: DesktopState, id: string): DesktopState {
  if (!find(state, id)) return state;
  const next = replace(state, id, { minimised: true });
  return { ...next, focusedId: topWindow(next)?.id ?? null };
}

export function toggleMaximise(state: DesktopState, id: string): DesktopState {
  const win = find(state, id);
  if (!win) return state;
  return focusWindow(replace(state, id, { maximised: !win.maximised }), id);
}

export function closeWindow(state: DesktopState, id: string): DesktopState {
  if (!find(state, id)) return state;
  const next = { ...state, windows: state.windows.filter((w) => w.id !== id) };
  return { ...next, focusedId: topWindow(next)?.id ?? null };
}

/** Rename a window or point it at something else (a document saved under a new name). */
export function updateWindow(
  state: DesktopState,
  id: string,
  patch: Partial<Pick<WindowState, 'title' | 'key' | 'params'>>,
): DesktopState {
  if (!find(state, id)) return state;
  return replace(state, id, patch);
}

export function setRect(state: DesktopState, id: string, rect: Rect): DesktopState {
  if (!find(state, id)) return state;
  return replace(state, id, { rect, chosen: rect });
}

/** Keep every window inside the area after the screen changed size. */
export function refit(
  state: DesktopState,
  area: Area,
  minSize: (kind: string) => Size,
): DesktopState {
  return {
    ...state,
    windows: state.windows.map((w) => ({
      ...w,
      rect: clampRect(w.chosen, minSize(w.kind), area),
    })),
  };
}

/** Windows from bottom to top (the order to draw them). */
export function stackOrder(state: DesktopState): WindowState[] {
  return [...state.windows].sort((a, b) => a.z - b.z);
}

/** The highest window that is not minimised. */
export function topWindow(state: DesktopState): WindowState | undefined {
  return stackOrder(state)
    .filter((w) => !w.minimised)
    .at(-1);
}

function cascadeRect(state: DesktopState, size: Size, area: Area): Rect {
  const step = (state.windows.length % CASCADE_STEPS) * CASCADE_STEP;
  const x = Math.max(0, (area.width - size.w) / 2 - (CASCADE_STEPS / 2) * CASCADE_STEP) + step;
  const y = Math.max(0, (area.height - size.h) / 3) + step;
  return { x, y, w: size.w, h: size.h };
}

function find(state: DesktopState, id: string): WindowState | undefined {
  return state.windows.find((w) => w.id === id);
}

function replace(state: DesktopState, id: string, patch: Partial<WindowState>): DesktopState {
  return { ...state, windows: state.windows.map((w) => (w.id === id ? { ...w, ...patch } : w)) };
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(Math.max(v, lo), Math.max(lo, hi));
}
