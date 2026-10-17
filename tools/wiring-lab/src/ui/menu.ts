/** What the right-click menus hold, for a part, a pin, a wire and the empty mat. */
import { pairVerdict } from '../lib/checks';
import type { ConnectOption } from '../lib/connect';
import { splitKey } from '../lib/types';
import { COLOR_NAMES } from '../lib/build';
import { lab } from './lab.svelte';

export interface MenuItem {
  label: string;
  /** A second line, or why the item can't be used. */
  hint?: string;
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  /** A colour chip in front of the label. */
  swatch?: string;
  /** ok / warn marks for pins. */
  mark?: 'ok' | 'warn';
  action?: () => void;
  children?: MenuItem[];
  /** Wires drawn dashed on the canvas while the item is hovered. */
  preview?: ConnectOption;
  separator?: boolean;
}

export function openMenu(x: number, y: number, items: MenuItem[]): void {
  lab.preview = null;
  lab.menu = { x, y, items };
}

export function connectItems(fromId: string): MenuItem[] {
  const items = lab.doc.parts
    .filter((p) => p.id !== fromId && lab.defs(p.def)?.kind !== 'breadboard')
    .map((o): MenuItem => {
      const r = lab.options(fromId, o.id);
      const sub = lab.defs(o.def)?.name ?? '';
      if (!r.ok) return { label: o.name, hint: r.reason, disabled: true };
      if (r.options.length === 1) {
        const opt = r.options[0]!;
        return { label: o.name, hint: `${sub} · ${opt.label}${opt.notes.length ? ' · ' + opt.notes[0] : ''}`, action: () => lab.apply(opt), preview: opt };
      }
      return {
        label: o.name,
        hint: `${sub} · ${r.options.length} ways`,
        children: r.options.map((opt) => ({ label: opt.label, hint: [`${opt.wires.length} wires`, ...opt.notes].join(' · '), action: () => lab.apply(opt), preview: opt })),
      };
    });
  // The parts it can't join go in one submenu at the end, each saying why.
  const can = items.filter((i) => !i.disabled);
  const cannot = items.filter((i) => i.disabled).map((i) => ({ ...i, disabled: false, action: undefined, hint: i.hint, label: i.label }));
  if (!items.length) return [{ label: 'Nothing else on the drawing yet', disabled: true }];
  if (!can.length) return [{ label: 'Nothing on the drawing fits', disabled: true }, { label: `Why not (${cannot.length})`, children: cannot }];
  return cannot.length ? [...can, { separator: true, label: '' }, { label: `Can't connect (${cannot.length})`, children: cannot }] : can;
}

export function partMenu(id: string): MenuItem[] {
  const p = lab.part(id);
  if (!p) return [];
  const d = lab.defs(p.def);
  const wired = lab.doc.wires.some((w) => splitKey(w.a)[0] === id || splitKey(w.b)[0] === id);
  const items: MenuItem[] = [];
  if (d?.kind !== 'breadboard') items.push({ label: 'Connect to', children: connectItems(id) });
  if (d?.kind !== 'breadboard') items.push({ label: 'Rotate', shortcut: 'R', action: () => lab.rotate(id) });
  items.push({ label: 'Duplicate', action: () => lab.duplicate(id) });
  if (wired) items.push({ label: 'Remove its wires', action: () => lab.disconnect(id) });
  items.push({ separator: true, label: '' }, { label: `Remove ${p.name}`, shortcut: 'Del', danger: true, action: () => lab.removePart(id) });
  return items;
}

export function pinMenu(key: string): MenuItem[] {
  const [pid] = splitKey(key);
  const items: MenuItem[] = [];
  const good: MenuItem[] = [];
  const doubtful: MenuItem[] = [];
  const usedBy = (k: string) => {
    const w = lab.doc.wires.find((x) => x.a === k || x.b === k);
    if (!w) return undefined;
    const other = w.a === k ? w.b : w.a;
    const [op, opin] = splitKey(other);
    const part = lab.part(op);
    return part ? `${part.name} ${lab.defs(part.def)?.pins.find((p) => p.id === opin)?.label ?? opin}` : other;
  };
  for (const o of lab.doc.parts) {
    if (o.id === pid) continue;
    const d = lab.defs(o.def);
    if (!d || d.kind === 'breadboard') continue;
    const fits = d.pins
      .map((p) => ({ p, k: `${o.id}.${p.id}`, v: pairVerdict(lab.snap, lab.defs, key, `${o.id}.${p.id}`) }))
      .filter(({ v }) => v !== 'bad')
      .map((f) => ({ ...f, by: usedBy(f.k) }))
      // Free pins first, then pins already in use.
      .sort((a, b) => Number(!!a.by) - Number(!!b.by) || (a.v === b.v ? 0 : a.v === 'ok' ? -1 : 1));
    if (!fits.length) continue;
    const ok = fits.filter((f) => f.v === 'ok');
    const entry: MenuItem = {
      label: o.name,
      hint: ok.length ? `${ok.length} pin${ok.length > 1 ? 's' : ''} fit` : `${fits.length} doubtful`,
      children: fits.slice(0, 48).map(({ p, k, v, by }) => ({
        label: p.label,
        hint: by ? `in use: ${by}` : p.place,
        mark: v === 'ok' ? 'ok' : 'warn',
        action: () => lab.connectPins(key, k),
      })),
    };
    (ok.length ? good : doubtful).push(entry);
  }
  const list = [...good];
  if (doubtful.length) list.push({ separator: true, label: '' }, { label: `Doubtful (${doubtful.length})`, hint: 'They take it, but it may not be what you want', children: doubtful });
  items.push({ label: 'Connect to', children: list.length ? list : [{ label: 'No pin on the drawing fits', disabled: true }] });
  items.push({ label: 'Draw a wire from here', hint: 'Then click the other pin', action: () => (lab.connecting = key) });
  if (lab.doc.wires.some((w) => w.a === key || w.b === key)) items.push({ separator: true, label: '' }, { label: 'Remove wires on this pin', danger: true, action: () => lab.removeWiresAt(key) });
  return items;
}

const COLOURS = ['', '#ef4444', '#202328', '#3b82f6', '#facc15', '#22c55e', '#f5f5f4', '#f97316', '#c084fc', '#b07a43'];

export function wireMenu(id: string): MenuItem[] {
  const w = lab.doc.wires.find((x) => x.id === id);
  return [
    {
      label: 'Colour',
      children: COLOURS.map((c) => ({
        label: c ? COLOR_NAMES[c] ?? c : 'By what it carries',
        swatch: c || undefined,
        hint: (w?.color ?? '') === c ? 'Current' : undefined,
        action: () => lab.colour(id, c || undefined),
      })),
    },
    { separator: true, label: '' },
    { label: 'Remove wire', shortcut: 'Del', danger: true, action: () => lab.removeWire(id) },
  ];
}

export function matMenu(at: [number, number]): MenuItem[] {
  return [
    {
      label: 'Add a part here',
      children: lab.allDefs.map((d) => ({ label: d.name, hint: d.covers, action: () => lab.addPart(d.id, at) })),
    },
    { label: 'Arrange everything', action: () => lab.arrange() },
    { label: 'Fit to the window', action: () => lab.fit() },
  ];
}
