/**
 * Power mode: switch the drawing on and see what happens. Batteries and adapters are on; a part
 * is on once its power input sits on a live net in its range, and its own outputs (a buck's OUT,
 * a board's 5 V, a Pi's USB ports) go live in turn. A live net that also holds a ground is a short:
 * a fuse or BMS in its path cuts the power (and says so); without one, it is a fire.
 *
 * Current direction: on a live net it flows away from the supply; on a ground net back toward the
 * source's minus. Nothing here models current size beyond each part's rated draw.
 */
import { computeNets, type Net, type NetIndex } from './nets';
import type { DefLookup, Diagram, PinDef } from './types';
import { splitKey } from './types';

export type PartPower = 'on' | 'off' | 'over' | 'under' | 'hot' | 'tripped' | 'source';

export interface PowerState {
  /** Net id → volts, for every live net. */
  live: Map<number, number>;
  shorts: Set<number>;
  parts: Map<string, PartPower>;
  /** Wire id → 1 if current runs from `a` to `b`, −1 the other way. */
  flow: Map<string, 1 | -1>;
  /** Parts that cut the power (a blown fuse, a tripped BMS). */
  tripped: string[];
  messages: { sev: 'ok' | 'warn' | 'error'; text: string }[];
  /** The nets the simulation ran on (switches as they are, tripped parts open). */
  nets: NetIndex;
}

const POWER_IN = new Set(['vcc', 'vin', 'power-in']);
const PROTECTS = new Set(['bms3s', 'fuse']);

function isSource(pins: PinDef[]): boolean {
  return pins.some((p) => p.kind === 'supply') && !pins.some((p) => POWER_IN.has(p.kind) || (p.kind === 'port' && !p.host));
}

export function simulate(doc: Diagram, defs: DefLookup): PowerState {
  let state = run(doc, defs, []);
  // Protection: open each fuse or BMS that sits on a shorted net, then look again.
  for (let round = 0; round < 3 && state.shorts.size; round++) {
    const hit = doc.parts.filter((p) => PROTECTS.has(defs(p.def)?.id ?? '') && !state.tripped.includes(p.id) && defs(p.def)!.pins.some((x) => {
      const n = state.nets.of.get(`${p.id}.${x.id}`);
      return n && state.shorts.has(n.id);
    }));
    if (!hit.length) break;
    state = run(doc, defs, [...state.tripped, ...hit.map((p) => p.id)]);
  }
  return state;
}

function run(doc: Diagram, defs: DefLookup, tripped: string[]): PowerState {
  // Tripped parts are open: simulate on a copy where they conduct nothing.
  const view: Diagram = { ...doc, parts: doc.parts.map((p) => (tripped.includes(p.id) ? { ...p, open: true } : p)) };
  const nets = computeNets(view, defs);
  const live = new Map<number, number>();
  const powered = new Set<string>();
  const parts = new Map<string, PartPower>();
  const netOf = (pid: string, pin: string) => nets.of.get(`${pid}.${pin}`);

  for (const p of view.parts) {
    const d = defs(p.def);
    if (d && d.kind === 'part' && isSource(d.pins)) {
      powered.add(p.id);
      parts.set(p.id, 'source');
    }
  }
  const supplyOut = (pid: string, pin: PinDef): number | undefined => {
    if (pin.kind === 'supply') return pin.volts;
    if (pin.kind === 'port' && pin.host && pin.volts !== undefined) return pin.volts;
    return undefined;
  };
  for (let i = 0; i < 30; i++) {
    let changed = false;
    // Live nets from powered parts' outputs.
    for (const p of view.parts) {
      if (!powered.has(p.id)) continue;
      for (const pin of defs(p.def)!.pins) {
        const v = supplyOut(p.id, pin);
        const n = netOf(p.id, pin.id);
        if (v !== undefined && n && (live.get(n.id) ?? -1) < v) {
          live.set(n.id, v);
          changed = true;
        }
      }
    }
    // Parts whose power input is live come on.
    for (const p of view.parts) {
      if (powered.has(p.id)) continue;
      const d = defs(p.def);
      if (!d) continue;
      const inputs = d.pins.filter((x) => POWER_IN.has(x.kind) || (x.kind === 'port' && !x.host && (x.port === 'usb' || x.port === 'usbc-power')));
      // A board also runs when something feeds its 5 V pin from outside.
      const feeds = d.kind === 'board' ? d.pins.filter((x) => x.kind === 'supply' && x.volts === 5) : [];
      const on = [...inputs, ...feeds].some((x) => {
        const n = netOf(p.id, x.id);
        const v = n ? live.get(n.id) : undefined;
        if (v === undefined) return false;
        if (x.kind === 'port' && x.port === 'usb' && x.min === undefined) return v >= 4.4;
        return v >= (x.min ?? 4.4) - 0.3 && v <= (x.limit ?? x.max ?? 99) + 0.05;
      });
      if (on && inputs.length + feeds.length) {
        powered.add(p.id);
        changed = true;
      }
    }
    if (!changed) break;
  }

  // Shorts: a live net that also holds a ground.
  const shorts = new Set<number>();
  for (const n of nets.nets) {
    if (!live.has(n.id)) continue;
    if (n.keys.some((k) => pinKind(view, defs, k) === 'gnd')) shorts.add(n.id);
  }

  // What each part makes of it.
  for (const p of view.parts) {
    const d = defs(p.def);
    if (!d || parts.has(p.id)) continue;
    if (tripped.includes(p.id)) {
      parts.set(p.id, 'tripped');
      continue;
    }
    const inputs = d.pins.filter((x) => POWER_IN.has(x.kind) || (x.kind === 'port' && !x.host && x.min !== undefined));
    if (!inputs.length && !powered.has(p.id)) continue;
    let st: PartPower = powered.has(p.id) ? 'on' : 'off';
    for (const x of inputs) {
      const n = netOf(p.id, x.id);
      const v = n ? live.get(n.id) : undefined;
      if (v === undefined) continue;
      if (v > (x.limit ?? x.max ?? 99) + 0.05) st = 'over';
      else if (v > (x.max ?? 99) + 0.05 && st !== 'over') st = 'hot';
      else if (v < (x.min ?? 0) - 0.05 && st === 'off') st = 'under';
    }
    parts.set(p.id, st);
  }

  const flow = flows(view, defs, nets, live, powered);
  const messages = describe(view, defs, nets, live, shorts, parts, tripped);
  return { live, shorts, parts, flow, tripped, messages, nets };
}

function pinKind(doc: Diagram, defs: DefLookup, key: string): string | undefined {
  const [pid, pn] = splitKey(key);
  const part = doc.parts.find((p) => p.id === pid);
  return part ? defs(part.def)?.pins.find((x) => x.id === pn)?.kind : undefined;
}

/** Which way current runs in each wire of a live or returning net. */
function flows(doc: Diagram, defs: DefLookup, nets: NetIndex, live: Map<number, number>, powered: Set<string>): Map<string, 1 | -1> {
  // The net as a graph: wires, plugged legs, breadboard strips and pass-throughs.
  const adj = new Map<string, Set<string>>();
  const link = (a: string, b: string) => {
    if (!adj.has(a)) adj.set(a, new Set());
    if (!adj.has(b)) adj.set(b, new Set());
    adj.get(a)!.add(b);
    adj.get(b)!.add(a);
  };
  for (const w of doc.wires) link(w.a, w.b);
  for (const [p, h] of nets.plugs) link(p, h);
  for (const part of doc.parts) {
    const d = defs(part.def);
    for (const bus of d?.buses ?? []) {
      const used = bus.map((id) => `${part.id}.${id}`).filter((k) => adj.has(k));
      for (let i = 1; i < used.length; i++) link(used[0]!, used[i]!);
    }
    if (!part.open) for (const [a, b] of d?.passes ?? []) link(`${part.id}.${a}`, `${part.id}.${b}`);
  }
  const bfs = (starts: string[]) => {
    const dist = new Map<string, number>();
    const queue = [...starts];
    for (const s of starts) dist.set(s, 0);
    while (queue.length) {
      const k = queue.shift()!;
      for (const n of adj.get(k) ?? []) {
        if (!dist.has(n)) {
          dist.set(n, dist.get(k)! + 1);
          queue.push(n);
        }
      }
    }
    return dist;
  };
  const out = new Map<string, 1 | -1>();
  const kindOf = (k: string) => pinKind(doc, defs, k);
  const pinOf = (k: string) => {
    const [pid, pn] = splitKey(k);
    const part = doc.parts.find((p) => p.id === pid);
    return part ? defs(part.def)?.pins.find((x) => x.id === pn) : undefined;
  };
  const byNet = new Map<Net, string[]>();
  for (const w of doc.wires) {
    const n = nets.of.get(w.a);
    if (n) byNet.set(n, [...(byNet.get(n) ?? []), w.id]);
  }
  for (const [net, wires] of byNet) {
    let starts: string[] = [];
    let toward = false;
    if (live.has(net.id)) {
      starts = net.keys.filter((k) => {
        const p = pinOf(k);
        return p && (p.kind === 'supply' || (p.kind === 'port' && p.host)) && powered.has(splitKey(k)[0]);
      });
    } else if (net.kind === 'gnd') {
      // Current returns to the source's minus, if anything on the net is drawing.
      const drawing = net.keys.some((k) => kindOf(k) === 'gnd' && powered.has(splitKey(k)[0]));
      if (!drawing) continue;
      starts = net.keys.filter((k) => {
        const [pid] = splitKey(k);
        const d = defs(doc.parts.find((p) => p.id === pid)!.def)!;
        return kindOf(k) === 'gnd' && d.kind === 'part' && isSource(d.pins);
      });
      if (!starts.length) starts = net.keys.filter((k) => kindOf(k) === 'gnd' && powered.has(splitKey(k)[0])).slice(0, 1);
      toward = true;
    } else continue;
    if (!starts.length) continue;
    const dist = bfs(starts);
    for (const id of wires) {
      const w = doc.wires.find((x) => x.id === id)!;
      const da = dist.get(w.a);
      const db = dist.get(w.b);
      if (da === undefined || db === undefined || da === db) continue;
      const away: 1 | -1 = da < db ? 1 : -1;
      out.set(id, toward ? ((-away) as 1 | -1) : away);
    }
  }
  return out;
}

function describe(doc: Diagram, defs: DefLookup, nets: NetIndex, live: Map<number, number>, shorts: Set<number>, parts: Map<string, PartPower>, tripped: string[]): PowerState['messages'] {
  const msgs: PowerState['messages'] = [];
  const name = (id: string) => doc.parts.find((p) => p.id === id)?.name ?? id;
  for (const id of tripped) {
    const d = defs(doc.parts.find((p) => p.id === id)!.def)!;
    msgs.push({ sev: 'error', text: d.id === 'fuse' ? `${name(id)} blew: there is a short circuit after it` : `${name(id)} cut the power: there is a short circuit after it` });
  }
  if (shorts.size) msgs.push({ sev: 'error', text: 'Short circuit with nothing to stop it: the battery dumps its current through the wires. Fire risk.' });
  for (const [id, st] of parts) {
    if (st === 'over') msgs.push({ sev: 'error', text: `${name(id)} gets too much voltage and would burn out` });
    if (st === 'hot') msgs.push({ sev: 'warn', text: `${name(id)} runs, but its regulator gets hot` });
    if (st === 'under') msgs.push({ sev: 'warn', text: `${name(id)} gets too little voltage to start` });
  }
  const on = [...parts].filter(([, s]) => s === 'on').map(([id]) => name(id));
  const off = [...parts].filter(([id, s]) => s === 'off' && defs(doc.parts.find((p) => p.id === id)!.def)?.kind !== 'breadboard').map(([id]) => name(id));
  if (on.length) msgs.push({ sev: 'ok', text: `On: ${on.join(', ')}` });
  if (off.length) msgs.push({ sev: 'warn', text: `No power: ${off.join(', ')}` });
  void nets;
  void live;
  return msgs;
}
