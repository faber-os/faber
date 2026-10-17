/** Where a part's body, pins and wire stubs are in the world, after rotation (quarter turns cw). */
import type { DefLookup, Dir, Part, PartDef, PinDef } from './types';

export const DIRS: readonly (readonly [number, number])[] = [
  [0, -1],
  [1, 0],
  [0, 1],
  [-1, 0],
];

export type Cell = [number, number];

export function rotPt(d: PartDef, rot: Dir, x: number, y: number): Cell {
  switch (rot) {
    case 1:
      return [d.h - y, x];
    case 2:
      return [d.w - x, d.h - y];
    case 3:
      return [y, d.w - x];
    default:
      return [x, y];
  }
}

/**
 * The fixed start of a wire: straight out of the pin to two units past the body, where the router
 * takes over. A pin in an inner header row first steps sideways into the gap between the outer
 * row's pins. A breadboard hole has none: the wire starts in the hole.
 */
export function stubOf(d: PartDef, p: PinDef): Cell[] {
  if (p.kind === 'hole') return [];
  const cells: Cell[] = [];
  let x = p.x;
  let y = p.y;
  if (p.jog) {
    x += p.jog[0];
    y += p.jog[1];
    cells.push([x, y]);
  }
  const [dx, dy] = DIRS[p.d]!;
  const out = (cx: number, cy: number) =>
    p.d === 0 ? cy <= -2 : p.d === 2 ? cy >= d.h + 2 : p.d === 1 ? cx >= d.w + 2 : cx <= -2;
  do {
    x += dx;
    y += dy;
    cells.push([x, y]);
  } while (!out(x, y));
  return cells;
}

export interface PinGeo {
  x: number;
  y: number;
  d: Dir;
  stub: Cell[];
  pin: PinDef;
}

export interface PartGeo {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  w: number;
  h: number;
  pins: Record<string, PinGeo>;
  def: PartDef;
}

const cache = new Map<string, { key: string; g: PartGeo }>();

export function geom(part: Part, defs: DefLookup): PartGeo {
  const d = defs(part.def);
  if (!d) throw new Error(`unknown part type ${part.def}`);
  const key = `${part.def}|${part.x}|${part.y}|${part.rot}|${d.w}|${d.h}|${d.pins.length}`;
  const hit = cache.get(part.id);
  if (hit && hit.key === key && hit.g.def === d) return hit.g;
  const odd = part.rot & 1;
  const w = odd ? d.h : d.w;
  const h = odd ? d.w : d.h;
  const pins: Record<string, PinGeo> = {};
  for (const p0 of d.pins) {
    const p = p0;
    const [px, py] = rotPt(d, part.rot, p.x, p.y);
    const stub = stubOf(d, p).map(([sx, sy]): Cell => {
      const [rx, ry] = rotPt(d, part.rot, sx, sy);
      return [part.x + rx, part.y + ry];
    });
    pins[p.id] = {
      x: part.x + px,
      y: part.y + py,
      d: ((p.d + part.rot) & 3) as Dir,
      stub,
      pin: p0,
    };
  }
  const g: PartGeo = { x0: part.x, y0: part.y, x1: part.x + w, y1: part.y + h, w, h, pins, def: d };
  cache.set(part.id, { key, g });
  return g;
}

/** Two parts' bodies come closer than `margin` units. */
export function tooClose(a: PartGeo, b: PartGeo, margin: number): boolean {
  return (
    a.x0 < b.x1 + margin && a.x1 + margin > b.x0 && a.y0 < b.y1 + margin && a.y1 + margin > b.y0
  );
}
