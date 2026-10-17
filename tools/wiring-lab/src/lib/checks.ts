/**
 * What Faber refuses or warns about before anyone plugs in a battery. Every issue carries a
 * `code` and the pins it is about, so fixes.ts can offer ways out of it.
 */
import type { NetIndex } from './nets';
import { pinDef } from './nets';
import type { Routing } from './router';
import type { Cap, DefLookup, Diagram, Part, PartDef, PinDef } from './types';
import { splitKey } from './types';

export type Severity = 'error' | 'warn';

export type IssueCode =
  | 'self-joined'
  | 'short'
  | 'supplies-fight'
  | 'no-power'
  | 'over-voltage'
  | 'under-voltage'
  | 'hot-regulator'
  | 'logic-joined'
  | 'tx-tx'
  | 'board-pins-joined'
  | 'signal-on-power'
  | 'signal-unrouted'
  | 'needs-cap'
  | 'drives-into'
  | 'serial-pin'
  | 'outputs-fight'
  | 'load-on-board'
  | 'shifter-tx-tx'
  | 'not-connected'
  | 'no-common-ground'
  | 'boards-no-ground'
  | 'cable-mismatch'
  | 'over-budget'
  | 'caution'
  | 'no-route';

export interface Issue {
  sev: Severity;
  code: IssueCode;
  text: string;
  more: string;
  parts: string[];
  wires: string[];
  /** The pins it is about, most important first. */
  keys: string[];
}

export type Verdict = 'ok' | 'warn' | 'bad';

export interface PinRef {
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
  return {
    pwm: 'PWM', analog: 'analog in', sda: 'I²C SDA', scl: 'I²C SCL', int: 'an interrupt', io: 'a digital pin',
    tx: 'serial TX', rx: 'serial RX', sck: 'SPI SCK', mosi: 'SPI MOSI', miso: 'SPI MISO', audio: 'audio',
  }[c];
}

export const POWER_IN = new Set(['vcc', 'vin', 'power-in']);
const LOAD_IN = new Set(['motor-in', 'speaker-in']);
const LOAD_OUT = new Set(['motor-out', 'speaker-out']);

export function inRange(v: number, p: PinDef): boolean {
  return v >= (p.min ?? 0) - 0.05 && v <= (p.max ?? 99) + 0.05;
}

/** Whether a wire between two pins makes sense, before it is made (pins glow by this). */
export function pairVerdict(doc: Diagram, defs: DefLookup, ka: string, kb: string): Verdict {
  const A = pinRef(doc, defs, ka);
  const B = pinRef(doc, defs, kb);
  if (!A || !B || ka === kb) return 'bad';
  if (A.pin.kind === 'hole' || B.pin.kind === 'hole') return A.part === B.part ? 'warn' : 'ok';
  if (A.part === B.part) return 'bad';
  const a = A.pin;
  const b = B.pin;
  const kinds: string[] = [a.kind, b.kind];
  const has = (k: string) => kinds.includes(k);
  const other = (k: string) => (a.kind === k ? B : A);
  if (has('port')) {
    if (a.kind !== 'port' || b.kind !== 'port' || a.port !== b.port || !!a.host === !!b.host) return 'bad';
    const host = a.host ? a : b;
    const dev = a.host ? b : a;
    if (dev.min !== undefined && host.volts !== undefined && !inRange(host.volts, dev)) return 'bad';
    return 'ok';
  }
  if (has('pass')) {
    const o = other('pass').pin;
    if (o.kind === 'sig' || o.kind === 'io') return 'warn';
    return 'ok';
  }
  if (a.kind === 'gnd' && b.kind === 'gnd') return 'ok';
  if (has('gnd')) {
    const o = other('gnd').pin;
    if (o.kind === 'supply' || POWER_IN.has(o.kind)) return 'bad';
    return o.kind === 'sig' && o.needs ? 'bad' : 'warn';
  }
  for (const [i, o] of [['motor-in', 'motor-out'], ['speaker-in', 'speaker-out']] as const) {
    if (has(i)) return has(o) ? 'ok' : 'bad';
  }
  if (kinds.some((k) => LOAD_OUT.has(k))) return 'bad';
  if (a.kind === 'supply' && b.kind === 'supply') return Math.abs((a.volts ?? 0) - (b.volts ?? 0)) < 0.2 ? 'warn' : 'bad';
  if (has('supply')) {
    const s = a.kind === 'supply' ? A : B;
    const c = s === A ? B : A;
    if (!POWER_IN.has(c.pin.kind)) return 'bad';
    const v = s.pin.volts ?? 0;
    if (inRange(v, c.pin)) return 'ok';
    return c.pin.limit !== undefined && v <= c.pin.limit ? 'warn' : 'bad';
  }
  if (has('sig') && has('io')) {
    const s = a.kind === 'sig' ? A : B;
    const io = s === A ? B : A;
    if (!(s.pin.needs ?? ['io']).every((n) => io.pin.caps?.includes(n))) return 'bad';
    if (s.pin.drives && s.pin.drives > (io.pin.logic ?? 5) + 0.3) return 'bad';
    return io.pin.serial || s.pin.caution ? 'warn' : 'ok';
  }
  if (a.kind === 'io' && b.kind === 'io') {
    if (Math.abs((a.logic ?? 5) - (b.logic ?? 5)) > 0.3) return 'bad';
    const tx = (p: PinDef) => p.caps?.includes('tx');
    const rx = (p: PinDef) => p.caps?.includes('rx');
    if ((tx(a) && rx(b)) || (rx(a) && tx(b))) return 'ok';
    return 'warn';
  }
  if (a.kind === 'sig' && b.kind === 'sig') return 'warn';
  return 'bad';
}

export function runChecks(doc: Diagram, defs: DefLookup, nets: NetIndex, routing?: Routing): Issue[] {
  const out: Issue[] = [];
  const push = (i: Issue) => out.push(i);
  const touched = new Set<string>();
  for (const w of doc.wires) {
    touched.add(w.a);
    touched.add(w.b);
  }
  for (const [p] of nets.plugs) touched.add(p);
  const wiresOf = (keys: string[]) => doc.wires.filter((w) => keys.includes(w.a) || keys.includes(w.b)).map((w) => w.id);

  for (const net of nets.nets) {
    const refs = net.keys.map((k) => pinRef(doc, defs, k)).filter((r): r is PinRef => !!r && r.pin.kind !== 'hole' && r.pin.kind !== 'pass');
    if (!refs.length) continue;
    const parts = [...new Set(refs.map((r) => r.part.id))];
    const wires = wiresOf(net.keys);
    const tag = (r: PinRef) => `${r.part.name} ${r.pin.label}`;
    const sup = refs.filter((r) => r.pin.kind === 'supply' || (r.pin.kind === 'port' && r.pin.host && r.pin.volts !== undefined));
    const gnd = refs.filter((r) => r.pin.kind === 'gnd');
    const ios = refs.filter((r) => r.pin.kind === 'io');
    const sigs = refs.filter((r) => r.pin.kind === 'sig');
    const ports = refs.filter((r) => r.pin.kind === 'port');

    if (ports.length) {
      const types = new Set(ports.map((p) => p.pin.port));
      const hosts = ports.filter((p) => p.pin.host);
      if (types.size > 1 || hosts.length !== 1 || ports.length !== 2 || refs.length !== ports.length) {
        push({ sev: 'error', code: 'cable-mismatch', text: `A cable that can't be: ${refs.map(tag).join(' – ')}`, more: 'A cable joins one host socket (a Pi\'s USB port) to one device socket of the same kind.', parts, wires, keys: refs.map((r) => r.key) });
        continue;
      }
      const dev = ports.find((p) => !p.pin.host)!;
      const host = hosts[0]!;
      if (dev.pin.min !== undefined && host.pin.volts !== undefined && !inRange(host.pin.volts, dev.pin)) {
        const over = host.pin.volts > (dev.pin.max ?? 99);
        push({ sev: over ? 'error' : 'warn', code: over ? 'over-voltage' : 'under-voltage', text: `${tag(dev)} gets ${host.pin.volts} V, ${over ? 'more' : 'less'} than its ${over ? dev.pin.max : dev.pin.min} V`, more: over ? 'That can destroy it.' : 'It may not start, or brown out under load.', parts: [dev.part.id], wires, keys: [dev.key, host.key] });
      }
      if (dev.pin.ma && host.pin.maxMa && dev.pin.ma > host.pin.maxMa) {
        push({ sev: 'warn', code: 'over-budget', text: `${tag(host)} gives ${host.pin.maxMa} mA; ${dev.part.name} wants ${dev.pin.ma} mA`, more: 'Feed it from its own supply.', parts: [dev.part.id, host.part.id], wires, keys: [host.key, dev.key] });
      }
      continue;
    }

    // One part's own pins joined (a module turned along a breadboard strip, say).
    const byPart = new Map<string, PinRef[]>();
    for (const r of refs) byPart.set(r.part.id, [...(byPart.get(r.part.id) ?? []), r]);
    for (const [, rs] of byPart) {
      const d = rs[0]!.def;
      if (rs.length > 1 && d.kind !== 'board' && !rs.every((r) => r.pin.kind === 'gnd') && !rs.every((r) => r.pin.kind === 'sig' && !r.pin.out) && !rs.every((r) => r.pin.kind === 'speaker-out' || r.pin.kind === 'motor-out')) {
        push({ sev: 'error', code: 'self-joined', text: `${rs[0]!.part.name}: ${rs.map((r) => r.pin.label).join(' and ')} are joined`, more: 'Two of its own pins share a net. On a breadboard: are they in the same strip?', parts: [rs[0]!.part.id], wires, keys: rs.map((r) => r.key) });
      }
    }
    if (sup.length && gnd.length) {
      push({ sev: 'error', code: 'short', text: `Short circuit: ${tag(sup[0]!)} reaches ground`, more: 'Nothing may join a supply to GND. Remove the wire that crosses over.', parts, wires, keys: [sup[0]!.key, gnd[0]!.key] });
    }
    const volts = [...new Set(sup.map((s) => s.pin.volts ?? 0))].sort((a, b) => a - b);
    if (volts.length > 1 && volts[volts.length - 1]! - volts[0]! > 0.2) {
      push({ sev: 'error', code: 'supplies-fight', text: `Two supplies fight: ${volts.map((v) => `${v} V`).join(' and ')} are joined`, more: sup.map(tag).join(', '), parts, wires, keys: sup.map((s) => s.key) });
    }
    const sinks = refs.filter((r) => POWER_IN.has(r.pin.kind));
    for (const c of sinks) {
      if (!sup.length) {
        push({ sev: 'error', code: 'no-power', text: `${tag(c)} has no power`, more: `Wire it to a supply (${c.pin.min}–${c.pin.max} V).`, parts: [c.part.id], wires, keys: [c.key] });
        continue;
      }
      const v = Math.max(...sup.map((s) => s.pin.volts ?? 0));
      const src = sup.find((s) => (s.pin.volts ?? 0) === v)!;
      if (v > (c.pin.max ?? 99) + 0.05) {
        if (c.pin.limit !== undefined && v <= c.pin.limit) {
          push({ sev: 'warn', code: 'hot-regulator', text: `${tag(c)} gets ${v} V, above its ${c.pin.max} V`, more: `It works (the limit is ${c.pin.limit} V), but its regulator burns the difference as heat. A buck converter to ${c.pin.max === 12 ? '9' : c.pin.max} V runs cooler.`, parts: [c.part.id], wires, keys: [c.key, src.key] });
        } else {
          push({ sev: 'error', code: 'over-voltage', text: `${tag(c)} gets ${v} V, more than its ${c.pin.max} V`, more: 'That can destroy it. Use a supply in its range.', parts: [c.part.id], wires, keys: [c.key, src.key] });
        }
      } else if (v < (c.pin.min ?? 0) - 0.05) {
        push({ sev: 'warn', code: 'under-voltage', text: `${tag(c)} gets ${v} V, less than its ${c.pin.min} V`, more: 'It may not start, or may read wrong.', parts: [c.part.id], wires, keys: [c.key, src.key] });
      }
    }
    // How much the supply on this net is asked for.
    if (sup.length === 1 && sup[0]!.pin.maxMa) {
      const want = sinks.reduce((n, c) => n + (c.pin.ma ?? 0), 0);
      if (want > sup[0]!.pin.maxMa) {
        push({ sev: 'warn', code: 'over-budget', text: `${tag(sup[0]!)} gives about ${sup[0]!.pin.maxMa} mA; the parts on it want ${want} mA`, more: 'Feed the hungry ones from a buck converter instead.', parts, wires, keys: [sup[0]!.key, ...sinks.map((c) => c.key)] });
      }
    }
    // Board pins joined directly.
    const levels = [...new Set(ios.map((r) => r.pin.logic ?? 5))];
    if (levels.length > 1) {
      const lo = ios.find((r) => (r.pin.logic ?? 5) < 4)!;
      const hi = ios.find((r) => (r.pin.logic ?? 5) >= 4)!;
      push({ sev: 'error', code: 'logic-joined', text: `${ios.map(tag).join(' and ')}: ${levels.join(' V and ')} V logic joined`, more: 'Put a level shifter between them, or the 3.3 V pin may die.', parts, wires, keys: [lo.key, hi.key] });
    } else if (ios.length > 1) {
      const tx = ios.filter((r) => r.pin.caps?.includes('tx'));
      const rx = ios.filter((r) => r.pin.caps?.includes('rx'));
      if (tx.length > 1) push({ sev: 'error', code: 'tx-tx', text: `TX to TX: ${tx.map(tag).join(' and ')}`, more: 'Cross them: one side\'s TX goes to the other side\'s RX.', parts, wires, keys: tx.map((r) => r.key) });
      else if (!(tx.length === 1 && rx.length === 1 && ios.length === 2)) push({ sev: 'warn', code: 'board-pins-joined', text: `One net joins ${ios.map((r) => r.pin.label).join(' and ')}`, more: 'Two board pins wired together fight when both are outputs.', parts, wires, keys: ios.map((r) => r.key) });
    }
    for (const s of sigs) {
      if (s.pin.caution) push({ sev: 'warn', code: 'caution', text: `${tag(s)}: check this`, more: s.pin.caution, parts: [s.part.id], wires, keys: [s.key] });
      if (sup.length || gnd.length) {
        push({ sev: 'error', code: 'signal-on-power', text: `${tag(s)} is wired to ${sup.length ? 'power' : 'ground'}`, more: 'A signal pin goes to a board pin.', parts: [s.part.id], wires, keys: [s.key] });
        continue;
      }
      if (!ios.length) {
        const throughShifter = s.def.channels?.some((c) => c.includes(s.pin.id));
        if (!s.pin.optional && !throughShifter) push({ sev: 'warn', code: 'signal-unrouted', text: `${tag(s)} doesn't reach a board pin`, more: 'Signals go to a board, or the code can\'t see them.', parts: [s.part.id], wires, keys: [s.key] });
        continue;
      }
      const io = ios[0]!;
      const missing = (s.pin.needs ?? ['io']).filter((n) => !io.pin.caps?.includes(n));
      if (missing.length) {
        const fit = io.def.pins.filter((p) => p.kind === 'io' && (s.pin.needs ?? []).every((n) => p.caps?.includes(n))).map((p) => p.label);
        push({ sev: 'error', code: 'needs-cap', text: `${tag(s)} needs ${missing.map(capName).join(', ')}; ${io.pin.label} can't`, more: fit.length ? `Pins that can: ${fit.slice(0, 12).join(', ')}${fit.length > 12 ? ' …' : ''}` : 'This board has no such pin.', parts: [s.part.id, io.part.id], wires, keys: [s.key, io.key] });
      }
      if (s.pin.drives && s.pin.drives > (io.pin.logic ?? 5) + 0.3) {
        push({ sev: 'error', code: 'drives-into', text: `${tag(s)} sends ${s.pin.drives} V into ${tag(io)} (a ${io.pin.logic} V pin)`, more: 'Put a level shifter or a divider between them, or the board pin may die.', parts: [s.part.id, io.part.id], wires, keys: [s.key, io.key] });
      }
      if (io.pin.serial) push({ sev: 'warn', code: 'serial-pin', text: `${tag(io)} is the board's USB serial`, more: 'Uploads and the Faber link use it. Pick another pin.', parts: [io.part.id], wires, keys: [s.key, io.key] });
    }
    const outs = sigs.filter((s) => s.pin.out);
    if (outs.length > 1 && !outs.every((s) => (s.pin.needs ?? []).some((n) => n === 'sda' || n === 'scl'))) {
      push({ sev: 'warn', code: 'outputs-fight', text: `${outs.length} outputs on one line: ${outs.map(tag).join(', ')}`, more: 'They fight each other. Give each its own pin.', parts, wires, keys: outs.map((s) => s.key) });
    }
    for (const m of refs.filter((r) => LOAD_IN.has(r.pin.kind))) {
      const want = m.pin.kind === 'motor-in' ? 'motor-out' : 'speaker-out';
      if (!refs.some((r) => r.pin.kind === want)) {
        const what = m.pin.kind === 'motor-in' ? 'a motor driver' : 'an amplifier or an MP3 player';
        push({ sev: 'error', code: 'load-on-board', text: `${tag(m)} must go to ${what}`, more: m.pin.kind === 'motor-in' ? 'A motor pulls amps; a board pin gives 20 mA.' : 'A speaker needs a driven output, not a logic pin.', parts: [m.part.id], wires, keys: [m.key] });
      }
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
          push({ sev: 'error', code: 'shifter-tx-tx', text: `${c} to ${c} through ${part.name}: ${a.part.name} ${a.pin.label} and ${b.part.name} ${b.pin.label}`, more: 'Cross them: TX on one side, RX on the other.', parts: [part.id, a.part.id, b.part.id], wires: wiresOf([`${part.id}.${lo}`, `${part.id}.${hi}`]), keys: [a.key, b.key] });
        }
      }
    }
  }

  // Every pin a part needs, connected.
  for (const part of doc.parts) {
    const d = defs(part.def);
    if (!d || d.kind !== 'part') continue;
    const missing = d.pins.filter((p) => !p.optional && !touched.has(`${part.id}.${p.id}`));
    if (missing.length) push({ sev: 'warn', code: 'not-connected', text: `${part.name}: ${missing.map((p) => p.label).join(', ')} not connected`, more: d.name, parts: [part.id], wires: [], keys: missing.map((p) => `${part.id}.${p.id}`) });
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
    // Only a source whose power reaches a board's parts matters.
    if (boardGnds.length && !boardGnds.some((b) => reach(mine, b.k))) {
      push({ sev: 'warn', code: 'no-common-ground', text: `${part.name}: its − doesn't meet a board's GND`, more: 'Join the grounds, or drivers read the board\'s signals wrong.', parts: [part.id], wires: [], keys: g.filter((k) => nets.of.get(k)) });
    }
  }
  // Two boards wired by signals must share ground (a USB cable does that).
  const boards = doc.parts.filter((p) => defs(p.def)?.kind === 'board');
  for (let i = 0; i < boards.length; i++) {
    for (let j = i + 1; j < boards.length; j++) {
      const A = boards[i]!;
      const B = boards[j]!;
      const linked = nets.nets.some((n) => n.kind !== 'gnd' && n.kind !== 'cable' && n.keys.some((k) => splitKey(k)[0] === A.id) && n.keys.some((k) => splitKey(k)[0] === B.id));
      const viaShifter = doc.parts.some((s) => defs(s.def)?.channels && [A, B].every((b) => nets.nets.some((n) => n.keys.some((k) => splitKey(k)[0] === s.id) && n.keys.some((k) => splitKey(k)[0] === b.id && pinDef(doc, defs, k)?.kind === 'io'))));
      if (!linked && !viaShifter) continue;
      const cable = nets.nets.some((n) => n.kind === 'cable' && n.keys.some((k) => splitKey(k)[0] === A.id) && n.keys.some((k) => splitKey(k)[0] === B.id));
      const ga = groundNets(boardGnds.find((b) => b.p === A)!.k);
      if (!cable && !reach(ga, boardGnds.find((b) => b.p === B)!.k)) {
        push({ sev: 'warn', code: 'boards-no-ground', text: `${A.name} and ${B.name} talk but share no ground`, more: 'Run a GND wire between them, or a USB cable.', parts: [A.id, B.id], wires: [], keys: [`${A.id}`, `${B.id}`] });
      }
    }
  }
  if (routing) {
    for (const w of doc.wires) {
      if (routing.routes.get(w.id)?.failed) push({ sev: 'warn', code: 'no-route', text: `No clear path for ${pinName(doc, defs, w.a)} → ${pinName(doc, defs, w.b)}`, more: 'Move the parts apart, or press Arrange.', parts: [], wires: [w.id], keys: [w.a, w.b] });
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
