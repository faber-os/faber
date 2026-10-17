/**
 * Ways out of a check's issue. Each fix is data (wires to remove, add or move, parts to add), so
 * it can be shown before it is applied and undone after. A fix is only offered when applying it
 * makes the issue go away; where there are several, the person picks.
 */
import { capName, inRange, pinName, pinRef, POWER_IN, runChecks, type Issue } from './checks';
import { centre, connectOptions, context, levelBridge, partnerRx, pickGnd, pickIo, type Ctx } from './connect';
import { geom } from './geometry';
import { adviceFixes } from './advice';
import { computeNets } from './nets';
import type { DefLookup, Diagram, Dir, Part, PinDef } from './types';
import { splitKey } from './types';

export interface Fix {
  label: string;
  hint?: string;
  remove?: string[];
  add?: [string, string][];
  addParts?: Part[];
  /** Wire ends to move: the end at `from` goes to `to`. */
  move?: { wire: string; from: string; to: string }[];
  rotate?: string;
  arrange?: boolean;
  /** Board pins the code sets to INPUT_PULLUP. */
  pullups?: string[];
}

/** Applies a fix to a diagram (in place). New wires take ids from `doc.seq`. */
export function applyFix(doc: Diagram, fix: Fix): void {
  if (fix.addParts) doc.parts.push(...fix.addParts.map((p) => ({ ...p })));
  if (fix.remove) doc.wires = doc.wires.filter((w) => !fix.remove!.includes(w.id));
  for (const m of fix.move ?? []) {
    const w = doc.wires.find((x) => x.id === m.wire);
    if (!w) continue;
    if (w.a === m.from) w.a = m.to;
    else if (w.b === m.from) w.b = m.to;
  }
  for (const [a, b] of fix.add ?? []) doc.wires.push({ id: `w${doc.seq++}`, a, b });
  if (fix.pullups) doc.pullups = [...new Set([...(doc.pullups ?? []), ...fix.pullups])];
  if (fix.rotate) {
    const p = doc.parts.find((x) => x.id === fix.rotate);
    if (p) p.rot = ((p.rot + 1) & 3) as Dir;
  }
}

/** `R1`, `R2` … the first free name from the `n`th on. */
function freeName(doc: Diagram, prefix: string, n: number): string {
  const names = new Set(doc.parts.map((p) => p.name));
  let i = 1;
  let found = 0;
  for (;;) {
    if (!names.has(`${prefix}${i}`) && ++found === n) return `${prefix}${i}`;
    i++;
  }
}

const clone = (doc: Diagram): Diagram => JSON.parse(JSON.stringify(doc)) as Diagram;

/** Whether the fix makes this issue (same code, same first pin) go away. */
function works(doc: Diagram, defs: DefLookup, issue: Issue, fix: Fix): boolean {
  if (fix.arrange) return true;
  const next = clone(doc);
  applyFix(next, fix);
  const after = runChecks(next, defs, computeNets(next, defs));
  return !after.some((i) => i.code === issue.code && i.text === issue.text);
}

function wireTouching(doc: Diagram, key: string, among?: string[]): string | undefined {
  return doc.wires.find((w) => (w.a === key || w.b === key) && (!among || among.includes(w.id)))?.id;
}

function label(doc: Diagram, defs: DefLookup, wireId: string): string {
  const w = doc.wires.find((x) => x.id === wireId)!;
  return `${pinName(doc, defs, w.a)} – ${pinName(doc, defs, w.b)}`;
}

/** Board pins that could take this part pin instead, best first. */
function betterPins(ctx: Ctx, sig: PinDef, host: Part, count: number): string[] {
  const out: string[] = [];
  const taken = new Set<string>();
  const near = centre(ctx, host);
  for (let i = 0; i < count; i++) {
    const r = pickIo(ctx, host, ctx.defs(host.def)!, sig, near, taken);
    if (typeof r !== 'string') break;
    out.push(r);
    taken.add(r);
  }
  return out;
}

function supplies(ctx: Ctx, sink: PinDef, except?: string): { key: string; volts: number }[] {
  const out: { key: string; volts: number }[] = [];
  for (const part of ctx.doc.parts) {
    const d = ctx.defs(part.def);
    for (const p of d?.pins ?? []) {
      if (p.kind !== 'supply' || p.inlet) continue;
      const key = `${part.id}.${p.id}`;
      if (key === except || !inRange(p.volts ?? 0, sink)) continue;
      out.push({ key, volts: p.volts ?? 0 });
    }
  }
  return out;
}

export function fixesFor(doc: Diagram, defs: DefLookup, issue: Issue): Fix[] {
  const ctx = context(doc, defs);
  const candidates: Fix[] = [];
  const [k0, k1] = issue.keys;
  const r0 = k0 ? pinRef(doc, defs, k0) : undefined;
  const r1 = k1 ? pinRef(doc, defs, k1) : undefined;

  candidates.push(...adviceFixes(doc, defs, issue));
  switch (issue.code) {
    case 'needs-cap':
    case 'serial-pin': {
      if (!r0 || !r1) break;
      const w = wireTouching(doc, r1.key, issue.wires);
      if (!w) break;
      for (const to of betterPins(ctx, r0.pin, r1.part, 3)) {
        const p = pinRef(doc, defs, to)!;
        candidates.push({ label: `Move it to ${r1.part.name} ${p.pin.label}`, hint: p.pin.caps?.filter((c) => c !== 'io').map(capName).join(', ') || 'a plain pin', move: [{ wire: w, from: r1.key, to }] });
      }
      break;
    }
    case 'drives-into':
    case 'logic-joined': {
      if (!r0 || !r1) break;
      // drives-into: k0 is the 5 V part pin, k1 the 3.3 V board pin. logic-joined: k0 is 3.3 V.
      const [lo, hi] = issue.code === 'drives-into' ? [r1.key, r0.key] : [r0.key, r1.key];
      const b = levelBridge({ ...ctx, used: new Set([...ctx.used].filter((k) => k !== lo && k !== hi)) }, lo, hi);
      if (b) {
        const direct = issue.wires.filter((id) => {
          const w = doc.wires.find((x) => x.id === id)!;
          return [w.a, w.b].includes(lo) || [w.a, w.b].includes(hi);
        });
        candidates.push({ label: 'Put a level shifter between', hint: b.addParts.length ? 'Adds a BSS138 shifter' : 'Uses a free channel of the shifter you have', remove: direct, add: b.wires, addParts: b.addParts });
      }
      if (issue.code === 'drives-into') {
        // Or two resistors: a one-way line from a 5 V output only needs its level brought down.
        const w = wireTouching(doc, r1.key, issue.wires);
        const gnd = defs(r1.part.def)?.pins.find((p) => p.kind === 'gnd');
        if (w && gnd) {
          const at = geom(r1.part, defs).pins[r1.pin.id]!;
          const mk = (n: number, ohms: number, dx: number): Part => ({ id: `div${n}${Date.now().toString(36)}`, def: `res?ohms=${ohms}&w=0.25`, name: freeName(doc, 'R', n), x: Math.round((at.x + dx) / 2) * 2, y: Math.round((at.y - 16) / 2) * 2, rot: 0 });
          const top = mk(1, 1800, -14);
          const bottom = mk(2, 3300, 2);
          candidates.push({ label: 'Put a divider between: 1.8 kΩ and 3.3 kΩ', hint: `${r0.pin.drives} V becomes about ${Math.round(((r0.pin.drives ?? 5) * 3300) / 5100 * 100) / 100} V`, remove: [w], add: [[r0.key, `${top.id}.1`], [`${top.id}.2`, r1.key], [r1.key, `${bottom.id}.1`], [`${bottom.id}.2`, `${r1.part.id}.${gnd.id}`]], addParts: [top, bottom] });
        }
        // Or a pin on a 5 V board.
        for (const board of doc.parts.filter((p) => p.id !== r1.part.id && defs(p.def)?.kind === 'board')) {
          const pick = betterPins(ctx, r0.pin, board, 1)[0];
          if (pick && w) candidates.push({ label: `Move it to ${board.name} ${pinRef(doc, defs, pick)!.pin.label}`, hint: 'A 5 V board pin takes it', move: [{ wire: w, from: r1.key, to: pick }] });
        }
      }
      break;
    }
    case 'tx-tx':
    case 'shifter-tx-tx': {
      for (const r of [r1, r0]) {
        if (!r) continue;
        const partner = r.pin.caps?.includes('tx') ? partnerRx(r.def, r.pin) : undefined;
        const w = wireTouching(doc, r.key);
        if (partner && w) candidates.push({ label: `Use ${r.part.name} ${partner.label} (RX) instead`, hint: 'TX goes to RX', move: [{ wire: w, from: r.key, to: `${r.part.id}.${partner.id}` }] });
      }
      break;
    }
    case 'over-voltage':
    case 'under-voltage':
    case 'no-power': {
      if (!r0 || !POWER_IN.has(r0.pin.kind)) break;
      const remove = doc.wires.filter((w) => w.a === r0.key || w.b === r0.key).map((w) => w.id);
      const near = centre(ctx, r0.part);
      const opts = supplies(ctx, r0.pin, k1)
        .map((s) => ({ ...s, d: (() => { const [pid, pn] = splitKey(s.key); const g = geom(doc.parts.find((p) => p.id === pid)!, defs).pins[pn]!; return Math.abs(g.x - near[0]) + Math.abs(g.y - near[1]); })() }))
        .sort((a, b) => Number(ctx.used.has(a.key)) - Number(ctx.used.has(b.key)) || a.d - b.d)
        .slice(0, 3);
      for (const s of opts) {
        const place = pinRef(doc, defs, s.key)?.pin.place;
        candidates.push({ label: `Power it from ${pinName(doc, defs, s.key)}${place ? ` (${place})` : ''}`, hint: `${s.volts} V`, remove, add: [[r0.key, s.key]] });
      }
      break;
    }
    case 'not-connected':
    case 'signal-unrouted': {
      const part = r0?.part;
      if (!part) break;
      const hosts = doc.parts.filter((p) => p.id !== part.id && defs(p.def)?.kind !== 'breadboard');
      for (const h of hosts) {
        const r = connectOptions(doc, defs, part.id, h.id);
        if (!r.ok) continue;
        for (const o of r.options) {
          const useful = o.wires.filter(([a, b]) => issue.keys.includes(a) || issue.keys.includes(b));
          if (!useful.length) continue;
          candidates.push({ label: `Connect it to ${h.name}`, hint: `${useful.length} wire${useful.length > 1 ? 's' : ''}${o.label.includes('Serial') ? ' · ' + o.label : ''}`, add: useful, addParts: o.addParts });
        }
      }
      break;
    }
    case 'no-common-ground':
    case 'boards-no-ground': {
      const from = issue.code === 'no-common-ground' ? k0 : (() => { const a = doc.parts.find((p) => p.id === k0)!; return pickGnd(ctx, a, defs(a.def)!, centre(ctx, a)); })();
      if (!from) break;
      const boards = doc.parts.filter((p) => defs(p.def)?.kind === 'board' && p.id !== splitKey(from)[0] && (issue.code === 'no-common-ground' || p.id === k1));
      for (const b of boards) {
        const g = pickGnd(ctx, b, defs(b.def)!, centre(ctx, doc.parts.find((p) => p.id === splitKey(from)[0])!));
        if (g) candidates.push({ label: `Join it to ${pinName(doc, defs, g)}`, hint: 'One ground wire', add: [[from, g]] });
      }
      break;
    }
    case 'self-joined': {
      const part = r0?.part;
      if (part) candidates.push({ label: `Turn ${part.name} a quarter`, hint: 'So its legs land in different strips', rotate: part.id });
      break;
    }
    case 'no-route':
      return [{ label: 'Arrange everything', hint: 'Spreads the parts so every wire finds a way', arrange: true }];
    default:
      break;
  }
  // Removing one wire, where that alone solves it.
  if (['short', 'supplies-fight', 'board-pins-joined', 'signal-on-power', 'outputs-fight', 'load-on-board', 'cable-mismatch', 'self-joined', 'tx-tx', 'logic-joined', 'over-voltage', 'drives-into'].includes(issue.code)) {
    // For a short, only the wires that reach ground are the mistake, not the battery's own lead.
    const touchesGround = (id: string) => {
      const w = doc.wires.find((x) => x.id === id)!;
      return [w.a, w.b].some((k) => pinRef(doc, defs, k)?.pin.kind === 'gnd');
    };
    const suspects = issue.code === 'short' ? issue.wires.filter(touchesGround) : issue.wires;
    for (const id of suspects.slice(0, 10)) candidates.push({ label: `Remove ${label(doc, defs, id)}`, remove: [id] });
  }
  const out: Fix[] = [];
  for (const f of candidates) {
    if (out.some((o) => o.label === f.label)) continue;
    if (works(doc, defs, issue, f)) out.push(f);
    if (out.length >= 6) break;
  }
  return out;
}
