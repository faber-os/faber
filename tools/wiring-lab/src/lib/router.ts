/**
 * Routes every wire on the unit grid with A* over (cell, direction).
 *
 * Rules, so wires stay readable: parts (plus one unit) are walls, but a breadboard is floor;
 * wires of different nets never share a cell in the same direction, never bend where another
 * wire runs, and cross only at right angles; wires of the same net may share track (a bus).
 * Signals route first, shortest first; power and ground last, so they gather into buses.
 */
import { DIRS, geom, type Cell, type PinGeo } from './geometry';
import type { NetIndex } from './nets';
import type { DefLookup, Diagram, Wire } from './types';
import { splitKey } from './types';

export const COST = { step: 1, share: 0.25, bend: 3, cross: 4, near: 0.6 };

export interface Route {
  /** Corner points from pin to pin, in units. */
  pts: Cell[];
  failed: boolean;
}

export interface Routing {
  routes: Map<string, Route>;
  /** Where a net's track splits three or more ways. */
  junctions: { x: number; y: number; net: number }[];
}

class Heap {
  private k: number[] = [];
  private v: number[] = [];
  get size(): number {
    return this.k.length;
  }
  push(val: number, key: number): void {
    const { k, v } = this;
    let i = k.length;
    k.push(key);
    v.push(val);
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (k[p]! <= key) break;
      k[i] = k[p]!;
      v[i] = v[p]!;
      i = p;
    }
    k[i] = key;
    v[i] = val;
  }
  pop(): number {
    const { k, v } = this;
    const top = v[0]!;
    const lk = k.pop()!;
    const lv = v.pop()!;
    if (k.length) {
      let i = 0;
      const n = k.length;
      for (;;) {
        let c = 2 * i + 1;
        if (c >= n) break;
        if (c + 1 < n && k[c + 1]! < k[c]!) c += 1;
        if (k[c]! >= lk) break;
        k[i] = k[c]!;
        v[i] = v[c]!;
        i = c;
      }
      k[i] = lk;
      v[i] = lv;
    }
    return top;
  }
}

export function simplify(pts: Cell[]): Cell[] {
  const out: Cell[] = [pts[0]!];
  for (let i = 1; i < pts.length - 1; i++) {
    const a = out[out.length - 1]!;
    const b = pts[i]!;
    const c = pts[i + 1]!;
    if ((a[0] === b[0] && b[0] === c[0]) || (a[1] === b[1] && b[1] === c[1])) continue;
    if (a[0] === b[0] && a[1] === b[1]) continue;
    out.push(b);
  }
  out.push(pts[pts.length - 1]!);
  return out;
}

export function routeAll(doc: Diagram, defs: DefLookup, nets: NetIndex): Routing {
  const routes = new Map<string, Route>();
  const junctions: Routing['junctions'] = [];
  if (!doc.parts.length) return { routes, junctions };
  const geos = doc.parts.map((p) => ({ part: p, g: geom(p, defs) }));
  let minx = Infinity;
  let miny = Infinity;
  let maxx = -Infinity;
  let maxy = -Infinity;
  for (const { g } of geos) {
    minx = Math.min(minx, g.x0);
    miny = Math.min(miny, g.y0);
    maxx = Math.max(maxx, g.x1);
    maxy = Math.max(maxy, g.y1);
  }
  const PAD = 40;
  const ox = minx - PAD;
  const oy = miny - PAD;
  const W = maxx - minx + 2 * PAD + 1;
  const H = maxy - miny + 2 * PAD + 1;
  const N = W * H;
  const wall = new Uint8Array(N);
  const hNet = new Int32Array(N);
  const vNet = new Int32Array(N);
  const node = new Int32Array(N);
  const idx = (x: number, y: number) => (x < ox || y < oy || x >= ox + W || y >= oy + H ? -1 : x - ox + (y - oy) * W);
  for (const { g } of geos) {
    if (g.def.kind === 'breadboard') continue;
    for (let y = g.y0 - 1; y <= g.y1 + 1; y++) {
      for (let x = g.x0 - 1; x <= g.x1 + 1; x++) {
        const i = idx(x, y);
        if (i >= 0) wall[i] = 1;
      }
    }
  }
  const pinAt = (key: string): PinGeo | undefined => {
    const [pid, pn] = splitKey(key);
    const part = doc.parts.find((p) => p.id === pid);
    return part ? geom(part, defs).pins[pn] : undefined;
  };
  const netId = (key: string) => nets.of.get(key)?.id ?? 0;
  // Every hole in use (by a wire or a plugged pin) belongs to its net; wire stubs too.
  for (const [pinKey, holeKey] of nets.plugs) {
    const h = pinAt(holeKey);
    if (h) {
      const i = idx(h.x, h.y);
      if (i >= 0) node[i] = netId(pinKey);
    }
  }
  for (const w of doc.wires) {
    for (const k of [w.a, w.b]) {
      const p = pinAt(k);
      if (!p) continue;
      const net = netId(k);
      for (const [x, y] of p.stub.length ? p.stub : [[p.x, p.y] as Cell]) {
        const i = idx(x, y);
        if (i >= 0) {
          wall[i] = 0;
          node[i] = net;
        }
      }
    }
  }

  const S = N * 4;
  const gScore = new Float32Array(S);
  const seen = new Int32Array(S);
  const done = new Int32Array(S);
  const prev = new Int32Array(S);
  let stamp = 0;

  const astar = (sx: number, sy: number, sdirs: number[], gx: number, gy: number, net: number): Cell[] | null => {
    stamp += 1;
    const start = idx(sx, sy);
    const goal = idx(gx, gy);
    if (start < 0 || goal < 0) return null;
    const heap = new Heap();
    for (const sd of sdirs) {
      const s0 = start * 4 + sd;
      gScore[s0] = 0;
      seen[s0] = stamp;
      prev[s0] = -1;
      heap.push(s0, Math.abs(sx - gx) + Math.abs(sy - gy));
    }
    let guard = 0;
    while (heap.size) {
      const st = heap.pop();
      if (done[st] === stamp) continue;
      done[st] = stamp;
      const cell = st >> 2;
      const d = st & 3;
      if (cell === goal) {
        const out: number[] = [];
        for (let s = st; s >= 0; s = prev[s]!) out.push(s >> 2);
        return out.reverse().map((c): Cell => [ox + (c % W), oy + ((c / W) | 0)]);
      }
      if (++guard > 600000) return null;
      const cx = cell % W;
      const cy = (cell / W) | 0;
      for (let nd = 0; nd < 4; nd++) {
        if (nd === ((d + 2) & 3)) continue;
        const nx = cx + DIRS[nd]![0];
        const ny = cy + DIRS[nd]![1];
        if (nx < 0 || ny < 0 || nx >= W || ny >= H) continue;
        const ni = nx + ny * W;
        if (wall[ni]) continue;
        if (node[ni] && node[ni] !== net) continue;
        const horiz = nd & 1;
        const same = horiz ? hNet[ni]! : vNet[ni]!;
        const cross = horiz ? vNet[ni]! : hNet[ni]!;
        if (same && same !== net) continue;
        let cost = COST.step;
        if (same === net || node[ni] === net) cost = COST.share;
        else if (cross && cross !== net) cost += COST.cross;
        if (nd !== d) {
          if ((hNet[cell] && hNet[cell] !== net) || (vNet[cell] && vNet[cell] !== net)) continue;
          cost += COST.bend;
        }
        if (cost !== COST.share) {
          for (const o of [-1, 1]) {
            const j = horiz ? ni + o * W : ni + o;
            if (j >= 0 && j < N) {
              const q = horiz ? hNet[j]! : vNet[j]!;
              if (q && q !== net) cost += COST.near;
            }
          }
        }
        const ns = ni * 4 + nd;
        const ng = gScore[st]! + cost;
        if (seen[ns] !== stamp || ng < gScore[ns]!) {
          seen[ns] = stamp;
          gScore[ns] = ng;
          prev[ns] = st;
          heap.push(ns, ng + Math.abs(nx + ox - gx) + Math.abs(ny + oy - gy));
        }
      }
    }
    return null;
  };

  const ends = (w: Wire) => ({ a: pinAt(w.a), b: pinAt(w.b) });
  const length = (w: Wire) => {
    const { a, b } = ends(w);
    return a && b ? Math.abs(a.x - b.x) + Math.abs(a.y - b.y) : 0;
  };
  const rank = (w: Wire) => {
    const k = nets.of.get(w.a)?.kind;
    return k === 'signal' || k === 'empty' ? 0 : k === 'motor' || k === 'speaker' || k === 'cable' ? 1 : 2;
  };
  const order = doc.wires.slice().sort((x, y) => rank(x) - rank(y) || length(x) - length(y));
  const edges = new Map<number, Set<string>>();

  for (const w of order) {
    const { a, b } = ends(w);
    if (!a || !b) continue;
    const net = netId(w.a);
    const sa: Cell = a.stub.length ? a.stub[a.stub.length - 1]! : [a.x, a.y];
    const sb: Cell = b.stub.length ? b.stub[b.stub.length - 1]! : [b.x, b.y];
    let sdirs = [0, 1, 2, 3];
    if (a.stub.length) {
      const before: Cell = a.stub.length > 1 ? a.stub[a.stub.length - 2]! : [a.x, a.y];
      sdirs = [dirBetween(before, sa)];
    }
    const path = astar(sa[0], sa[1], sdirs, sb[0], sb[1], net);
    if (!path) {
      routes.set(w.id, { pts: [[a.x, a.y], sa, sb, [b.x, b.y]], failed: true });
      continue;
    }
    const full: Cell[] = [[a.x, a.y], ...a.stub.slice(0, -1), ...path, ...b.stub.slice(0, -1).reverse(), [b.x, b.y]];
    const set = edges.get(net) ?? new Set<string>();
    edges.set(net, set);
    for (let i = 0; i < full.length - 1; i++) {
      const c1 = full[i]!;
      const c2 = full[i + 1]!;
      if (c1[0] === c2[0] && c1[1] === c2[1]) continue;
      const horiz = c1[1] === c2[1];
      for (const c of [c1, c2]) {
        const j = idx(c[0], c[1]);
        if (j >= 0) {
          if (horiz) hNet[j] = net;
          else vNet[j] = net;
        }
      }
      const i1 = idx(c1[0], c1[1]);
      const i2 = idx(c2[0], c2[1]);
      set.add(Math.min(i1, i2) + '|' + Math.max(i1, i2));
    }
    for (let i = 1; i < full.length - 1; i++) {
      const p0 = full[i - 1]!;
      const p1 = full[i]!;
      const p2 = full[i + 1]!;
      if ((p0[1] === p1[1]) !== (p1[1] === p2[1])) {
        const j = idx(p1[0], p1[1]);
        if (j >= 0) node[j] = net;
      }
    }
    routes.set(w.id, { pts: simplify(full), failed: false });
  }
  for (const [net, set] of edges) {
    const deg = new Map<number, number>();
    for (const e of set) {
      const [i, j] = e.split('|').map(Number) as [number, number];
      deg.set(i, (deg.get(i) ?? 0) + 1);
      deg.set(j, (deg.get(j) ?? 0) + 1);
    }
    for (const [c, n] of deg) if (n >= 3) junctions.push({ x: ox + (c % W), y: oy + ((c / W) | 0), net });
  }
  return { routes, junctions };
}

function dirBetween(a: Cell, b: Cell): number {
  if (b[0] > a[0]) return 1;
  if (b[0] < a[0]) return 3;
  if (b[1] > a[1]) return 2;
  return 0;
}
