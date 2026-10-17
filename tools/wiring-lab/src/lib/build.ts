/** The diagram as wiring steps to tick off with the parts in your hand. */
import { pinName, pinRef } from './checks';
import type { NetIndex } from './nets';
import type { DefLookup, Diagram } from './types';
import { splitKey } from './types';

export interface Step {
  /** The key it is ticked under in `Diagram.done`. */
  id: string;
  text: string;
  detail: string;
  color: string;
  wires: string[];
  parts: string[];
}

export interface StepGroup {
  title: string;
  sub: string;
  steps: Step[];
}

export const COLOR_NAMES: Record<string, string> = {
  '#ef4444': 'Red', '#f97316': 'Orange', '#be123c': 'Red (thick)', '#202328': 'Black', '#3b82f6': 'Blue',
  '#facc15': 'Yellow', '#f5f5f4': 'White', '#c084fc': 'Purple', '#22d3ee': 'Cyan', '#f472b6': 'Pink',
  '#a3e635': 'Lime', '#b07a43': 'Brown', '#94a3b8': 'Grey', '#22c55e': 'Green', '#60a5fa': 'Light blue',
  '#fde047': 'Light yellow', '#e879f9': 'Magenta', '#2dd4bf': 'Teal', '#fb7185': 'Rose', '#8b939c': 'USB',
};

export function wireColor(doc: Diagram, nets: NetIndex, wireId: string): string {
  const w = doc.wires.find((x) => x.id === wireId);
  if (!w) return '#94a3b8';
  return w.color ?? nets.of.get(w.a)?.color ?? '#94a3b8';
}

export function buildSteps(doc: Diagram, defs: DefLookup, nets: NetIndex): StepGroup[] {
  const groups: StepGroup[] = [];
  const done = new Set<string>();
  const meet = new Map<string, number>();
  for (const w of doc.wires) for (const k of [w.a, w.b]) meet.set(k, (meet.get(k) ?? 0) + 1);
  const rank = (id: string) => {
    const w = doc.wires.find((x) => x.id === id)!;
    const k = nets.of.get(w.a)?.kind;
    return k === 'usb' ? -1 : k === 'gnd' ? 0 : k === 'power' ? 1 : 2;
  };
  const order = doc.parts.slice().sort((a, b) => {
    const ka = defs(a.def)?.kind;
    const kb = defs(b.def)?.kind;
    // Parts first, so each part's wires are listed under it; what is left joins boards.
    const r = (k?: string) => (k === 'part' ? 0 : k === 'breadboard' ? 1 : 2);
    return r(ka) - r(kb);
  });
  for (const part of order) {
    const d = defs(part.def);
    if (!d) continue;
    const steps: Step[] = [];
    const plugs = nets.plugs.filter(([p]) => splitKey(p)[0] === part.id);
    if (plugs.length) {
      const holes = plugs.map(([p, h]) => `${pinRef(doc, defs, p)?.pin.label} in ${splitKey(h)[1]}`).join(', ');
      steps.push({ id: `plug:${part.id}`, text: `Plug ${part.name} into the breadboard`, detail: holes, color: '#e2bd52', wires: [], parts: [part.id] });
    }
    const mine = doc.wires.filter((w) => !done.has(w.id) && (splitKey(w.a)[0] === part.id || splitKey(w.b)[0] === part.id)).sort((x, y) => rank(x.id) - rank(y.id));
    for (const w of mine) {
      done.add(w.id);
      const [near, far] = splitKey(w.a)[0] === part.id ? [w.a, w.b] : [w.b, w.a];
      const farRef = pinRef(doc, defs, far);
      const nearRef = pinRef(doc, defs, near);
      const color = wireColor(doc, nets, w.id);
      const kind = nets.of.get(w.a)?.kind;
      const where = farRef?.pin.place ?? '';
      const shared = (meet.get(far) ?? 0) > 1 && farRef?.pin.kind !== 'hole' ? ` · ${meet.get(far)} wires meet here: use a Wago or a breadboard rail` : '';
      const what = kind === 'usb' ? 'USB cable' : `${COLOR_NAMES[color] ?? 'A'} wire`;
      steps.push({
        id: w.id,
        text: `${nearRef?.pin.label ?? near} → ${pinName(doc, defs, far)}`,
        detail: `${what}${where ? ' · ' + where : ''}${shared}`,
        color,
        wires: [w.id],
        parts: [part.id, splitKey(far)[0]],
      });
    }
    if (steps.length) groups.push({ title: part.name, sub: d.name, steps });
  }
  return groups;
}
