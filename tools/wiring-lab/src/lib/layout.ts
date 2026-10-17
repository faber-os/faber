/**
 * Arrange: boards in a row (the busiest in the middle, each next to the side its link leaves
 * from); every other part on the side of its board where its pins are, turned so those pins face
 * the board, in the order of those pins; parts that only meet other parts (a motor, a battery)
 * beyond their busiest partner. Parts plugged into a breadboard ride along with it. Finally every
 * part is nudged until it keeps clear of every other.
 */
import { DIRS, geom, tooClose, type PartGeo } from './geometry';
import { findPlugs } from './nets';
import type { DefLookup, Diagram, Dir, Part } from './types';
import { splitKey } from './types';

const CLEAR = 8;

interface Link {
  mine: string;
  other: string;
}

export function mountedOn(doc: Diagram, defs: DefLookup): Map<string, string> {
  // Part id → the breadboard it is plugged into.
  const out = new Map<string, string>();
  for (const [p, h] of findPlugs(doc, defs)) out.set(splitKey(p)[0], splitKey(h)[0]);
  return out;
}

export function overlapsAny(doc: Diagram, defs: DefLookup, part: Part, margin = CLEAR, ignore: Set<string> = new Set()): boolean {
  const g = geom(part, defs);
  const isBoard = defs(part.def)?.kind === 'breadboard';
  return doc.parts.some((o) => {
    if (o === part || ignore.has(o.id)) return false;
    const od = defs(o.def);
    // Anything may sit on a breadboard; a breadboard may slide under parts already on it.
    if (od?.kind === 'breadboard' && !isBoard) return false;
    if (isBoard && ignore.has(o.id)) return false;
    return tooClose(g, geom(o, defs), margin);
  });
}

/** Move a part outward (or spiral around it) until it keeps clear of every other part. */
export function settle(doc: Diagram, defs: DefLookup, part: Part, dir?: Dir, ignore: Set<string> = new Set()): void {
  if (!overlapsAny(doc, defs, part, CLEAR, ignore)) return;
  const ox = part.x;
  const oy = part.y;
  if (dir !== undefined) {
    for (let i = 1; i < 120; i++) {
      part.x = ox + DIRS[dir]![0] * i * 2;
      part.y = oy + DIRS[dir]![1] * i * 2;
      if (!overlapsAny(doc, defs, part, CLEAR, ignore)) return;
    }
  }
  for (let r = 2; r < 300; r += 2) {
    for (let a = -r; a <= r; a += 2) {
      for (const [dx, dy] of [[a, -r], [a, r], [-r, a], [r, a]] as const) {
        part.x = ox + dx;
        part.y = oy + dy;
        if (!overlapsAny(doc, defs, part, CLEAR, ignore)) return;
      }
    }
  }
  part.x = ox;
  part.y = oy;
}

function even(n: number): number {
  return Math.round(n / 2) * 2;
}

export function arrange(doc: Diagram, defs: DefLookup): void {
  if (!doc.parts.length) return;
  const mounted = mountedOn(doc, defs);
  const startPos = new Map(doc.parts.map((p) => [p.id, [p.x, p.y] as const]));
  // Plugged parts are left out of every clearance test until they follow their breadboard.
  const floating = new Set(mounted.keys());
  const links = (part: Part): Link[] =>
    doc.wires.flatMap((w) => {
      const [pa] = splitKey(w.a);
      const [pb] = splitKey(w.b);
      if (pa === part.id && pb !== part.id) return [{ mine: splitKey(w.a)[1], other: w.b }];
      if (pb === part.id && pa !== part.id) return [{ mine: splitKey(w.b)[1], other: w.a }];
      return [];
    });
  // A plugged part's wires count for its breadboard.
  const owner = (id: string) => mounted.get(id) ?? id;
  const kindOf = (key: string) => {
    const [pid, pn] = splitKey(key);
    const part = doc.parts.find((p) => p.id === pid);
    return part ? defs(part.def)?.pins.find((p) => p.id === pn)?.kind : undefined;
  };
  const isPower = (key: string) => {
    const k = kindOf(key);
    return k === 'gnd' || k === 'supply';
  };

  // Boards: the busiest first, the others beside the side their link leaves it from.
  const boards = doc.parts.filter((p) => defs(p.def)?.kind === 'board').sort((x, y) => links(y).length - links(x).length);
  const placed = new Set<string>();
  if (boards.length) {
    const first = boards[0]!;
    first.x = 0;
    first.y = 0;
    first.rot = 0;
    placed.add(first.id);
    let leftEdge = 0;
    let rightEdge = geom(first, defs).x1;
    for (const b of boards.slice(1)) {
      b.rot = 0;
      const g = geom(b, defs);
      const toFirst = links(b).filter((l) => placed.has(splitKey(l.other)[0]));
      const leaves = toFirst.map((l) => {
        const host = doc.parts.find((p) => p.id === splitKey(l.other)[0])!;
        return geom(host, defs).pins[splitKey(l.other)[1]]!.d;
      });
      const left = leaves.length ? leaves.filter((d) => d === 3).length >= leaves.filter((d) => d === 1).length : true;
      const gap = 110;
      if (left) {
        b.x = even(leftEdge - gap - g.w);
        leftEdge = b.x;
      } else {
        b.x = even(rightEdge + gap);
        rightEdge = b.x + g.w;
      }
      b.y = 0;
      placed.add(b.id);
    }
  }

  // Parts and breadboards that meet a board: on that board's side where their pins are.
  type Item = { part: Part; board: Part; side: Dir; want: number; mine: string[]; count: number };
  const items: Item[] = [];
  const tier2: Part[] = [];
  const loose: Part[] = [];
  for (const part of doc.parts) {
    if (placed.has(part.id) || mounted.has(part.id)) continue;
    const riders = doc.parts.filter((p) => mounted.get(p.id) === part.id);
    const ls = [part, ...riders].flatMap(links).filter((l) => owner(splitKey(l.other)[0]) !== part.id);
    const toBoards = ls.filter((l) => defs(doc.parts.find((p) => p.id === splitKey(l.other)[0])!.def)?.kind === 'board');
    if (!toBoards.length || (toBoards.every((l) => isPower(l.other)) && ls.length > toBoards.length * 1.5)) {
      (ls.length ? tier2 : loose).push(part);
      continue;
    }
    // The board it has most signal wires to.
    const pick = toBoards.filter((l) => !isPower(l.other));
    const use = pick.length ? pick : toBoards;
    const per = new Map<string, Link[]>();
    for (const l of use) per.set(splitKey(l.other)[0], [...(per.get(splitKey(l.other)[0]) ?? []), l]);
    const [boardId, mine] = [...per].sort((x, y) => y[1].length - x[1].length)[0]!;
    const board = doc.parts.find((p) => p.id === boardId)!;
    const bg = geom(board, defs);
    const count = [0, 0, 0, 0];
    let sx = 0;
    let sy = 0;
    for (const l of mine) {
      const bp = bg.pins[splitKey(l.other)[1]]!;
      count[bp.d]! += 1;
      sx += bp.x;
      sy += bp.y;
    }
    const side = count.indexOf(Math.max(...count)) as Dir;
    const own = mine.map((l) => l.mine).filter((m) => defs(part.def)?.pins.some((p) => p.id === m));
    items.push({ part, board, side, want: side === 0 || side === 2 ? sx / mine.length : sy / mine.length, mine: own, count: toBoards.length });
  }
  const face = (part: Part, mine: string[], towards: Dir) => {
    const d = defs(part.def)!;
    if (d.kind === 'breadboard') return;
    const count = [0, 0, 0, 0];
    for (const m of mine) {
      const p = d.pins.find((x) => x.id === m);
      if (p) count[p.d]! += 1;
    }
    const dom = count.indexOf(Math.max(...count));
    part.rot = ((towards - dom + 4) & 3) as Dir;
  };
  for (const board of doc.parts.filter((p) => placed.has(p.id))) {
    const bg = geom(board, defs);
    const sides: Item[][] = [[], [], [], []];
    for (const it of items.filter((i) => i.board === board)) sides[it.side]!.push(it);
    // A top or bottom row much wider than the board spills its outermost parts to the sides.
    for (const s of [0, 2]) {
      const width = () => sides[s]!.reduce((n, it) => n + Math.max(geom(it.part, defs).w, geom(it.part, defs).h) + CLEAR, -CLEAR);
      let guard = 0;
      while (sides[s]!.length > 1 && width() > bg.w * 1.7 && guard++ < 20) {
        const cx = (bg.x0 + bg.x1) / 2;
        const list = sides[s]!.slice().sort((x, y) => x.want - y.want);
        const lft = list[0]!;
        const rgt = list[list.length - 1]!;
        const take = cx - lft.want > rgt.want - cx ? lft : rgt;
        sides[s]!.splice(sides[s]!.indexOf(take), 1);
        take.side = take === lft ? 3 : 1;
        take.want = (bg.y0 + bg.y1) / 2;
        sides[take.side]!.push(take);
      }
    }
    for (const s of [0, 1, 2, 3] as Dir[]) {
      const list = sides[s]!;
      if (!list.length) continue;
      const towards = ((s + 2) & 3) as Dir;
      list.forEach((it) => face(it.part, it.mine, towards));
      list.sort((x, y) => x.want - y.want);
      const wiresOut = list.reduce((n, it) => n + it.count, 0);
      const gap = even(12 + Math.min(30, wiresOut * 1.4));
      pack(list, s, gap, bg);
    }
  }
  for (const it of items) placed.add(it.part.id);

  // Parts that only meet other parts: beyond their busiest partner, facing it.
  let guard = 0;
  while (tier2.length && guard++ < 60) {
    const part = tier2.shift()!;
    const ls = links(part);
    const partners = new Map<string, number>();
    for (const l of ls) {
      const o = owner(splitKey(l.other)[0]);
      if (placed.has(o)) partners.set(o, (partners.get(o) ?? 0) + 1);
    }
    const best = [...partners].sort((x, y) => y[1] - x[1])[0]?.[0];
    if (!best) {
      if (tier2.length && tier2.some((p) => links(p).some((l) => placed.has(owner(splitKey(l.other)[0]))))) tier2.push(part);
      else loose.push(part);
      continue;
    }
    const host = doc.parts.find((p) => p.id === best)!;
    const hg = geom(host, defs);
    const used = ls.filter((l) => owner(splitKey(l.other)[0]) === best);
    const count = [0, 0, 0, 0];
    const pts: { x: number; y: number }[] = [];
    for (const l of used) {
      const hp = geom(doc.parts.find((p) => p.id === splitKey(l.other)[0])!, defs).pins[splitKey(l.other)[1]]!;
      count[hp.pin.kind === 'hole' ? 2 : hp.d]! += 1;
      pts.push(hp);
    }
    const out = count.indexOf(Math.max(...count)) as Dir;
    face(part, used.map((l) => l.mine), ((out + 2) & 3) as Dir);
    const g = geom(part, defs);
    const mx = pts.reduce((s, p) => s + p.x, 0) / pts.length;
    const my = pts.reduce((s, p) => s + p.y, 0) / pts.length;
    const gap = 16 + used.length * 2;
    if (out === 3) [part.x, part.y] = [hg.x0 - gap - g.w, my - g.h / 2];
    if (out === 1) [part.x, part.y] = [hg.x1 + gap, my - g.h / 2];
    if (out === 0) [part.x, part.y] = [mx - g.w / 2, hg.y0 - gap - g.h];
    if (out === 2) [part.x, part.y] = [mx - g.w / 2, hg.y1 + gap];
    part.x = even(part.x);
    part.y = even(part.y);
    settle(doc, defs, part, out, floating);
    placed.add(part.id);
  }
  // Anything unconnected waits in a row under everything else.
  let maxY = -Infinity;
  let minX = Infinity;
  for (const p of doc.parts) {
    if (!placed.has(p.id)) continue;
    const g = geom(p, defs);
    maxY = Math.max(maxY, g.y1);
    minX = Math.min(minX, g.x0);
  }
  if (!Number.isFinite(maxY)) [maxY, minX] = [0, 0];
  let x = minX;
  for (const part of loose) {
    if (defs(part.def)?.kind !== 'breadboard') part.rot = 0;
    const g = geom(part, defs);
    part.x = even(x);
    part.y = even(maxY + 40);
    x += g.w + 12;
    settle(doc, defs, part, 2, floating);
    placed.add(part.id);
  }
  // Settle everything that ended up too close, outward from the middle.
  for (const part of doc.parts) {
    if (mounted.has(part.id) || defs(part.def)?.kind === 'board') continue;
    const g = geom(part, defs);
    const dir: Dir = Math.abs(g.y0) > Math.abs(g.x0) ? (g.y0 < 0 ? 0 : 2) : g.x0 < 0 ? 3 : 1;
    settle(doc, defs, part, dir, floating);
  }
  // Plugged parts follow their breadboard.
  for (const [pid, bid] of mounted) {
    const p = doc.parts.find((x) => x.id === pid)!;
    const b = doc.parts.find((x) => x.id === bid)!;
    const [bx, by] = startPos.get(bid)!;
    p.x += b.x - bx;
    p.y += b.y - by;
  }

  function pack(list: Item[], s: Dir, gap: number, bg: PartGeo) {
    // Along the side in order, each as near its pins as the one before allows, then the whole row
    // shifted back toward the middle of what they wished for.
    const horizontal = s === 0 || s === 2;
    const lens = list.map((it) => (horizontal ? geom(it.part, defs).w : geom(it.part, defs).h));
    const pos: number[] = [];
    let cursor = -Infinity;
    list.forEach((it, i) => {
      const at = Math.max(Math.round(it.want - lens[i]! / 2), cursor);
      pos.push(at);
      cursor = at + lens[i]! + CLEAR;
    });
    const wish = list.reduce((n, it) => n + it.want, 0) / list.length;
    const mid = (pos[0]! + pos[pos.length - 1]! + lens[lens.length - 1]!) / 2;
    const shift = Math.round(wish - mid);
    list.forEach((it, i) => {
      const g = geom(it.part, defs);
      const at = pos[i]! + shift;
      if (s === 0) [it.part.x, it.part.y] = [at, bg.y0 - gap - g.h];
      if (s === 2) [it.part.x, it.part.y] = [at, bg.y1 + gap];
      if (s === 3) [it.part.x, it.part.y] = [bg.x0 - gap - g.w, at];
      if (s === 1) [it.part.x, it.part.y] = [bg.x1 + gap, at];
      it.part.x = even(it.part.x);
      it.part.y = even(it.part.y);
    });
  }
}

/**
 * A part set down over a breadboard drops into the nearest spot where its legs land in holes
 * (within 6 units, about 7.6 mm): all its legs if it can, as many as it can otherwise.
 * Returns whether it is now plugged in.
 */
export function snapToBreadboard(doc: Diagram, defs: DefLookup, part: Part): boolean {
  const d = defs(part.def);
  if (!d || (d.kind !== 'part' && !d.pluggable)) return false;
  const g = geom(part, defs);
  const legs = Object.values(g.pins).filter((p) => !p.pin.terminal && p.pin.kind !== 'port');
  if (!legs.length) return false;
  const holes = new Set<string>();
  for (const b of doc.parts) {
    if (defs(b.def)?.kind !== 'breadboard') continue;
    const bg = geom(b, defs);
    if (!tooClose(bg, g, 6)) continue;
    for (const h of Object.values(bg.pins)) holes.add(`${h.x},${h.y}`);
  }
  if (!holes.size) return false;
  let best: { dx: number; dy: number; n: number } | undefined;
  for (let dy = -6; dy <= 6; dy += 2) {
    for (let dx = -6; dx <= 6; dx += 2) {
      const n = legs.filter((p) => holes.has(`${p.x + dx},${p.y + dy}`)).length;
      if (!n) continue;
      const score = n * 100 - Math.abs(dx) - Math.abs(dy);
      if (!best || score > best.n * 100 - Math.abs(best.dx) - Math.abs(best.dy)) best = { dx, dy, n };
    }
  }
  if (!best || best.n < Math.min(2, legs.length)) return false;
  part.x += best.dx;
  part.y += best.dy;
  return true;
}

/**
 * Mirrors where the parts `ids` sit, about the middle of their bounding box, left–right (`h`) or
 * top–bottom (`v`). The parts themselves keep their face and turn: a real board's pins can't be
 * mirrored. A module in a breadboard moves with the breadboard unless it is in `ids` itself.
 */
export function mirrorLayout(doc: Diagram, defs: DefLookup, ids: string[], axis: 'h' | 'v'): void {
  const parts = ids.map((id) => doc.parts.find((p) => p.id === id)).filter((p): p is Part => !!p);
  if (!parts.length) return;
  const gs = parts.map((p) => geom(p, defs));
  const cx = (Math.min(...gs.map((g) => g.x0)) + Math.max(...gs.map((g) => g.x1))) / 2;
  const cy = (Math.min(...gs.map((g) => g.y0)) + Math.max(...gs.map((g) => g.y1))) / 2;
  const even = (v: number) => Math.round(v / 2) * 2;
  const shift = new Map<string, [number, number]>();
  parts.forEach((p, i) => {
    const g = gs[i]!;
    const tx = axis === 'h' ? 2 * cx - (g.x0 + g.w / 2) : g.x0 + g.w / 2;
    const ty = axis === 'v' ? 2 * cy - (g.y0 + g.h / 2) : g.y0 + g.h / 2;
    shift.set(p.id, [even(tx - g.w / 2) - p.x, even(ty - g.h / 2) - p.y]);
  });
  const on = mountedOn(doc, defs);
  for (const r of doc.parts) {
    const host = on.get(r.id);
    if (host && !shift.has(r.id) && shift.has(host)) shift.set(r.id, shift.get(host)!);
  }
  for (const p of doc.parts) {
    const s = shift.get(p.id);
    if (!s) continue;
    p.x += s[0];
    p.y += s[1];
  }
}
