/** Wires as SVG paths: rounded corners, and a hop wherever another net's wire crosses. */
import type { Cell } from '../lib/geometry';
import type { NetIndex } from '../lib/nets';
import type { Routing } from '../lib/router';
import type { Diagram } from '../lib/types';
import { G } from './looks';

export interface WirePath {
  id: string;
  d: string;
  color: string;
  width: number;
  kind: string;
  failed: boolean;
}

const R = 4;
const HOP = 3.4;

export function pathD(pts: Cell[], hops: Map<number, number[]>): string {
  let d = `M${pts[0]![0]} ${pts[0]![1]}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const a = pts[i]!;
    const b = pts[i + 1]!;
    const next = pts[i + 2];
    const horiz = a[1] === b[1];
    const len = Math.abs(horiz ? b[0] - a[0] : b[1] - a[1]);
    const sg = Math.sign(horiz ? b[0] - a[0] : b[1] - a[1]) || 1;
    const nlen = next ? Math.abs(b[1] === next[1] ? next[0] - b[0] : next[1] - b[1]) : 0;
    const r = next ? Math.min(R, len / 2, nlen / 2) : 0;
    if (horiz) {
      for (const hx of hops.get(i) ?? []) d += ` L${hx - sg * HOP} ${a[1]} A${HOP} ${HOP} 0 0 ${sg > 0 ? 1 : 0} ${hx + sg * HOP} ${a[1]}`;
    }
    d += horiz ? ` L${b[0] - sg * r} ${b[1]}` : ` L${b[0]} ${b[1] - sg * r}`;
    if (next) {
      const h2 = b[1] === next[1];
      const s2 = Math.sign(h2 ? next[0] - b[0] : next[1] - b[1]) || 1;
      d += ` Q${b[0]} ${b[1]} ${h2 ? b[0] + s2 * r : b[0]} ${h2 ? b[1] : b[1] + s2 * r}`;
    }
  }
  return d;
}

export function wirePaths(doc: Diagram, nets: NetIndex, routing: Routing): WirePath[] {
  // Vertical runs of every wire, for the hops on other nets' horizontal runs.
  const verts: { x: number; y0: number; y1: number; net: number }[] = [];
  for (const w of doc.wires) {
    const r = routing.routes.get(w.id);
    if (!r || r.failed) continue;
    const net = nets.of.get(w.a)?.id ?? 0;
    for (let i = 0; i < r.pts.length - 1; i++) {
      const [x0, y0] = r.pts[i]!;
      const [x1, y1] = r.pts[i + 1]!;
      if (x0 === x1) verts.push({ x: x0 * G, y0: Math.min(y0, y1) * G, y1: Math.max(y0, y1) * G, net });
    }
  }
  return doc.wires.flatMap((w): WirePath[] => {
    const r = routing.routes.get(w.id);
    if (!r) return [];
    const net = nets.of.get(w.a);
    const kind = net?.kind ?? 'signal';
    const color = w.color ?? net?.color ?? '#94a3b8';
    const pts = r.pts.map(([x, y]): Cell => [x * G, y * G]);
    if (r.failed) return [{ id: w.id, d: `M${pts.map((p) => p.join(' ')).join(' L')}`, color: '#ef4444', width: 2.5, kind, failed: true }];
    const hops = new Map<number, number[]>();
    for (let i = 0; i < pts.length - 1; i++) {
      const a = pts[i]!;
      const b = pts[i + 1]!;
      if (a[1] !== b[1]) continue;
      const lo = Math.min(a[0], b[0]);
      const hi = Math.max(a[0], b[0]);
      const xs = verts.filter((v) => v.net !== net?.id && v.x > lo + 4 && v.x < hi - 4 && a[1] > v.y0 && a[1] < v.y1).map((v) => v.x);
      if (xs.length) hops.set(i, [...new Set(xs)].sort((p, q) => (b[0] > a[0] ? p - q : q - p)));
    }
    const width = kind === 'cable' ? 6 : kind === 'power' && (net?.volts ?? 0) > 6 ? 4.2 : kind === 'motor' ? 3.8 : 3;
    return [{ id: w.id, d: pathD(pts, hops), color, width, kind, failed: false }];
  });
}
