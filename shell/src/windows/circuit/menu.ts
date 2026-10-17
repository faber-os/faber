/** What the right-click menus hold, for a part, a pin, a wire and the empty mat. */
import { t } from '../../lib/i18n';
import { COLOR_NAMES } from '../../lib/circuit/build';
import { pairVerdict } from '../../lib/circuit/checks';
import { splitKey } from '../../lib/circuit/types';
import type { CircuitStore } from './circuit.svelte';
import type { MenuItem } from './menuItem';

export type { MenuItem } from './menuItem';

const SEP: MenuItem = { separator: true, label: '' };

export function openMenu(lab: CircuitStore, x: number, y: number, items: MenuItem[]): void {
  lab.preview = null;
  lab.menu = { x, y, items };
}

export function connectItems(lab: CircuitStore, fromId: string): MenuItem[] {
  const items = lab.doc.parts
    .filter((p) => p.id !== fromId && lab.defs(p.def)?.kind !== 'breadboard')
    .map((o): MenuItem => {
      const r = lab.options(fromId, o.id);
      const sub = lab.defs(o.def)?.name ?? '';
      if (!r.ok) return { label: o.name, hint: r.reason, disabled: true };
      if (r.options.length === 1) {
        const opt = r.options[0]!;
        return {
          label: o.name,
          hint: [sub, opt.label, ...opt.notes.slice(0, 1)].join(' · '),
          action: () => lab.apply(opt),
          preview: opt,
        };
      }
      return {
        label: o.name,
        hint: [sub, t('circuit.menu.ways', { count: r.options.length })].join(' · '),
        children: r.options.map((opt) => ({
          label: opt.label,
          hint: [t('circuit.menu.wireCount', { count: opt.wires.length }), ...opt.notes].join(
            ' · ',
          ),
          action: () => lab.apply(opt),
          preview: opt,
        })),
      };
    });
  // The parts it can't join go in one submenu at the end, each saying why.
  const can = items.filter((i) => !i.disabled);
  const cannot = items
    .filter((i) => i.disabled)
    .map((i) => ({ ...i, disabled: false, action: undefined }));
  if (!items.length) return [{ label: t('circuit.menu.nothingElse'), disabled: true }];
  if (!can.length)
    return [
      { label: t('circuit.menu.nothingFits'), disabled: true },
      { label: t('circuit.menu.whyNot', { count: cannot.length }), children: cannot },
    ];
  return cannot.length
    ? [
        ...can,
        SEP,
        { label: t('circuit.menu.cantConnect', { count: cannot.length }), children: cannot },
      ]
    : can;
}

export function partMenu(lab: CircuitStore, id: string): MenuItem[] {
  const p = lab.part(id);
  if (!p) return [];
  const d = lab.defs(p.def);
  const many = lab.selParts.length > 1 && lab.selParts.includes(id);
  const ids = many ? lab.selParts : [id];
  const wired = lab.doc.wires.some(
    (w) => ids.includes(splitKey(w.a)[0]) || ids.includes(splitKey(w.b)[0]),
  );
  const items: MenuItem[] = [];
  if (!many && d?.kind !== 'breadboard')
    items.push({ label: t('circuit.menu.connectTo'), children: connectItems(lab, id) });
  if (lab.power && d?.switch) {
    const label = p.open
      ? t('circuit.menu.close')
      : d.switch === 'push'
        ? t('circuit.menu.press')
        : t('circuit.menu.open');
    items.push({ label, hint: t('circuit.menu.powerMode'), action: () => lab.flick(id) });
  }
  if (d?.kind !== 'breadboard') {
    items.push({ label: t('circuit.menu.rotate'), shortcut: 'R', action: () => lab.rotate(ids) });
    items.push({
      label: t('circuit.menu.rotateBack'),
      shortcut: 'Shift+R',
      action: () => lab.rotate(ids, true),
    });
    if (many) {
      items.push({
        label: t('circuit.menu.mirrorH'),
        shortcut: 'H',
        action: () => lab.mirror('h', ids),
      });
      items.push({
        label: t('circuit.menu.mirrorV'),
        shortcut: 'V',
        action: () => lab.mirror('v', ids),
      });
    }
  }
  items.push(SEP);
  items.push({ label: t('circuit.menu.copy'), shortcut: 'Ctrl+C', action: () => lab.copy() });
  items.push({
    label: t('circuit.menu.duplicate'),
    shortcut: 'Ctrl+D',
    action: () => lab.duplicate(),
  });
  if (wired)
    items.push({
      label: many ? t('circuit.menu.removeTheirWires') : t('circuit.menu.removeItsWires'),
      action: () => lab.disconnect(ids),
    });
  items.push(SEP, {
    label: many
      ? t('circuit.menu.removeParts', { count: ids.length })
      : t('circuit.menu.removeNamed', { name: p.name }),
    shortcut: 'Del',
    danger: true,
    action: () => lab.removeParts(ids),
  });
  return items;
}

export function pinMenu(lab: CircuitStore, key: string): MenuItem[] {
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
    return part
      ? `${part.name} ${lab.defs(part.def)?.pins.find((p) => p.id === opin)?.label ?? opin}`
      : other;
  };
  for (const o of lab.doc.parts) {
    if (o.id === pid) continue;
    const d = lab.defs(o.def);
    if (!d || d.kind === 'breadboard') continue;
    const fits = d.pins
      .map((p) => ({
        p,
        k: `${o.id}.${p.id}`,
        v: pairVerdict(lab.snap, lab.defs, key, `${o.id}.${p.id}`),
      }))
      .filter(({ v }) => v !== 'bad')
      .map((f) => ({ ...f, by: usedBy(f.k) }))
      // Free pins first, then pins already in use.
      .sort((a, b) => Number(!!a.by) - Number(!!b.by) || (a.v === b.v ? 0 : a.v === 'ok' ? -1 : 1));
    if (!fits.length) continue;
    const ok = fits.filter((f) => f.v === 'ok');
    const entry: MenuItem = {
      label: o.name,
      hint: ok.length
        ? t('circuit.menu.pinsFit', { count: ok.length })
        : t('circuit.menu.pinsDoubtful', { count: fits.length }),
      children: fits.slice(0, 48).map(({ p, k, v, by }) => ({
        label: p.label,
        hint: by ? t('circuit.menu.inUse', { by }) : p.place,
        mark: v === 'ok' ? 'ok' : 'warn',
        action: () => lab.connectPins(key, k),
      })),
    };
    (ok.length ? good : doubtful).push(entry);
  }
  const list = [...good];
  if (doubtful.length)
    list.push(SEP, {
      label: t('circuit.menu.doubtful', { count: doubtful.length }),
      hint: t('circuit.menu.doubtfulHint'),
      children: doubtful,
    });
  items.push({
    label: t('circuit.menu.connectTo'),
    children: list.length ? list : [{ label: t('circuit.menu.noPinFits'), disabled: true }],
  });
  items.push({
    label: t('circuit.menu.drawWire'),
    hint: t('circuit.menu.drawWireHint'),
    action: () => (lab.connecting = key),
  });
  if (lab.doc.wires.some((w) => w.a === key || w.b === key))
    items.push(SEP, {
      label: t('circuit.menu.removePinWires'),
      danger: true,
      action: () => lab.removeWiresAt(key),
    });
  return items;
}

const COLOURS = [
  '',
  '#ef4444',
  '#202328',
  '#3b82f6',
  '#facc15',
  '#22c55e',
  '#f5f5f4',
  '#f97316',
  '#c084fc',
  '#b07a43',
];

export function wireMenu(lab: CircuitStore, id: string): MenuItem[] {
  const w = lab.doc.wires.find((x) => x.id === id);
  return [
    {
      label: t('circuit.menu.colour'),
      children: COLOURS.map((c) => ({
        label: c ? (COLOR_NAMES[c] ?? c) : t('circuit.menu.colourAuto'),
        swatch: c || undefined,
        hint: (w?.color ?? '') === c ? t('circuit.menu.current') : undefined,
        action: () => lab.colour(id, c || undefined),
      })),
    },
    SEP,
    {
      label: t('circuit.menu.removeWire'),
      shortcut: 'Del',
      danger: true,
      action: () => lab.removeWire(id),
    },
  ];
}

export function matMenu(lab: CircuitStore, at: [number, number]): MenuItem[] {
  return [
    { label: t('circuit.menu.pasteHere'), shortcut: 'Ctrl+V', action: () => lab.paste() },
    { label: t('circuit.menu.selectAll'), shortcut: 'Ctrl+A', action: () => lab.selectAll() },
    SEP,
    {
      label: t('circuit.menu.addPartHere'),
      children: lab.allDefs.map((d) => ({
        label: d.name,
        hint: d.covers,
        action: () => lab.addPart(d.id, at),
      })),
    },
    { label: t('circuit.menu.arrange'), action: () => lab.arrange() },
    { label: t('circuit.menu.fit'), action: () => lab.fit() },
  ];
}
