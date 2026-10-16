/**
 * Desktop icons sit on a grid. Positions are stored in pixels (Settings.desktop.icons, shared by
 * every screen), and snapped to the grid of the screen that shows them, so one layout works on a
 * 7" kiosk and on a big monitor.
 */

export interface GridSpec {
  cellW: number;
  cellH: number;
  /** Space between the desktop edge and the first cell. */
  padX: number;
  padY: number;
}

export interface Cell {
  col: number;
  row: number;
}

export interface Grid {
  cols: number;
  rows: number;
}

export interface SavedPosition {
  key: string;
  x: number;
  y: number;
}

export type Placement = Map<string, Cell>;

export const ICON_GRID: GridSpec = { cellW: 92, cellH: 96, padX: 12, padY: 12 };

export function gridSize(area: { width: number; height: number }, spec: GridSpec): Grid {
  return {
    cols: Math.max(1, Math.floor((area.width - spec.padX) / spec.cellW)),
    rows: Math.max(1, Math.floor((area.height - spec.padY) / spec.cellH)),
  };
}

export function cellToPoint(cell: Cell, spec: GridSpec): { x: number; y: number } {
  return { x: spec.padX + cell.col * spec.cellW, y: spec.padY + cell.row * spec.cellH };
}

/** The cell whose top-left corner is nearest to `p`, inside the grid. */
export function pointToCell(p: { x: number; y: number }, spec: GridSpec, grid: Grid): Cell {
  const col = Math.round((p.x - spec.padX) / spec.cellW);
  const row = Math.round((p.y - spec.padY) / spec.cellH);
  return { col: clamp(col, 0, grid.cols - 1), row: clamp(row, 0, grid.rows - 1) };
}

/**
 * Where every icon in `keys` goes: its saved position (snapped) when it has one, otherwise the
 * next free cell, filling columns top to bottom from the left, as Windows does.
 */
export function placeIcons(
  keys: readonly string[],
  saved: readonly SavedPosition[],
  area: { width: number; height: number },
  spec: GridSpec,
): Placement {
  const grid = gridSize(area, spec);
  const wanted = new Set(keys);
  const taken = new Set<string>();
  const out: Placement = new Map();

  for (const s of saved) {
    if (!wanted.has(s.key) || out.has(s.key)) continue;
    const cell = freeNear(pointToCell(s, spec, grid), taken, grid);
    out.set(s.key, cell);
    taken.add(id(cell));
  }
  for (const key of keys) {
    if (out.has(key)) continue;
    const cell = nextFree(taken, grid);
    out.set(key, cell);
    taken.add(id(cell));
  }
  // Keep the caller's order, so the tab order follows the list, not the save order.
  return new Map(keys.map((k) => [k, out.get(k)!]));
}

/** `key` dropped at pixel `p`: the cell under it, or the nearest free one if that is taken. */
export function moveIcon(
  placement: Placement,
  key: string,
  p: { x: number; y: number },
  area: { width: number; height: number },
  spec: GridSpec,
): Placement {
  const grid = gridSize(area, spec);
  const taken = new Set([...placement].filter(([k]) => k !== key).map(([, c]) => id(c)));
  const next = new Map(placement);
  next.set(key, freeNear(pointToCell(p, spec, grid), taken, grid));
  return next;
}

/**
 * Several icons dragged together, moved by the same pixels. They keep their shape where the
 * cells are free; one that would land on another icon (or off the grid) takes the nearest free
 * cell. The cells the group leaves are free for the group itself.
 */
export function moveIcons(
  placement: Placement,
  keys: readonly string[],
  delta: { dx: number; dy: number },
  area: { width: number; height: number },
  spec: GridSpec,
): Placement {
  const grid = gridSize(area, spec);
  const moving = new Set(keys);
  const taken = new Set([...placement].filter(([k]) => !moving.has(k)).map(([, c]) => id(c)));
  const next = new Map(placement);
  for (const key of keys) {
    const from = placement.get(key);
    if (!from) continue;
    const p = cellToPoint(from, spec);
    const cell = freeNear(
      pointToCell({ x: p.x + delta.dx, y: p.y + delta.dy }, spec, grid),
      taken,
      grid,
    );
    next.set(key, cell);
    taken.add(id(cell));
  }
  return next;
}

/** A rectangle on the desktop in pixels; `w` and `h` are negative when drawn up or left. */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The icons whose cell a selection rectangle touches, in placement order. */
export function iconsInRect(placement: Placement, rect: Rect, spec: GridSpec): string[] {
  const left = Math.min(rect.x, rect.x + rect.w);
  const right = Math.max(rect.x, rect.x + rect.w);
  const top = Math.min(rect.y, rect.y + rect.h);
  const bottom = Math.max(rect.y, rect.y + rect.h);
  return [...placement]
    .filter(([, cell]) => {
      const p = cellToPoint(cell, spec);
      return p.x < right && p.x + spec.cellW > left && p.y < bottom && p.y + spec.cellH > top;
    })
    .map(([key]) => key);
}

/** The layout to save: every placed icon, in pixels, sorted by key so saves are stable. */
export function positionsForSettings(placement: Placement, spec: GridSpec): SavedPosition[] {
  return [...placement]
    .map(([key, cell]) => ({ key, ...cellToPoint(cell, spec) }))
    .sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
}

function freeNear(cell: Cell, taken: Set<string>, grid: Grid): Cell {
  if (!taken.has(id(cell))) return cell;
  // Rings of growing distance around the cell; the first free one wins.
  const maxR = Math.max(grid.cols, grid.rows);
  for (let r = 1; r <= maxR; r++) {
    let best: Cell | null = null;
    let bestD = Infinity;
    for (let dc = -r; dc <= r; dc++) {
      for (let dr = -r; dr <= r; dr++) {
        if (Math.max(Math.abs(dc), Math.abs(dr)) !== r) continue;
        const c = { col: cell.col + dc, row: cell.row + dr };
        if (c.col < 0 || c.row < 0 || c.col >= grid.cols || c.row >= grid.rows) continue;
        if (taken.has(id(c))) continue;
        const d = dc * dc + dr * dr;
        if (d < bestD) {
          best = c;
          bestD = d;
        }
      }
    }
    if (best) return best;
  }
  return nextFree(taken, grid);
}

function nextFree(taken: Set<string>, grid: Grid): Cell {
  for (let col = 0; ; col++) {
    for (let row = 0; row < grid.rows; row++) {
      const c = { col, row };
      if (!taken.has(id(c))) return c;
    }
    // A full screen keeps going into columns past the edge rather than stacking icons.
  }
}

function id(c: Cell): string {
  return `${c.col},${c.row}`;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(Math.max(v, lo), Math.max(lo, hi));
}
