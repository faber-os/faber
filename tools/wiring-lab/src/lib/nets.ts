/**
 * Nets: pins joined by wires, by a breadboard's strips, or by a part plugged into a breadboard.
 * A net's kind and colour follow what it carries.
 */
import { geom } from './geometry';
import type { DefLookup, Diagram, PinDef } from './types';
import { splitKey } from './types';

export type NetKind = 'power' | 'gnd' | 'short' | 'signal' | 'motor' | 'speaker' | 'cable' | 'empty';

export interface Net {
  id: number;
  /** Every pin on the net, breadboard holes included. */
  keys: string[];
  kind: NetKind;
  volts: number;
  color: string;
}

export interface NetIndex {
  of: Map<string, Net>;
  nets: Net[];
  /** Part pins sitting in breadboard holes: [part pin, hole]. */
  plugs: [string, string][];
}

export const SIGNAL_COLORS = [
  '#3b82f6', '#facc15', '#f5f5f4', '#c084fc', '#22d3ee', '#f472b6', '#a3e635', '#b07a43',
  '#94a3b8', '#60a5fa', '#fde047', '#e879f9', '#2dd4bf', '#fb7185',
];
export const GND_COLOR = '#202328';
export const USB_COLOR = '#8b939c';

export function powerColor(volts: number): string {
  if (Math.abs(volts - 3.3) < 0.2) return '#f97316';
  if (volts <= 5.5) return '#ef4444';
  return '#be123c';
}

/** Part pins that land exactly on a breadboard hole are plugged into it. */
export function findPlugs(doc: Diagram, defs: DefLookup): [string, string][] {
  const boards = doc.parts.filter((p) => defs(p.def)?.kind === 'breadboard');
  if (!boards.length) return [];
  const holes = new Map<string, string>();
  for (const b of boards) {
    const g = geom(b, defs);
    for (const id in g.pins) {
      const p = g.pins[id]!;
      holes.set(`${p.x},${p.y}`, `${b.id}.${id}`);
    }
  }
  const out: [string, string][] = [];
  for (const part of doc.parts) {
    const d = defs(part.def);
    // Only modules (and a Nano) plug in; an Uno or a Pi has sockets, not legs.
    if (!d || (d.kind !== 'part' && !d.pluggable)) continue;
    const g = geom(part, defs);
    for (const id in g.pins) {
      const p = g.pins[id]!;
      if (p.pin.terminal || p.pin.kind === 'port') continue;
      const hole = holes.get(`${p.x},${p.y}`);
      if (hole) out.push([`${part.id}.${id}`, hole]);
    }
  }
  return out;
}

export function pinDef(doc: Diagram, defs: DefLookup, key: string): PinDef | undefined {
  const [pid, pn] = splitKey(key);
  const part = doc.parts.find((p) => p.id === pid);
  return part ? defs(part.def)?.pins.find((p) => p.id === pn) : undefined;
}

export function computeNets(doc: Diagram, defs: DefLookup): NetIndex {
  const parent = new Map<string, string>();
  const find = (k: string): string => {
    let r = k;
    while (parent.get(r) !== r) {
      const up = parent.get(r)!;
      parent.set(r, parent.get(up)!);
      r = up;
    }
    return r;
  };
  const add = (k: string) => {
    if (!parent.has(k)) parent.set(k, k);
  };
  const join = (a: string, b: string) => {
    add(a);
    add(b);
    const ra = find(a);
    const rb = find(b);
    if (ra !== rb) parent.set(ra, rb);
  };
  for (const w of doc.wires) join(w.a, w.b);
  const plugs = findPlugs(doc, defs);
  for (const [a, b] of plugs) join(a, b);
  // A closed fuse, switch or connector joins its two sides (an open switch doesn't).
  for (const part of doc.parts) {
    const d = defs(part.def);
    if (!d?.passes || part.open) continue;
    for (const [a, b] of d.passes) {
      const ka = `${part.id}.${a}`;
      const kb = `${part.id}.${b}`;
      if (parent.has(ka) || parent.has(kb)) join(ka, kb);
    }
  }
  // A breadboard strip joins only what is on it: a strip with nothing on it is no net.
  for (const part of doc.parts) {
    const d = defs(part.def);
    if (!d?.buses) continue;
    for (const bus of d.buses) {
      const used = bus.map((id) => `${part.id}.${id}`).filter((k) => parent.has(k));
      for (let i = 1; i < used.length; i++) join(used[0]!, used[i]!);
    }
  }
  const byRoot = new Map<string, Net>();
  const nets: Net[] = [];
  const of = new Map<string, Net>();
  for (const k of parent.keys()) {
    const r = find(k);
    let net = byRoot.get(r);
    if (!net) {
      net = { id: nets.length + 1, keys: [], kind: 'empty', volts: 0, color: '' };
      byRoot.set(r, net);
      nets.push(net);
    }
    net.keys.push(k);
    of.set(k, net);
  }
  // Colours in the order the wires were made, so adding a wire never repaints the others.
  let signal = 0;
  const seen = new Set<Net>();
  const order: Net[] = [];
  for (const w of doc.wires) {
    const n = of.get(w.a);
    if (n && !seen.has(n)) {
      seen.add(n);
      order.push(n);
    }
  }
  for (const n of nets) if (!seen.has(n)) order.push(n);
  for (const net of order) {
    const pins = net.keys.map((k) => pinDef(doc, defs, k)).filter((p): p is PinDef => !!p && p.kind !== 'hole');
    const sup = pins.filter((p) => p.kind === 'supply');
    const gnd = pins.some((p) => p.kind === 'gnd');
    if (pins.some((p) => p.kind === 'port')) {
      net.kind = 'cable';
      net.color = USB_COLOR;
    } else if (sup.length && gnd) {
      net.kind = 'short';
      net.color = '#ef4444';
    } else if (sup.length) {
      net.kind = 'power';
      net.volts = Math.max(...sup.map((p) => p.volts ?? 0));
      net.color = powerColor(net.volts);
    } else if (gnd) {
      net.kind = 'gnd';
      net.color = GND_COLOR;
    } else if (!pins.length) {
      net.kind = 'empty';
      net.color = '#94a3b8';
    } else {
      net.kind = pins.some((p) => p.kind === 'motor-in' || p.kind === 'motor-out') ? 'motor' : pins.some((p) => p.kind === 'speaker-in' || p.kind === 'speaker-out') ? 'speaker' : 'signal';
      net.color = SIGNAL_COLORS[signal % SIGNAL_COLORS.length]!;
      signal += 1;
    }
  }
  return { of, nets, plugs };
}
