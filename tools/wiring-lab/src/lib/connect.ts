/**
 * "Connect to…": the wires that join two parts, on free pins that fit. Like the core's allocator
 * (core/src/arduino/wiring/allocator.rs), but over the whole drawing: plain pins are used before
 * pins with rare skills (PWM, interrupts, serial), I²C joins the bus that is already there, and
 * pins near the part win.
 */
import { geom } from './geometry';
import type { Cap, DefLookup, Diagram, Part, PartDef, PinDef } from './types';
import { splitKey } from './types';

export interface ConnectOption {
  label: string;
  wires: [string, string][];
  /** Parts the option needs that aren't on the drawing yet (a level shifter). */
  addParts: Part[];
  /** Pins the option leaves unconnected, or other things to know. */
  notes: string[];
}

export type ConnectResult = { ok: true; options: ConnectOption[] } | { ok: false; reason: string };

const POWER_IN = new Set(['vcc', 'vin', 'power-in']);
const RARE: Cap[] = ['pwm', 'int', 'tx', 'rx', 'sda', 'scl', 'analog'];

interface Ctx {
  doc: Diagram;
  defs: DefLookup;
  used: Set<string>;
  /** I²C buses already wired: board pin key → the need it carries. */
  bus: Map<string, Cap>;
}

function context(doc: Diagram, defs: DefLookup): Ctx {
  const used = new Set<string>();
  const bus = new Map<string, Cap>();
  for (const w of doc.wires) {
    used.add(w.a);
    used.add(w.b);
    for (const [mine, far] of [[w.a, w.b], [w.b, w.a]] as const) {
      const p = pinOf(doc, defs, mine);
      const need = p?.needs?.find((n) => n === 'sda' || n === 'scl');
      if (need) bus.set(far, need);
    }
  }
  return { doc, defs, used, bus };
}

function pinOf(doc: Diagram, defs: DefLookup, key: string): PinDef | undefined {
  const [pid, pn] = splitKey(key);
  const part = doc.parts.find((p) => p.id === pid);
  return part ? defs(part.def)?.pins.find((p) => p.id === pn) : undefined;
}

function centre(ctx: Ctx, part: Part): [number, number] {
  const g = geom(part, ctx.defs);
  return [(g.x0 + g.x1) / 2, (g.y0 + g.y1) / 2];
}

function distance(ctx: Ctx, host: Part, pinId: string, to: [number, number]): number {
  const p = geom(host, ctx.defs).pins[pinId];
  return p ? Math.abs(p.x - to[0]) + Math.abs(p.y - to[1]) : 1e9;
}

/** The best free board pin for a part's signal pin, or why there is none. */
function pickIo(ctx: Ctx, host: Part, hostDef: PartDef, sig: PinDef, near: [number, number], taken: Set<string>): string | { reason: string } {
  const needs = sig.needs ?? ['io'];
  const i2c = needs.find((n) => n === 'sda' || n === 'scl');
  if (i2c) {
    // Join the bus that is already there.
    for (const [key, carried] of ctx.bus) if (carried === i2c && splitKey(key)[0] === host.id) return key;
  }
  let best: { key: string; score: number } | undefined;
  for (const p of hostDef.pins) {
    if (p.kind !== 'io' || p.serial) continue;
    const key = `${host.id}.${p.id}`;
    if (ctx.used.has(key) || taken.has(key)) continue;
    if (!needs.every((n) => p.caps?.includes(n))) continue;
    if (sig.drives && sig.drives > (p.logic ?? 5) + 0.3) continue;
    const wasted = (p.caps ?? []).filter((c) => RARE.includes(c) && !needs.includes(c)).length;
    const score = wasted * 40 + distance(ctx, host, p.id, near) / 4;
    if (!best || score < best.score) best = { key, score };
  }
  if (best) return best.key;
  const what = needs.filter((n) => n !== 'io').map((n) => ({ pwm: 'PWM', int: 'interrupt', analog: 'analog', sda: 'I²C SDA', scl: 'I²C SCL', tx: 'TX', rx: 'RX', io: 'digital' })[n]);
  return { reason: `${hostDef.name} has no free ${what.length ? what.join('/') + ' ' : ''}pin left for ${sig.label}` };
}

function pickSupply(ctx: Ctx, host: Part, hostDef: PartDef, sink: PinDef, near: [number, number]): string | undefined {
  const fits = hostDef.pins.filter((p) => p.kind === 'supply' && !p.inlet && (p.volts ?? 0) >= (sink.min ?? 0) - 0.05 && (p.volts ?? 0) <= (sink.max ?? 99) + 0.05);
  if (!fits.length) return undefined;
  // Prefer 5 V when the part takes it, then a free pin, then the nearest.
  const want = fits.some((p) => Math.abs((p.volts ?? 0) - 5) < 0.2) ? 5 : Math.max(...fits.map((p) => p.volts ?? 0));
  return fits
    .filter((p) => Math.abs((p.volts ?? 0) - want) < 0.2)
    .map((p) => ({ key: `${host.id}.${p.id}`, p }))
    .sort((a, b) => Number(ctx.used.has(a.key)) - Number(ctx.used.has(b.key)) || distance(ctx, host, a.p.id, near) - distance(ctx, host, b.p.id, near))[0]?.key;
}

function pickGnd(ctx: Ctx, host: Part, hostDef: PartDef, near: [number, number]): string | undefined {
  return hostDef.pins
    .filter((p) => p.kind === 'gnd' && !p.inlet)
    .map((p) => ({ key: `${host.id}.${p.id}`, p }))
    .sort((a, b) => Number(ctx.used.has(a.key)) - Number(ctx.used.has(b.key)) || distance(ctx, host, a.p.id, near) - distance(ctx, host, b.p.id, near))[0]?.key;
}

/** A part (or board) joined to another by matching what one needs with what the other has. */
function matchPins(ctx: Ctx, a: Part, b: Part): { wires: [string, string][]; notes: string[]; blocked: string[] } {
  const wires: [string, string][] = [];
  const notes: string[] = [];
  const blocked: string[] = [];
  const taken = new Set<string>();
  const da = ctx.defs(a.def)!;
  const db = ctx.defs(b.def)!;
  const free = (part: Part, p: PinDef) => !ctx.used.has(`${part.id}.${p.id}`);
  for (const [dev, ddef, host, hdef] of [[a, da, b, db], [b, db, a, da]] as const) {
    const near = centre(ctx, dev);
    // Signals.
    if (hdef.pins.some((p) => p.kind === 'io')) {
      for (const p of ddef.pins.filter((x) => x.kind === 'sig' && free(dev, x) && !x.optional)) {
        const r = pickIo(ctx, host, hdef, p, near, taken);
        if (typeof r === 'string') {
          wires.push([`${dev.id}.${p.id}`, r]);
          taken.add(r);
        } else blocked.push(r.reason);
      }
    }
    // Power in from the other side's supply (never from one board into another).
    for (const p of ddef.pins.filter((x) => POWER_IN.has(x.kind) && free(dev, x) && !(ddef.kind === 'board' && hdef.kind === 'board'))) {
      const s = pickSupply(ctx, host, hdef, p, near);
      if (s) wires.push([`${dev.id}.${p.id}`, s]);
    }
    // Motor wires, in order.
    const outs = hdef.pins.filter((x) => x.kind === 'motor-out' && free(host, x));
    const ins = ddef.pins.filter((x) => x.kind === 'motor-in' && free(dev, x));
    if (outs.length && ins.length && outs.length >= ins.length) ins.forEach((p, i) => wires.push([`${dev.id}.${p.id}`, `${host.id}.${outs[i]!.id}`]));
    // USB.
    for (const p of ddef.pins.filter((x) => x.kind === 'usb-device' && free(dev, x))) {
      const port = hdef.pins.find((x) => x.kind === 'usb-host' && free(host, x) && !taken.has(`${host.id}.${x.id}`));
      if (port) {
        wires.push([`${dev.id}.${p.id}`, `${host.id}.${port.id}`]);
        taken.add(`${host.id}.${port.id}`);
      }
    }
  }
  // One ground wire, once something else joins them. Each side uses the ground beside the pin
  // that carries the power (a driver's B− beside B+, the Pi's USB-C ground beside its 5 V).
  if (wires.length && !wires.every(([x, y]) => isUsb(ctx, x) || isUsb(ctx, y))) {
    const ground = (part: Part, d: PartDef): string | undefined => {
      const mine = wires.flat().filter((k) => splitKey(k)[0] === part.id).map((k) => d.pins.find((p) => p.id === splitKey(k)[1])!);
      const power = mine.find((p) => p.kind === 'supply' || POWER_IN.has(p.kind));
      const gnds = d.pins.filter((p) => p.kind === 'gnd');
      if (!gnds.length) return undefined;
      if (power) {
        const g = gnds.slice().sort((x, y) => Math.abs(x.x - power.x) + Math.abs(x.y - power.y) - (Math.abs(y.x - power.x) + Math.abs(y.y - power.y)))[0]!;
        return `${part.id}.${g.id}`;
      }
      if (d.kind === 'board') return pickGnd(ctx, part, d, centre(ctx, part === a ? b : a));
      const g = gnds.find((p) => free(part, p)) ?? gnds[0]!;
      return `${part.id}.${g.id}`;
    };
    const ga = ground(a, da);
    const gb = ground(b, db);
    const already = ga && gb && ctx.doc.wires.some((w) => (w.a === ga && w.b === gb) || (w.a === gb && w.b === ga));
    if (ga && gb && !already && !(ctx.used.has(ga) && ctx.used.has(gb) && da.kind === 'part' && db.kind === 'part')) wires.push([ga, gb]);
  }
  for (const [dev, ddef] of [[a, da], [b, db]] as const) {
    if (ddef.kind !== 'part') continue;
    const missing = ddef.pins.filter((p) => !p.optional && free(dev, p) && !wires.some(([x, y]) => x === `${dev.id}.${p.id}` || y === `${dev.id}.${p.id}`));
    if (missing.length && wires.length) notes.push(`${dev.name}: ${missing.map((p) => p.label).join(', ')} still free`);
  }
  return { wires, notes, blocked };
}

function isUsb(ctx: Ctx, key: string): boolean {
  const k = pinOf(ctx.doc, ctx.defs, key)?.kind;
  return k === 'usb-host' || k === 'usb-device';
}

/** The serial port's RX that goes with a TX pin (Mega TX1 18 ↔ RX1 19, Pi GPIO14 ↔ GPIO15). */
function partnerRx(d: PartDef, tx: PinDef): PinDef | undefined {
  const m = /^(D|GPIO)(\d+)$/.exec(tx.label);
  if (!m) return undefined;
  const label = m[1] + String(Number(m[2]) + 1);
  return d.pins.find((p) => p.label === label && p.caps?.includes('rx'));
}

function num(label: string): number {
  return Number(/\d+$/.exec(label)?.[0] ?? 0);
}

function freeUart(ctx: Ctx, board: Part, d: PartDef): [PinDef, PinDef] | undefined {
  // Serial1 before Serial2 before Serial3 on a Mega (TX1 is D18, TX3 is D14).
  const txs = d.pins.filter((p) => p.caps?.includes('tx') && !p.serial).sort((a, b) => num(b.label) - num(a.label));
  for (const tx of txs) {
    const rx = partnerRx(d, tx);
    if (rx && !ctx.used.has(`${board.id}.${tx.id}`) && !ctx.used.has(`${board.id}.${rx.id}`)) return [tx, rx];
  }
  return undefined;
}

function uart(ctx: Ctx, a: Part, b: Part): ConnectOption | { reason: string } {
  const da = ctx.defs(a.def)!;
  const db = ctx.defs(b.def)!;
  const ua = freeUart(ctx, a, da);
  const ub = freeUart(ctx, b, db);
  if (!ua || !ub) return { reason: `No free serial port on ${!ua ? a.name : b.name}` };
  const la = ua[0].logic ?? 5;
  const lb = ub[0].logic ?? 5;
  const k = (p: Part, pin: PinDef) => `${p.id}.${pin.id}`;
  const wires: [string, string][] = [];
  const notes: string[] = [];
  const addParts: Part[] = [];
  if (Math.abs(la - lb) < 0.3) {
    wires.push([k(a, ua[0]), k(b, ub[1])], [k(b, ub[0]), k(a, ua[1])]);
    const ga = pickGnd(ctx, a, da, centre(ctx, b));
    const gb = pickGnd(ctx, b, db, centre(ctx, a));
    if (ga && gb) wires.push([ga, gb]);
    return { label: `Serial: ${ua[0].label}/${ua[1].label} ↔ ${ub[0].label}/${ub[1].label}`, wires, addParts, notes };
  }
  const [lo, loDef, loU, hi, hiDef, hiU] = la < lb ? [a, da, ua, b, db, ub] : [b, db, ub, a, da, ua];
  // A shifter with two free channels, or a new one between the boards.
  let shifter = ctx.doc.parts.find((p) => {
    const d = ctx.defs(p.def);
    return d?.channels && d.channels.filter(([l, h]) => !ctx.used.has(`${p.id}.${l}`) && !ctx.used.has(`${p.id}.${h}`)).length >= 2;
  });
  if (!shifter) {
    const [ax, ay] = centre(ctx, lo);
    const [bx, by] = centre(ctx, hi);
    let n = 1;
    while (ctx.doc.parts.some((p) => p.name === `shifter${n > 1 ? '-' + n : ''}`)) n += 1;
    shifter = { id: `p${Date.now().toString(36)}s`, def: 'shifter', name: `shifter${n > 1 ? '-' + n : ''}`, x: Math.round((ax + bx) / 4) * 2, y: Math.round((ay + by) / 4) * 2 + 30, rot: 0 };
    addParts.push(shifter);
    notes.push('Adds a level shifter (BSS138, 4 channels)');
  }
  const sd = ctx.defs(shifter.def)!;
  const chans = sd.channels!.filter(([l, h]) => !ctx.used.has(`${shifter.id}.${l}`) && !ctx.used.has(`${shifter.id}.${h}`));
  const [c1, c2] = chans as [[string, string], [string, string]];
  const s = (id: string) => `${shifter.id}.${id}`;
  wires.push([k(lo, loU[0]), s(c1[0])], [s(c1[1]), k(hi, hiU[1])], [k(hi, hiU[0]), s(c2[1])], [s(c2[0]), k(lo, loU[1])]);
  const lv = sd.pins.find((p) => p.id === 'LV')!;
  const hv = sd.pins.find((p) => p.id === 'HV')!;
  if (!ctx.used.has(s('LV'))) {
    const p = pickSupply(ctx, lo, loDef, lv, centre(ctx, shifter));
    if (p) wires.push([s('LV'), p]);
  }
  if (!ctx.used.has(s('HV'))) {
    const p = pickSupply(ctx, hi, hiDef, hv, centre(ctx, shifter));
    if (p) wires.push([s('HV'), p]);
  }
  const lg = pickGnd(ctx, lo, loDef, centre(ctx, shifter));
  const hg = pickGnd(ctx, hi, hiDef, centre(ctx, shifter));
  if (!ctx.used.has(s('LGND')) && lg) wires.push([s('LGND'), lg]);
  if (!ctx.used.has(s('HGND')) && hg) wires.push([s('HGND'), hg]);
  return { label: `Serial through a level shifter: ${loU[0].label}/${loU[1].label} ↔ ${hiU[0].label}/${hiU[1].label}`, wires, addParts, notes };
}

export function connectOptions(doc: Diagram, defs: DefLookup, fromId: string, toId: string): ConnectResult {
  const a = doc.parts.find((p) => p.id === fromId);
  const b = doc.parts.find((p) => p.id === toId);
  if (!a || !b || a === b) return { ok: false, reason: 'Pick another part' };
  const da = defs(a.def);
  const db = defs(b.def);
  if (!da || !db) return { ok: false, reason: 'Unknown part' };
  if (da.kind === 'breadboard' || db.kind === 'breadboard') return { ok: false, reason: 'Drag a part onto the breadboard to plug it in' };
  const ctx = context(doc, defs);
  const options: ConnectOption[] = [];
  const reasons: string[] = [];
  if (da.kind === 'board' && db.kind === 'board') {
    const m = matchPins(ctx, a, b);
    if (m.wires.length) options.push({ label: 'USB cable', wires: m.wires, addParts: [], notes: [`${db.pins.some((p) => p.kind === 'usb-device') ? b.name : a.name} gets its power and the Faber link through it`] });
    const u = uart(ctx, a, b);
    if ('wires' in u) options.push(u);
    else reasons.push(u.reason);
  } else {
    const m = matchPins(ctx, a, b);
    if (m.wires.length) options.push({ label: `${m.wires.length} wire${m.wires.length > 1 ? 's' : ''}`, wires: m.wires, addParts: [], notes: [...m.notes, ...m.blocked] });
    else reasons.push(...m.blocked);
  }
  if (!options.length) return { ok: false, reason: reasons[0] ?? `Nothing on ${a.name} fits ${b.name}` };
  return { ok: true, options };
}
