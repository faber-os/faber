/** What Faber refuses or warns about before anyone plugs in a battery. */
import type { NetIndex } from './nets';
import { pinDef } from './nets';
import type { Routing } from './router';
import type { Cap, DefLookup, Diagram, Part, PartDef, PinDef } from './types';
import { splitKey } from './types';

export type Severity = 'error' | 'warn';

export interface Issue {
  sev: Severity;
  text: string;
  more: string;
  parts: string[];
  wires: string[];
}

export type Verdict = 'ok' | 'warn' | 'bad';

interface PinRef {
  key: string;
  part: Part;
  def: PartDef;
  pin: PinDef;
}

export function pinRef(doc: Diagram, defs: DefLookup, key: string): PinRef | undefined {
  const [pid, pn] = splitKey(key);
  const part = doc.parts.find((p) => p.id === pid);
  const def = part ? defs(part.def) : undefined;
  const pin = def?.pins.find((p) => p.id === pn);
  return part && def && pin ? { key, part, def, pin } : undefined;
}

export function pinName(doc: Diagram, defs: DefLookup, key: string): string {
  const r = pinRef(doc, defs, key);
  return r ? `${r.part.name} ${r.pin.label}` : key;
}

export function capName(c: Cap): string {
  return { pwm: 'PWM', analog: 'analog in', sda: 'I²C SDA', scl: 'I²C SCL', int: 'an interrupt', io: 'a digital pin', tx: 'serial TX', rx: 'serial RX' }[c];
}

const POWER_IN = new Set(['vcc', 'vin', 'power-in']);

/** Whether a wire between two pins makes sense, before it is made (pins glow by this). */
export function pairVerdict(doc: Diagram, defs: DefLookup, ka: string, kb: string): Verdict {
  const A = pinRef(doc, defs, ka);
  const B = pinRef(doc, defs, kb);
  if (!A || !B || ka === kb) return 'bad';
  if (A.pin.kind === 'hole' || B.pin.kind === 'hole') return A.part === B.part ? 'warn' : 'ok';
  if (A.part === B.part) return 'bad';
  const kinds: string[] = [A.pin.kind, B.pin.kind];
  const has = (k: string) => kinds.includes(k);
  const other = (k: string) => (A.pin.kind === k ? B : A);
  if (has('usb-host') || has('usb-device')) return has('usb-host') && has('usb-device') ? 'ok' : 'bad';
  if (A.pin.kind === 'gnd' && B.pin.kind === 'gnd') return 'ok';
  if (has('gnd')) {
    const o = other('gnd');
    if (o.pin.kind === 'supply' || POWER_IN.has(o.pin.kind)) return 'bad';
    return o.pin.kind === 'sig' && o.pin.needs ? 'bad' : 'warn';
  }
  if (has('motor-in')) return has('motor-out') ? 'ok' : 'bad';
  if (has('motor-out')) return 'bad';
  if (A.pin.kind === 'supply' && B.pin.kind === 'supply') return Math.abs((A.pin.volts ?? 0) - (B.pin.volts ?? 0)) < 0.2 ? 'warn' : 'bad';
  if (has('supply')) {
    const s = other(kinds.find((k) => k !== 'supply')!);
    const c = s === A ? B : A;
    if (POWER_IN.has(c.pin.kind)) return inRange(s.pin.volts ?? 0, c.pin) ? 'ok' : 'bad';
    return 'bad';
  }
  if (has('sig') && has('io')) {
    const s = A.pin.kind === 'sig' ? A : B;
    const io = s === A ? B : A;
    if (!(s.pin.needs ?? ['io']).every((n) => io.pin.caps?.includes(n))) return 'bad';
    if (s.pin.drives && s.pin.drives > (io.pin.logic ?? 5) + 0.3) return 'bad';
    return io.pin.serial ? 'warn' : 'ok';
  }
  if (A.pin.kind === 'io' && B.pin.kind === 'io') {
    if (Math.abs((A.pin.logic ?? 5) - (B.pin.logic ?? 5)) > 0.3) return 'bad';
    return 'warn';
  }
  if (A.pin.kind === 'sig' && B.pin.kind === 'sig') return 'warn';
  return 'bad';
}

function inRange(v: number, p: PinDef): boolean {
  return v >= (p.min ?? 0) - 0.05 && v <= (p.max ?? 99) + 0.05;
}

export function runChecks(doc: Diagram, defs: DefLookup, nets: NetIndex, routing?: Routing): Issue[] {
  const out: Issue[] = [];
  const touched = new Set<string>();
  for (const w of doc.wires) {
    touched.add(w.a);
    touched.add(w.b);
  }
  for (const [p] of nets.plugs) touched.add(p);
  const wiresOf = (keys: string[]) => doc.wires.filter((w) => keys.includes(w.a) || keys.includes(w.b)).map((w) => w.id);

  for (const net of nets.nets) {
    const refs = net.keys.map((k) => pinRef(doc, defs, k)).filter((r): r is PinRef => !!r && r.pin.kind !== 'hole');
    if (!refs.length) continue;
    const parts = [...new Set(refs.map((r) => r.part.id))];
    const wires = wiresOf(net.keys);
    const tag = (r: PinRef) => `${r.part.name} ${r.pin.label}`;
    const sup = refs.filter((r) => r.pin.kind === 'supply');
    const gnd = refs.filter((r) => r.pin.kind === 'gnd');
    const ios = refs.filter((r) => r.pin.kind === 'io');
    const sigs = refs.filter((r) => r.pin.kind === 'sig');

    // One part's own pins joined (a module turned along a breadboard strip, say).
    const byPart = new Map<string, PinRef[]>();
    for (const r of refs) byPart.set(r.part.id, [...(byPart.get(r.part.id) ?? []), r]);
    for (const [, rs] of byPart) {
      const d = rs[0]!.def;
      if (rs.length > 1 && d.kind !== 'board' && !rs.every((r) => r.pin.kind === 'gnd') && !rs.every((r) => r.pin.kind === 'sig' && !r.pin.out)) {
        out.push({ sev: 'error', text: `${rs[0]!.part.name}: ${rs.map((r) => r.pin.label).join(' and ')} are joined`, more: 'Two of its own pins share a net. On a breadboard: are they in the same strip?', parts: [rs[0]!.part.id], wires });
      }
    }
    if (sup.length && gnd.length) {
      out.push({ sev: 'error', text: `Short circuit: ${tag(sup[0]!)} reaches ground`, more: 'Nothing may join a supply to GND. Remove the wire that crosses over.', parts, wires });
    }
    const volts = [...new Set(sup.map((s) => s.pin.volts ?? 0))].sort((a, b) => a - b);
    if (volts.length > 1 && volts[volts.length - 1]! - volts[0]! > 0.2) {
      out.push({ sev: 'error', text: `Two supplies fight: ${volts.map((v) => `${v} V`).join(' and ')} are joined`, more: sup.map(tag).join(', '), parts, wires });
    }
    for (const c of refs.filter((r) => POWER_IN.has(r.pin.kind))) {
      if (!sup.length) {
        if (!(gnd.length && sup.length)) out.push({ sev: 'error', text: `${tag(c)} has no power`, more: `Wire it to a supply (${c.pin.min}–${c.pin.max} V).`, parts: [c.part.id], wires });
        continue;
      }
      const v = Math.max(...sup.map((s) => s.pin.volts ?? 0));
      if (v > (c.pin.max ?? 99) + 0.05) out.push({ sev: 'error', text: `${tag(c)} gets ${v} V, more than its ${c.pin.max} V`, more: 'That can destroy it. Use a supply in its range.', parts: [c.part.id], wires });
      else if (v < (c.pin.min ?? 0) - 0.05) out.push({ sev: 'warn', text: `${tag(c)} gets ${v} V, less than its ${c.pin.min} V`, more: 'It may not start, or may read wrong.', parts: [c.part.id], wires });
    }
    // Board pins at different logic levels joined directly.
    const levels = [...new Set(ios.map((r) => r.pin.logic ?? 5))];
    if (levels.length > 1) {
      out.push({ sev: 'error', text: `${ios.map(tag).join(' and ')}: ${levels.join(' V and ')} V logic joined`, more: 'Put a level shifter between them, or the 3.3 V pin may die.', parts, wires });
    } else if (ios.length > 1) {
      const tx = ios.filter((r) => r.pin.caps?.includes('tx'));
      if (tx.length > 1) out.push({ sev: 'error', text: `TX to TX: ${tx.map(tag).join(' and ')}`, more: 'Cross them: one side\'s TX goes to the other side\'s RX.', parts, wires });
      else out.push({ sev: 'warn', text: `One net joins ${ios.map((r) => r.pin.label).join(' and ')}`, more: 'Two board pins wired together fight when both are outputs.', parts, wires });
    }
    for (const s of sigs) {
      if (sup.length || gnd.length) {
        out.push({ sev: 'error', text: `${tag(s)} is wired to ${sup.length ? 'power' : 'ground'}`, more: 'A signal pin goes to a board pin.', parts: [s.part.id], wires });
        continue;
      }
      if (!ios.length) {
        const throughShifter = s.def.channels?.some((c) => c.includes(s.pin.id));
        if (!s.pin.optional && !throughShifter) out.push({ sev: 'warn', text: `${tag(s)} doesn't reach a board pin`, more: 'Signals go to a board, or the code can\'t see them.', parts: [s.part.id], wires });
        continue;
      }
      const io = ios[0]!;
      const missing = (s.pin.needs ?? ['io']).filter((n) => !io.pin.caps?.includes(n));
      if (missing.length) {
        const fit = io.def.pins.filter((p) => p.kind === 'io' && (s.pin.needs ?? []).every((n) => p.caps?.includes(n))).map((p) => p.label);
        out.push({ sev: 'error', text: `${tag(s)} needs ${missing.map(capName).join(', ')}; ${io.pin.label} can't`, more: fit.length ? `Pins that can: ${fit.slice(0, 12).join(', ')}${fit.length > 12 ? ' …' : ''}` : 'This board has no such pin.', parts: [s.part.id, io.part.id], wires });
      }
      if (s.pin.drives && s.pin.drives > (io.pin.logic ?? 5) + 0.3) {
        out.push({ sev: 'error', text: `${tag(s)} sends ${s.pin.drives} V into ${tag(io)} (a ${io.pin.logic} V pin)`, more: 'Put a level shifter or a divider between them, or the board pin may die.', parts: [s.part.id, io.part.id], wires });
      }
      if (io.pin.serial) out.push({ sev: 'warn', text: `${tag(io)} is the board's USB serial`, more: 'Uploads and the Faber link use it. Pick another pin.', parts: [io.part.id], wires });
    }
    const outs = sigs.filter((s) => s.pin.out);
    if (outs.length > 1 && !outs.every((s) => (s.pin.needs ?? []).some((n) => n === 'sda' || n === 'scl'))) {
      out.push({ sev: 'warn', text: `${outs.length} outputs on one line: ${outs.map(tag).join(', ')}`, more: 'They fight each other. Give each its own pin.', parts, wires });
    }
    for (const m of refs.filter((r) => r.pin.kind === 'motor-in')) {
      if (!refs.some((r) => r.pin.kind === 'motor-out')) out.push({ sev: 'error', text: `${tag(m)} must go to a motor driver`, more: 'A motor pulls amps; a board pin gives 20 mA.', parts: [m.part.id], wires });
    }
  }

  // Serial through a level shifter: TX must meet RX on the far side.
  for (const part of doc.parts) {
    const d = defs(part.def);
    for (const [lo, hi] of d?.channels ?? []) {
      const ends = [lo, hi].map((id) => {
        const net = nets.of.get(`${part.id}.${id}`);
        return net?.keys.map((k) => pinRef(doc, defs, k)).find((r) => r?.pin.kind === 'io');
      });
      const [a, b] = ends;
      if (a && b) {
        const both = (c: Cap) => a.pin.caps?.includes(c) && b.pin.caps?.includes(c);
        if (both('tx') || both('rx')) {
          const c = both('tx') ? 'TX' : 'RX';
          out.push({ sev: 'error', text: `${c} to ${c} through ${part.name}: ${a.part.name} ${a.pin.label} and ${b.part.name} ${b.pin.label}`, more: 'Cross them: TX on one side, RX on the other.', parts: [part.id, a.part.id, b.part.id], wires: wiresOf([`${part.id}.${lo}`, `${part.id}.${hi}`]) });
        }
      }
    }
  }

  // Every pin a part needs, connected.
  for (const part of doc.parts) {
    const d = defs(part.def);
    if (!d || d.kind !== 'part') continue;
    const missing = d.pins.filter((p) => !p.optional && !touched.has(`${part.id}.${p.id}`)).map((p) => p.label);
    if (missing.length) out.push({ sev: 'warn', text: `${part.name}: ${missing.join(', ')} not connected`, more: d.name, parts: [part.id], wires: [] });
  }

  // Every power source's minus must meet every board's ground, or signals have no reference.
  const boardGnds = doc.parts.filter((p) => defs(p.def)?.kind === 'board').map((p) => ({ p, k: defs(p.def)!.pins.filter((x) => x.kind === 'gnd').map((x) => `${p.id}.${x.id}`) }));
  const groundNets = (keys: string[]) => new Set(keys.map((k) => nets.of.get(k)).filter((n) => n));
  const reach = (from: Set<unknown>, keys: string[]) => keys.some((k) => from.has(nets.of.get(k)));
  for (const part of doc.parts) {
    const d = defs(part.def);
    if (!d || d.kind !== 'part' || !d.pins.some((x) => x.kind === 'supply')) continue;
    const g = d.pins.filter((x) => x.kind === 'gnd').map((x) => `${part.id}.${x.id}`);
    const mine = groundNets(g);
    if (!mine.size) continue;
    if (boardGnds.length && !boardGnds.some((b) => reach(mine, b.k))) {
      out.push({ sev: 'warn', text: `${part.name}: its − doesn't meet a board's GND`, more: 'Join the grounds, or drivers read the board\'s signals wrong.', parts: [part.id], wires: [] });
    }
  }
  // Two boards wired by signals must share ground (a USB cable does that).
  const boards = doc.parts.filter((p) => defs(p.def)?.kind === 'board');
  for (let i = 0; i < boards.length; i++) {
    for (let j = i + 1; j < boards.length; j++) {
      const A = boards[i]!;
      const B = boards[j]!;
      const linked = nets.nets.some((n) => n.kind !== 'gnd' && n.kind !== 'usb' && n.keys.some((k) => splitKey(k)[0] === A.id) && n.keys.some((k) => splitKey(k)[0] === B.id));
      const viaShifter = doc.parts.some((s) => defs(s.def)?.channels && [A, B].every((b) => nets.nets.some((n) => n.keys.some((k) => splitKey(k)[0] === s.id) && n.keys.some((k) => splitKey(k)[0] === b.id && pinDef(doc, defs, k)?.kind === 'io'))));
      if (!linked && !viaShifter) continue;
      const usb = nets.nets.some((n) => n.kind === 'usb' && n.keys.some((k) => splitKey(k)[0] === A.id) && n.keys.some((k) => splitKey(k)[0] === B.id));
      const ga = groundNets(boardGnds.find((b) => b.p === A)!.k);
      if (!usb && !reach(ga, boardGnds.find((b) => b.p === B)!.k)) {
        out.push({ sev: 'warn', text: `${A.name} and ${B.name} talk but share no ground`, more: 'Run a GND wire between them, or a USB cable.', parts: [A.id, B.id], wires: [] });
      }
    }
  }
  if (routing) {
    for (const w of doc.wires) {
      if (routing.routes.get(w.id)?.failed) out.push({ sev: 'warn', text: `No clear path for ${pinName(doc, defs, w.a)} → ${pinName(doc, defs, w.b)}`, more: 'Move the parts apart, or press Arrange.', parts: [], wires: [w.id] });
    }
  }
  const seen = new Set<string>();
  return out
    .filter((i) => {
      const k = i.sev + i.text;
      if (seen.has(k)) return false;
      seen.add(k);
      return true;
    })
    .sort((a, b) => (a.sev === b.sev ? 0 : a.sev === 'error' ? -1 : 1));
}
