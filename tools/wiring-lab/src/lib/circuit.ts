/**
 * The electrical simulation: every part becomes resistors, sources, loads and diodes, every wire
 * the copper it is, and Ohm's and Kirchhoff's laws give the volts at every pin and the amps in
 * every wire (nodal analysis). Parts that switch (a diode, a regulator, a relay, a level shifter,
 * a part that browns out) are solved again until nothing changes; a fuse or BMS that sees more
 * than its trip current opens, and the whole thing is solved once more.
 *
 * Figures are datasheet or typical values (each part's notes say which); a real part varies.
 */
import { findPlugs } from './nets';
import type { DefLookup, Diagram, Gauge, Part, PartDef, PinDef, Wire } from './types';
import { splitKey } from './types';

export const GAUGES: Record<Gauge, { name: string; ohmPerM: number; amps: number }> = {
  // A jumper is about 26 AWG plus two crimped contacts; its pins take about 1 A.
  dupont: { name: 'Dupont jumper', ohmPerM: 0.14, amps: 1 },
  '22': { name: '22 AWG', ohmPerM: 0.053, amps: 5 },
  '16': { name: '16 AWG', ohmPerM: 0.0132, amps: 20 },
  '10': { name: '10 AWG silicone', ohmPerM: 0.0033, amps: 55 },
};
export const GAUGE_ORDER: Gauge[] = ['dupont', '22', '16', '10'];
const CONTACT = 0.01;
const CABLE_R = 0.1;
const CABLE_AMPS = 3;

/** What a board pin is doing, from the code (or a test). `level` is 0–1: a PWM duty is its average. */
export type PinState = { mode: 'out'; level: number } | { mode: 'pullup' } | { mode: 'in' };

export interface SimOptions {
  /** Motor drivers: off, driving every motor freely, or every motor held still (the worst case). */
  motors?: 'off' | 'run' | 'stall';
  /** Board pins set by code, by pin key. */
  pins?: Map<string, PinState>;
}

export type PartPower = 'on' | 'off' | 'over' | 'under' | 'hot' | 'tripped' | 'source' | 'overload' | 'burnt';

export interface Finding {
  sev: 'error' | 'warn' | 'info';
  text: string;
  /** Why, in words a beginner follows. */
  why?: string;
  parts: string[];
  wires: string[];
}

export interface CircuitResult {
  /** Volts at a pin, against its circuit's minus; undefined when the pin isn't in a circuit. */
  volts: Map<string, number>;
  /** Amps in each wire, positive from `a` to `b`. */
  amps: Map<string, number>;
  /** Amps through each part (a fuse, a resistor, an LED, a motor, a source's output). */
  partAmps: Map<string, number>;
  parts: Map<string, PartPower>;
  /** Fuses and BMSs that opened, in order. */
  blown: string[];
  findings: Finding[];
}

// ---- the network ----------------------------------------------------------------------------

type Tag = { wire?: string; part?: string; pin?: string };

interface Res {
  k: 'r';
  a: number;
  b: number;
  ohms: number;
  tag: Tag;
}
/** A Norton source: `volts` behind `r` from `neg` to `pos`, while `on`. */
interface Src {
  k: 's';
  pos: number;
  neg: number;
  volts: number;
  r: number;
  on: boolean;
  maxA?: number;
  tag: Tag;
}
/** A load: draws `amps` from `node` into `gnd` while it runs. */
interface Sink {
  k: 'i';
  node: number;
  gnd: number;
  amps: number;
  run: boolean;
  min: number;
  max: number;
  limit: number;
  tag: Tag;
}
interface Diode {
  k: 'd';
  a: number;
  kk: number;
  vf: number;
  rs: number;
  on: boolean;
  tag: Tag;
}
/** A contact that closes when `when` says so (a relay, a level shifter's MOSFET). */
interface Sw {
  k: 'w';
  a: number;
  b: number;
  ohms: number;
  closed: boolean;
  /** Reads the volts between two nodes (0 when they aren't joined). */
  when: (across: (a: number, b: number) => number) => boolean;
  tag: Tag;
}
type Dev = Res | Src | Sink | Diode | Sw;

/** A regulator: its output source is on while its input is in range; its input draws what the output gives. */
interface Reg {
  part: string;
  input: Sink;
  out: Src;
  type: 'linear' | 'buck';
  drop: number;
  quiescent: number;
  inMin: number;
  inLimit: number;
}

class Net {
  parent = new Map<string, string>();
  find(k: string): string {
    if (!this.parent.has(k)) this.parent.set(k, k);
    let r = k;
    while (this.parent.get(r) !== r) {
      const up = this.parent.get(r)!;
      this.parent.set(r, this.parent.get(up)!);
      r = up;
    }
    return r;
  }
  join(a: string, b: string) {
    const ra = this.find(a);
    const rb = this.find(b);
    if (ra !== rb) this.parent.set(ra, rb);
  }
}

const POWER_IN = new Set(['vcc', 'vin', 'power-in']);
const base = (def: string) => def.split('?')[0]!;

interface Built {
  devs: Dev[];
  regs: Reg[];
  index: Map<string, number>;
  n: number;
  /** Per part, its loads: they decide whether it runs. */
  loads: Map<string, Sink[]>;
  /** Signal outputs that only drive while their part runs. */
  drivers: { src: Src; part: string }[];
  /** Pins driven by code, for the pin-current check. */
  pinDrivers: { src: Src; key: string; maxMa: number }[];
  /** Board inputs, for the pin-voltage check. */
  inputs: { key: string; logic: number; part: string }[];
  protect: { part: string; res: Res; trip: number }[];
  /** The node a pin key sits on, if anything uses it. */
  nodeOf: (key: string) => number | undefined;
}

function build(doc: Diagram, defs: DefLookup, opt: SimOptions, blown: Set<string>): Built {
  const net = new Net();
  const used = new Set<string>();
  for (const w of doc.wires) {
    used.add(w.a);
    used.add(w.b);
  }
  const plugs = findPlugs(doc, defs);
  for (const [p, h] of plugs) {
    net.join(p, h);
    used.add(p);
    used.add(h);
  }
  for (const part of doc.parts) {
    const d = defs(part.def);
    for (const bus of d?.buses ?? []) {
      const on = bus.map((id) => `${part.id}.${id}`).filter((k) => used.has(k));
      for (let i = 1; i < on.length; i++) net.join(on[0]!, on[i]!);
    }
  }

  const pending: ((node: (k: string) => number) => void)[] = [];
  const later = (f: (node: (k: string) => number) => void) => pending.push(f);
  const loads = new Map<string, Sink[]>();
  const drivers: Built['drivers'] = [];
  const pinDrivers: Built['pinDrivers'] = [];
  const inputs: Built['inputs'] = [];
  const protect: Built['protect'] = [];
  const devs: Dev[] = [];
  const regs: Reg[] = [];

  const addLoad = (part: string, s: Sink) => {
    devs.push(s);
    loads.set(part, [...(loads.get(part) ?? []), s]);
  };

  for (const part of doc.parts) {
    const d = defs(part.def);
    if (!d || d.kind === 'breadboard') continue;
    const id = part.id;
    const k = (pin: string) => `${id}.${pin}`;
    const G = `${id}#gnd`;
    const claimed = new Set<string>();
    for (const p of d.pins) {
      if (p.kind === 'gnd') net.join(k(p.id), G);
      if (p.kind === 'port') net.join(`${k(p.id)}#g`, G);
    }
    const b = base(part.def);

    // A source part (battery, adapter, charger): its outputs are sources against its minus.
    const hasInput = d.pins.some((p) => POWER_IN.has(p.kind) || (p.kind === 'port' && !p.host));
    if (d.kind === 'part' && !hasInput && d.pins.some((p) => p.kind === 'supply')) {
      for (const p of d.pins.filter((x) => x.kind === 'supply')) {
        claimed.add(p.id);
        const cells = b === 'bat' ? batteryShape(part.def) : null;
        const r = cells ? (0.025 * cells.s) / cells.p : 0.1;
        later((node) => devs.push({ k: 's', pos: node(k(p.id)), neg: node(G), volts: p.volts ?? 0, r, on: true, maxA: p.maxMa ? p.maxMa / 1000 : undefined, tag: { part: id, pin: p.id } }));
      }
    }

    if (d.kind === 'board') boardModel(part, d, net, later, addLoad, regs, devs, claimed, opt, pinDrivers, inputs);
    else partModel(part, d, b, net, later, addLoad, regs, devs, claimed, opt, drivers, protect, blown);

    // Every power input left over is a load of its own.
    for (const p of d.pins) {
      if (claimed.has(p.id)) continue;
      if (POWER_IN.has(p.kind)) {
        const ma = p.ma ?? (p.kind === 'vcc' ? 10 : 0);
        later((node) => addLoad(id, { k: 'i', node: node(k(p.id)), gnd: node(G), amps: ma / 1000, run: false, min: p.min ?? 0, max: p.max ?? 99, limit: p.limit ?? p.max ?? 99, tag: { part: id, pin: p.id } }));
      } else if (p.kind === 'port' && !p.host && (p.port === 'usb' || p.port === 'usbc-power')) {
        const ma = p.ma ?? 0;
        later((node) => addLoad(id, { k: 'i', node: node(`${k(p.id)}#v`), gnd: node(G), amps: ma / 1000, run: false, min: p.min ?? 4.4, max: p.max ?? 5.5, limit: p.limit ?? 6, tag: { part: id, pin: p.id } }));
      } else if (p.kind === 'sig' && p.out && p.drives) {
        later((node) => {
          const src: Src = { k: 's', pos: node(k(p.id)), neg: node(G), volts: p.drives!, r: 100, on: false, tag: { part: id, pin: p.id } };
          devs.push(src);
          drivers.push({ src, part: id });
        });
      }
    }
  }

  // Index the nodes now that every ideal join is known.
  const index = new Map<string, number>();
  let n = 0;
  const node = (key: string): number => {
    const r = net.find(key);
    let i = index.get(r);
    if (i === undefined) {
      i = n++;
      index.set(r, i);
    }
    return i;
  };
  for (const f of pending) f(node);

  // Wires: copper with resistance. A cable between sockets carries VBUS and ground.
  for (const w of doc.wires) {
    const pa = pinOf(doc, defs, w.a);
    const pb = pinOf(doc, defs, w.b);
    if (pa?.kind === 'port' && pb?.kind === 'port') {
      if ((pa.port === 'usb' || pa.port === 'usbc-power') && pa.port === pb.port) {
        devs.push({ k: 'r', a: node(`${w.a}#v`), b: node(`${w.b}#v`), ohms: CABLE_R, tag: { wire: w.id } });
        devs.push({ k: 'r', a: node(`${w.a}#g`), b: node(`${w.b}#g`), ohms: CABLE_R, tag: {} });
      }
      continue;
    }
    devs.push({ k: 'r', a: node(w.a), b: node(w.b), ohms: wireOhms(w, gaugeOf(w, doc, defs)), tag: { wire: w.id } });
  }
  const nodeOf = (key: string) => index.get(net.find(key));
  return { devs, regs, index, n, loads, drivers, pinDrivers, inputs, protect, nodeOf };
}

function boardModel(
  part: Part,
  d: PartDef,
  net: Net,
  later: (f: (node: (k: string) => number) => void) => void,
  addLoad: (part: string, s: Sink) => void,
  regs: Reg[],
  devs: Dev[],
  claimed: Set<string>,
  opt: SimOptions,
  pinDrivers: Built['pinDrivers'],
  inputs: Built['inputs'],
): void {
  const id = part.id;
  const k = (pin: string) => `${id}.${pin}`;
  const G = `${id}#gnd`;
  const R5 = `${id}#5v`;
  const R3 = `${id}#3v3`;
  for (const p of d.pins) {
    if (p.kind === 'supply' && p.volts === 5) net.join(k(p.id), R5);
    if (p.kind === 'supply' && p.volts === 3.3) net.join(k(p.id), R3);
    if (p.kind === 'supply') claimed.add(p.id);
  }
  const pi = d.id === 'pi5';
  const draw = pi ? 0.8 : d.id === 'mega' ? 0.08 : d.id === 'nano' ? 0.03 : 0.05;
  later((node) => {
    // What the board itself runs on: its 5 V rail.
    addLoad(id, { k: 'i', node: node(R5), gnd: node(G), amps: draw, run: false, min: pi ? 4.63 : 4.3, max: 5.5, limit: 6, tag: { part: id } });
    // The 3.3 V regulator from the 5 V rail.
    const out3: Src = { k: 's', pos: node(R3), neg: node(G), volts: 3.3, r: 0.1, on: false, maxA: pi ? 0.8 : 0.05, tag: { part: id, pin: '3V3' } };
    const in3: Sink = { k: 'i', node: node(R5), gnd: node(G), amps: 0, run: true, min: 0, max: 99, limit: 99, tag: { part: id } };
    devs.push(out3, in3);
    regs.push({ part: id, input: in3, out: out3, type: 'linear', drop: 1.1, quiescent: 0.002, inMin: 4.3, inLimit: 6 });
  });
  for (const p of d.pins) {
    if (p.kind === 'port' && (p.port === 'usb' || p.port === 'usbc-power')) {
      claimed.add(p.id);
      if (pi && p.id === 'USBC') {
        net.join(`${k(p.id)}#v`, R5);
      } else if (p.host) {
        // A Pi's USB ports share its 5 V rail through a current switch.
        later((node) => devs.push({ k: 'r', a: node(R5), b: node(`${k(p.id)}#v`), ohms: 0.05, tag: { part: id, pin: p.id } }));
      } else {
        // An Arduino's USB feeds its 5 V rail through a 500 mA polyfuse.
        later((node) => devs.push({ k: 'r', a: node(`${k(p.id)}#v`), b: node(R5), ohms: 0.35, tag: { part: id, pin: p.id } }));
      }
    }
    if (p.kind === 'vin') {
      claimed.add(p.id);
      later((node) => {
        const out: Src = { k: 's', pos: node(R5), neg: node(G), volts: 5, r: 0.1, on: false, maxA: 0.8, tag: { part: id, pin: p.id } };
        const input: Sink = { k: 'i', node: node(k(p.id)), gnd: node(G), amps: 0, run: true, min: p.min ?? 7, max: p.max ?? 12, limit: p.limit ?? 20, tag: { part: id, pin: p.id } };
        devs.push(out, input);
        regs.push({ part: id, input, out, type: 'linear', drop: 1.2, quiescent: 0.005, inMin: 6.2, inLimit: p.limit ?? 20 });
      });
    }
    if (p.kind === 'io') {
      const key = k(p.id);
      const st = opt.pins?.get(key);
      const logic = p.logic ?? 5;
      inputs.push({ key, logic, part: id });
      if (st?.mode === 'out') {
        later((node) => {
          const src: Src = { k: 's', pos: node(key), neg: node(G), volts: logic * st.level, r: pi ? 40 : 25, on: true, tag: { part: id, pin: p.id } };
          devs.push(src);
          pinDrivers.push({ src, key, maxMa: pi ? 16 : 40 });
        });
      } else if (st?.mode === 'pullup') {
        later((node) => devs.push({ k: 'r', a: node(key), b: node(logic === 3.3 ? R3 : R5), ohms: pi ? 50000 : 35000, tag: {} }));
      }
    }
  }
}

function partModel(
  part: Part,
  d: PartDef,
  b: string,
  net: Net,
  later: (f: (node: (k: string) => number) => void) => void,
  addLoad: (part: string, s: Sink) => void,
  regs: Reg[],
  devs: Dev[],
  claimed: Set<string>,
  opt: SimOptions,
  drivers: Built['drivers'],
  protect: Built['protect'],
  blown: Set<string>,
): void {
  const id = part.id;
  const k = (pin: string) => `${id}.${pin}`;
  const G = `${id}#gnd`;
  const pin = (p: string) => d.pins.find((x) => x.id === p);

  const regulator = (inPin: string, outKey: string, volts: number, type: 'linear' | 'buck', drop: number, maxA: number, outPin?: string) => {
    const ip = pin(inPin);
    claimed.add(inPin);
    if (outPin) claimed.add(outPin);
    later((node) => {
      const out: Src = { k: 's', pos: node(outKey), neg: node(G), volts, r: 0.05, on: false, maxA, tag: { part: id, pin: outPin } };
      const input: Sink = { k: 'i', node: node(k(inPin)), gnd: node(G), amps: 0, run: true, min: ip?.min ?? 0, max: ip?.max ?? 99, limit: ip?.limit ?? ip?.max ?? 99, tag: { part: id, pin: inPin } };
      devs.push(out, input);
      regs.push({ part: id, input, out, type, drop, quiescent: 0.005, inMin: ip?.min ?? volts + drop, inLimit: (ip?.limit ?? ip?.max ?? 99) + 0.5 });
    });
  };
  const volts = (p?: string) => pin(p ?? '')?.volts ?? 5;
  switch (b) {
    case 'buck5':
      regulator('IN+', k('OUT+'), volts('OUT+'), 'buck', 1.5, (pin('OUT+')?.maxMa ?? 3000) / 1000, 'OUT+');
      return;
    case 'buckusb':
      regulator('IN+', `${k('USBC')}#v`, 5.1, 'buck', 1.5, 5, 'USBC');
      return;
    case 'l7805':
      regulator('IN', k('OUT'), 5, 'linear', 2, 1, 'OUT');
      return;
    case 'ld1117':
      regulator('IN', k('OUT'), 3.3, 'linear', 1.1, 0.8, 'OUT');
      return;
    case 'lm2596t12':
      regulator('VIN', k('OUT'), 12, 'buck', 1.5, 3, 'OUT');
      return;
    case 'l298n':
      regulator('12V', k('5V'), 5, 'linear', 2, 0.5, '5V');
      driverModel(id, d, opt, later, devs, ['12V', 'OUT1', 'OUT2'], 1);
      return;
    case 'mb102': {
      claimed.add('USB');
      regulator('IN', k('T+'), volts('T+'), 'linear', 1.2, 0.7, 'T+');
      regulator('IN', k('B+'), volts('B+'), 'linear', 1.2, 0.7, 'B+');
      return;
    }
    case 'cp2102': {
      // Its 5V pin is the USB host's 5 V; 3V3 comes from a small regulator inside.
      net.join(k('5V'), `${k('USB')}#v`);
      claimed.add('5V');
      claimed.add('3V3');
      later((node) => {
        const out: Src = { k: 's', pos: node(k('3V3')), neg: node(G), volts: 3.3, r: 0.2, on: false, maxA: 0.05, tag: { part: id, pin: '3V3' } };
        const input: Sink = { k: 'i', node: node(`${k('USB')}#v`), gnd: node(G), amps: 0, run: true, min: 0, max: 99, limit: 99, tag: { part: id } };
        devs.push(out, input);
        regs.push({ part: id, input, out, type: 'linear', drop: 1, quiescent: 0.02, inMin: 4.3, inLimit: 6 });
      });
      return;
    }
    case 'bts7960':
      driverModel(id, d, opt, later, devs, ['B+', 'M+', 'M-'], 0.008);
      return;
    case 'relay40': {
      // The coil pulls in above about 8 V; then 30 meets 87, otherwise 87a.
      claimed.add('86');
      later((node) => {
        addLoad(id, { k: 'i', node: node(k('86')), gnd: node(G), amps: 0.15, run: false, min: 8, max: 15, limit: 16, tag: { part: id, pin: '86' } });
        const coil = (across: (a: number, b: number) => number) => across(node(k('86')), node(G)) >= 8;
        devs.push({ k: 'w', a: node(k('30')), b: node(k('87')), ohms: 0.003, closed: false, when: coil, tag: { part: id } });
        devs.push({ k: 'w', a: node(k('30')), b: node(k('87a')), ohms: 0.003, closed: true, when: (v) => !coil(v), tag: { part: id } });
      });
      return;
    }
    case 'shifter': {
      // BSS138 channels: 10 kΩ pull-ups on both sides; a side pulled low pulls the other low.
      later((node) => {
        for (const [lo, hi] of d.channels ?? []) {
          devs.push({ k: 'r', a: node(k(lo)), b: node(k('LV')), ohms: 10000, tag: {} });
          devs.push({ k: 'r', a: node(k(hi)), b: node(k('HV')), ohms: 10000, tag: {} });
          const low = (across: (a: number, b: number) => number) => {
            const lv = across(node(k('LV')), node(G));
            return lv > 1.5 && (across(node(k(lo)), node(G)) < lv - 1.4 || across(node(k(hi)), node(G)) < lv - 1.4);
          };
          devs.push({ k: 'w', a: node(k(lo)), b: node(k(hi)), ohms: 5, closed: false, when: low, tag: {} });
        }
      });
      return;
    }
  }

  // Parts that only conduct: a closed fuse, switch, connector or BMS is a few milliohms.
  if (d.passes && !part.open && !blown.has(id)) {
    for (const [a, bb] of d.passes) {
      later((node) => {
        const res: Res = { k: 'r', a: node(k(a)), b: node(k(bb)), ohms: d.trip ? 0.004 : 0.002, tag: { part: id } };
        devs.push(res);
        if (d.trip) protect.push({ part: id, res, trip: d.trip });
      });
    }
  }
  const e = d.elec;
  if (!e) return;
  later((node) => {
    switch (e.t) {
      case 'resistor':
        devs.push({ k: 'r', a: node(k(e.a)), b: node(k(e.b)), ohms: e.ohms, tag: { part: id } });
        break;
      case 'diode':
        devs.push({ k: 'd', a: node(k(e.a)), kk: node(k(e.k)), vf: e.vf, rs: e.rs, on: false, tag: { part: id } });
        break;
      case 'tvs':
        devs.push({ k: 'd', a: node(k(e.k)), kk: node(k(e.a)), vf: e.vbr, rs: 0.5, on: false, tag: { part: id } });
        devs.push({ k: 'd', a: node(k(e.a)), kk: node(k(e.k)), vf: 0.7, rs: 0.05, on: false, tag: { part: id } });
        break;
      case 'motor': {
        const amps = opt.motors === 'stall' ? e.stallA : e.runA;
        devs.push({ k: 'r', a: node(k(e.a)), b: node(k(e.b)), ohms: e.volts / amps, tag: { part: id } });
        break;
      }
      case 'cap':
        break;
    }
  });
  void drivers;
}

/** A motor driver drives M+ high and M− low while the motors run (both bridges on). */
function driverModel(id: string, d: PartDef, opt: SimOptions, later: (f: (node: (k: string) => number) => void) => void, devs: Dev[], [supply, a, b]: string[], ohms: number): void {
  if (!opt.motors || opt.motors === 'off') return;
  if (!d.pins.some((p) => p.id === a)) return;
  later((node) => {
    devs.push({ k: 'r', a: node(`${id}.${supply}`), b: node(`${id}.${a}`), ohms, tag: { part: id } });
    devs.push({ k: 'r', a: node(`${id}.${b}`), b: node(`${id}#gnd`), ohms, tag: {} });
  });
}

function batteryShape(def: string): { s: number; p: number } {
  const q = new URLSearchParams(def.split('?')[1] ?? '');
  return { s: Number(q.get('s') ?? 3), p: Number(q.get('p') ?? 1) };
}

function pinOf(doc: Diagram, defs: DefLookup, key: string): PinDef | undefined {
  const [pid, pn] = splitKey(key);
  const part = doc.parts.find((p) => p.id === pid);
  return part ? defs(part.def)?.pins.find((p) => p.id === pn) : undefined;
}

/** A wire's gauge: what it says, else a jumper between pins and 22 AWG on a screw terminal. */
export function gaugeOf(w: Wire, doc: Diagram, defs: DefLookup): Gauge {
  if (w.gauge) return w.gauge;
  return pinOf(doc, defs, w.a)?.terminal || pinOf(doc, defs, w.b)?.terminal ? '22' : 'dupont';
}

export function wireOhms(w: Wire, g: Gauge): number {
  return GAUGES[g].ohmPerM * (w.len ?? 0.25) + (g === 'dupont' ? 2 * CONTACT : 0);
}

// ---- solving --------------------------------------------------------------------------------

const GMIN = 1e-7;

function solve(n: number, devs: Dev[]): Float64Array {
  const A = new Float64Array(n * n);
  const z = new Float64Array(n);
  const add = (i: number, c: number) => {
    A[i] = A[i]! + c;
  };
  const g = (a: number, b: number, c: number) => {
    add(a * n + a, c);
    add(b * n + b, c);
    add(a * n + b, -c);
    add(b * n + a, -c);
  };
  /** Current `i` leaves `from` and enters `to`. */
  const cur = (from: number, to: number, i: number) => {
    z[from] = z[from]! - i;
    z[to] = z[to]! + i;
  };
  for (let i = 0; i < n; i++) add(i * n + i, GMIN);
  for (const d of devs) {
    if (d.k === 'r') g(d.a, d.b, 1 / Math.max(d.ohms, 1e-6));
    else if (d.k === 's') {
      if (!d.on) continue;
      g(d.pos, d.neg, 1 / d.r);
      cur(d.neg, d.pos, d.volts / d.r);
    } else if (d.k === 'i') {
      if (d.run && d.amps) cur(d.node, d.gnd, d.amps);
    } else if (d.k === 'd') {
      if (d.on) {
        g(d.a, d.kk, 1 / d.rs);
        cur(d.kk, d.a, d.vf / d.rs);
      } else g(d.a, d.kk, 1e-9);
    } else if (d.k === 'w') {
      if (d.closed) g(d.a, d.b, 1 / d.ohms);
    }
  }
  // Gaussian elimination with partial pivoting.
  for (let c = 0; c < n; c++) {
    let p = c;
    let best = Math.abs(A[c * n + c]!);
    for (let r = c + 1; r < n; r++) {
      const v = Math.abs(A[r * n + c]!);
      if (v > best) {
        best = v;
        p = r;
      }
    }
    if (best < 1e-15) continue;
    if (p !== c) {
      for (let j = c; j < n; j++) {
        const t = A[c * n + j]!;
        A[c * n + j] = A[p * n + j]!;
        A[p * n + j] = t;
      }
      const t = z[c]!;
      z[c] = z[p]!;
      z[p] = t;
    }
    const piv = A[c * n + c]!;
    for (let r = c + 1; r < n; r++) {
      const f = A[r * n + c]! / piv;
      if (f === 0) continue;
      for (let j = c; j < n; j++) A[r * n + j] = A[r * n + j]! - f * A[c * n + j]!;
      z[r] = z[r]! - f * z[c]!;
    }
  }
  const v = new Float64Array(n);
  for (let r = n - 1; r >= 0; r--) {
    let s = z[r]!;
    for (let j = r + 1; j < n; j++) s -= A[r * n + j]! * v[j]!;
    const piv = A[r * n + r]!;
    v[r] = Math.abs(piv) < 1e-15 ? 0 : s / piv;
  }
  return v;
}

const srcAmps = (s: Src, v: Float64Array) => (s.on ? s.volts / s.r - (v[s.pos]! - v[s.neg]!) / s.r : 0);
const resAmps = (r: Res, v: Float64Array) => (v[r.a]! - v[r.b]!) / Math.max(r.ohms, 1e-6);
const diodeAmps = (d: Diode, v: Float64Array) => (d.on ? (v[d.a]! - v[d.kk]! - d.vf) / d.rs : 0);

/** Which nodes are joined by something that conducts: a node on its own floats, and reads 0 V. */
function components(n: number, devs: Dev[]): (i: number) => number {
  const up = new Int32Array(n).map((_, i) => i);
  const find = (i: number): number => {
    while (up[i] !== i) {
      up[i] = up[up[i]!]!;
      i = up[i]!;
    }
    return i;
  };
  const link = (x: number, y: number) => {
    const rx = find(x);
    const ry = find(y);
    if (rx !== ry) up[rx] = ry;
  };
  for (const d of devs) {
    if (d.k === 'r') link(d.a, d.b);
    else if (d.k === 's' && d.on) link(d.pos, d.neg);
    else if (d.k === 'd' && d.on) link(d.a, d.kk);
    else if (d.k === 'w' && d.closed) link(d.a, d.b);
  }
  return find;
}

/** Solves until every switching part agrees with the voltages it sees. */
function settle(b: Built): Float64Array {
  let v = solve(b.n, b.devs);
  for (let round = 0; round < 60; round++) {
    let changed = false;
    const comp = components(b.n, b.devs);
    const across = (x: number, y: number) => (comp(x) === comp(y) ? v[x]! - v[y]! : 0);
    for (const d of b.devs) {
      if (d.k === 'd') {
        const want = d.on ? diodeAmps(d, v) > -1e-6 : across(d.a, d.kk) > d.vf;
        if (want !== d.on) {
          d.on = want;
          changed = true;
        }
      } else if (d.k === 'w') {
        const want = d.when(across);
        if (want !== d.closed) {
          d.closed = want;
          changed = true;
        }
      }
    }
    // Loads run in their range (with a little hysteresis so a weak supply doesn't flicker).
    for (const list of b.loads.values()) {
      for (const s of list) {
        const u = across(s.node, s.gnd);
        const want = s.run ? u >= s.min - 0.6 : u >= s.min - 0.25;
        if (want !== s.run) {
          s.run = want;
          changed = true;
        }
      }
    }
    for (const r of b.regs) {
      const u = across(r.input.node, r.input.gnd);
      const on = u >= r.inMin - 0.05 && u >= r.out.volts + r.drop - 0.05 && u <= r.inLimit;
      if (on !== r.out.on) {
        r.out.on = on;
        changed = true;
      }
      // What the output gives, within reason (a shorted output is limited by the regulator itself).
      const out = Math.min(Math.max(0, srcAmps(r.out, v)), 3 * (r.out.maxA ?? 5));
      const vo = Math.max(0, across(r.out.pos, r.out.neg));
      const ideal = !on ? 0 : r.type === 'buck' ? (vo * out) / (0.88 * Math.max(u, r.inMin, 1)) + r.quiescent : out + r.quiescent;
      // Damped, so a regulator and a weak supply settle instead of swinging.
      const want = r.input.amps === 0 ? ideal : (r.input.amps + ideal) / 2;
      if (Math.abs(want - r.input.amps) > 2e-4 + 0.002 * want) {
        r.input.amps = want;
        changed = true;
      }
    }
    for (const dr of b.drivers) {
      const runs = (b.loads.get(dr.part) ?? []).length > 0 && (b.loads.get(dr.part) ?? []).every((s) => s.run);
      if (runs !== dr.src.on) {
        dr.src.on = runs;
        changed = true;
      }
    }
    if (!changed) break;
    v = solve(b.n, b.devs);
  }
  return v;
}

// ---- the whole simulation -------------------------------------------------------------------

export function runCircuit(doc: Diagram, defs: DefLookup, opt: SimOptions = {}): CircuitResult {
  const blown: string[] = [];
  let b = build(doc, defs, opt, new Set());
  let v = settle(b);
  for (let round = 0; round < 8; round++) {
    const over = b.protect.filter((p) => Math.abs(resAmps(p.res, v)) > p.trip);
    if (!over.length) break;
    // The one carrying the most, relative to its rating, goes first; then look again.
    over.sort((x, y) => Math.abs(resAmps(y.res, v)) / y.trip - Math.abs(resAmps(x.res, v)) / x.trip);
    trips.set(over[0]!.part, Math.abs(resAmps(over[0]!.res, v)));
    blown.push(over[0]!.part);
    b = build(doc, defs, opt, new Set(blown));
    v = settle(b);
  }
  const result = read(doc, defs, b, v, blown);
  trips.clear();
  return result;
}

/** The current that opened each protection, for its message. */
const trips = new Map<string, number>();

function read(doc: Diagram, defs: DefLookup, b: Built, v: Float64Array, blown: string[]): CircuitResult {
  const joined = components(b.n, b.devs);
  const across = (x: number, y: number) => (joined(x) === joined(y) ? v[x]! - v[y]! : 0);
  // Each separate circuit is measured against its own minus.
  const find = joined;
  const ref = new Map<number, number>();
  for (const d of b.devs) {
    if (d.k === 's' && d.on && !ref.has(find(d.neg))) {
      const part = doc.parts.find((p) => p.id === d.tag.part);
      if (part && defs(part.def)?.kind === 'part') ref.set(find(d.neg), d.neg);
    }
  }
  for (const d of b.devs) if (d.k === 's' && d.on && !ref.has(find(d.neg))) ref.set(find(d.neg), d.neg);
  const sourced = new Set(ref.keys());
  const volts = new Map<string, number>();
  const pinVolts = (key: string): number | undefined => {
    const i = b.nodeOf(key);
    if (i === undefined) return undefined;
    const c = find(i);
    return sourced.has(c) ? round(v[i]! - v[ref.get(c)!]!) : undefined;
  };
  for (const part of doc.parts) {
    const d = defs(part.def);
    for (const p of d?.pins ?? []) {
      const key = `${part.id}.${p.id}`;
      const u = pinVolts(key);
      if (u !== undefined) volts.set(key, u);
    }
  }

  const amps = new Map<string, number>();
  const partAmps = new Map<string, number>();
  const addPart = (id: string | undefined, a: number) => {
    if (id) partAmps.set(id, Math.max(partAmps.get(id) ?? 0, Math.abs(a)));
  };
  for (const d of b.devs) {
    if (d.k === 'r') {
      const a = resAmps(d, v);
      if (d.tag.wire) amps.set(d.tag.wire, a);
      else addPart(d.tag.part, a);
    } else if (d.k === 'd') addPart(d.tag.part, diodeAmps(d, v));
    else if (d.k === 's' && d.tag.part && defs(doc.parts.find((p) => p.id === d.tag.part)!.def)?.kind === 'part') addPart(d.tag.part, srcAmps(d, v));
  }

  const findings: Finding[] = [];
  const parts = new Map<string, PartPower>();
  const name = (id: string) => doc.parts.find((p) => p.id === id)?.name ?? id;
  const label = (id: string, pin?: string) => {
    const part = doc.parts.find((p) => p.id === id);
    const pd = part ? defs(part.def)?.pins.find((x) => x.id === pin) : undefined;
    return pd ? `${name(id)} ${pd.label}` : name(id);
  };
  const worst = (id: string, st: PartPower) => {
    const rank: PartPower[] = ['off', 'source', 'on', 'under', 'hot', 'overload', 'tripped', 'over', 'burnt'];
    const now = parts.get(id);
    if (!now || rank.indexOf(st) > rank.indexOf(now)) parts.set(id, st);
  };
  const fmtV = (x: number) => `${x.toFixed(x < 10 ? 2 : 1)} V`;
  const fmtA = (x: number) => (Math.abs(x) < 1 ? `${Math.round(Math.abs(x) * 1000)} mA` : `${Math.abs(x).toFixed(1)} A`);

  for (const id of blown) {
    const part = doc.parts.find((p) => p.id === id)!;
    const d = defs(part.def)!;
    worst(id, 'tripped');
    const a = trips.get(id) ?? 0;
    findings.push({
      sev: 'error',
      text: `${name(id)} ${base(d.id) === 'bms3s' ? 'cut the power' : 'blew'}: ${fmtA(a)} went through it, more than its ${d.trip} A`,
      why: a > 3 * (d.trip ?? 1) ? 'Something after it joins + to −, or nearly: a short circuit. That is what it is there for.' : 'Too much is drawn after it. Draw less, or use a part rated higher (and wires to match).',
      parts: [id],
      wires: [],
    });
  }

  // Loads: does each part get what it needs?
  for (const [pid, list] of b.loads) {
    for (const s of list) {
      const u = across(s.node, s.gnd);
      const where = label(pid, s.tag.pin);
      if (u > s.limit + 0.05) {
        worst(pid, 'over');
        findings.push({ sev: 'error', text: `${where} gets ${fmtV(u)}, more than the ${s.limit} V it can take: it burns out`, why: 'Every part has a highest voltage. Above it, too much current is pushed through the chip inside.', parts: [pid], wires: [] });
      } else if (u > s.max + 0.05) {
        worst(pid, 'hot');
        findings.push({ sev: 'warn', text: `${where} gets ${fmtV(u)}, above the ${s.max} V it likes`, why: 'It works, but its regulator turns the extra voltage into heat.', parts: [pid], wires: [] });
      } else if (!s.run && u > 0.3) {
        worst(pid, 'under');
        findings.push({ sev: 'warn', text: `${where} gets ${fmtV(u)}, less than the ${s.min} V it needs: it won't start`, why: 'Too little voltage: the supply is too weak, too far away through thin wires, or set too low.', parts: [pid], wires: [] });
      } else if (s.run) worst(pid, 'on');
    }
  }
  // Sources and regulators: are they asked for more than they give?
  for (const d of b.devs) {
    if (d.k !== 's' || !d.on || !d.maxA || !d.tag.part) continue;
    const a = srcAmps(d, v);
    const part = doc.parts.find((p) => p.id === d.tag.part)!;
    if (defs(part.def)?.kind === 'part' && !b.regs.some((r) => r.out === d)) worst(part.id, 'source');
    if (a > d.maxA * 1.02) {
      worst(part.id, 'overload');
      findings.push({ sev: a > d.maxA * 2 ? 'error' : 'warn', text: `${label(part.id, d.tag.pin)} gives ${fmtA(d.maxA)} at most; ${fmtA(a)} is drawn from it`, why: 'Asked for more current than it can give, a supply sags, gets hot or shuts off. Feed some of the parts from another supply.', parts: [part.id], wires: [] });
    }
  }
  for (const r of b.regs) {
    const vin0 = across(r.input.node, r.input.gnd);
    if (defs(doc.parts.find((p) => p.id === r.part)!.def)?.kind === 'part') worst(r.part, r.out.on ? 'on' : vin0 > 0.3 ? 'under' : 'off');
    if (!r.out.on) continue;
    const out = Math.max(0, srcAmps(r.out, v));
    const vin = across(r.input.node, r.input.gnd);
    const heat = r.type === 'linear' ? (vin - r.out.volts) * out : 0;
    if (heat > 1) {
      worst(r.part, 'hot');
      findings.push({ sev: heat > 2.5 ? 'error' : 'warn', text: `${label(r.part, r.out.tag.pin)} turns ${heat.toFixed(1)} W into heat (${fmtV(vin)} in, ${r.out.volts} V out, ${fmtA(out)})`, why: 'A linear regulator burns the voltage it drops: (volts in − volts out) × amps. Above about 1 W it needs a heatsink; a buck converter wastes far less.', parts: [r.part], wires: [] });
    }
  }
  // Wires: thick enough? Do they lose much voltage?
  for (const w of doc.wires) {
    const a = amps.get(w.id);
    if (a === undefined) continue;
    const pa = pinOf(doc, defs, w.a);
    const cable = pa?.kind === 'port';
    const g = gaugeOf(w, doc, defs);
    const max = cable ? CABLE_AMPS : GAUGES[g].amps;
    if (Math.abs(a) > max) {
      const fit = GAUGE_ORDER.find((x) => GAUGES[x].amps >= Math.abs(a) * 1.25);
      findings.push({
        sev: Math.abs(a) > 2 * max ? 'error' : 'warn',
        text: `${cable ? 'The cable' : `The ${GAUGES[g].name}`} from ${label(...splitKey(w.a))} to ${label(...splitKey(w.b))} carries ${fmtA(a)}; it takes about ${max} A`,
        why: fit ? `A wire that is too thin for its current gets hot and can melt. ${GAUGES[fit].name} takes ${GAUGES[fit].amps} A.` : 'No wire in the drawer takes that much: run two in parallel, or draw less.',
        parts: [],
        wires: [w.id],
      });
    }
  }
  // Resistors, LEDs and diodes: within their ratings?
  for (const part of doc.parts) {
    const d = defs(part.def);
    const e = d?.elec;
    if (!e) continue;
    const a = partAmps.get(part.id) ?? 0;
    if (e.t === 'resistor') {
      const w = a * a * e.ohms;
      if (w > e.watts) {
        worst(part.id, 'burnt');
        findings.push({ sev: 'error', text: `${name(part.id)} turns ${w.toFixed(2)} W into heat; it is made for ${e.watts} W`, why: `A resistor makes heat: amps × amps × ohms. ${fmtA(a)} through ${ohmText(e.ohms)} is too much for it: it browns, smokes and breaks. Use a bigger value or a ${Math.ceil(w * 2)} W resistor.`, parts: [part.id], wires: [] });
      } else if (a > 1e-4) worst(part.id, 'on');
    } else if (e.t === 'diode') {
      if (a * 1000 > e.maxMa) {
        worst(part.id, 'burnt');
        findings.push({ sev: 'error', text: `${name(part.id)} gets ${fmtA(a)}; ${e.led ? 'an LED' : 'it'} takes ${e.maxMa} mA at most`, why: e.led ? 'An LED does not limit its own current: it takes all it can get and burns out. A resistor in series sets how much it gets.' : 'Too much current through a diode heats it until it breaks.', parts: [part.id], wires: [] });
      } else if (a > (e.led ? 5e-4 : 1e-3)) worst(part.id, 'on');
    } else if (e.t === 'cap') {
      const u = (volts.get(`${part.id}.${e.a}`) ?? 0) - (volts.get(`${part.id}.${e.b}`) ?? 0);
      if (u > e.volts) {
        worst(part.id, 'burnt');
        findings.push({ sev: 'error', text: `${name(part.id)} sees ${fmtV(u)}, more than its ${e.volts} V`, why: 'A capacitor above its voltage breaks inside; an electrolytic one can burst. Use one rated well above the voltage.', parts: [part.id], wires: [] });
      } else if (e.polar && u < -0.5) {
        worst(part.id, 'burnt');
        findings.push({ sev: 'error', text: `${name(part.id)} is the wrong way round`, why: 'An electrolytic capacitor has a + and a − leg (the stripe is −). Backwards it heats up and can burst.', parts: [part.id], wires: [] });
      }
    } else if (e.t === 'motor' && a > 0.05) worst(part.id, 'on');
  }
  // Board pins: driven too hard, or fed a voltage they can't take?
  for (const pd of b.pinDrivers) {
    const a = srcAmps(pd.src, v);
    const [pid, pn] = splitKey(pd.key);
    if (Math.abs(a) * 1000 > pd.maxMa) {
      worst(pid, 'over');
      findings.push({ sev: 'error', text: `${label(pid, pn)} gives ${fmtA(a)}; the pin takes ${pd.maxMa} mA at most`, why: 'A board pin is a tiny switch inside the chip. Pull more than its limit and it burns out; the whole chip can go. Add a resistor, or let a transistor or driver carry the load.', parts: [pid], wires: [] });
    }
  }
  for (const inp of b.inputs) {
    const u = volts.get(inp.key);
    if (u === undefined) continue;
    if (u > inp.logic + 0.5) {
      const [pid, pn] = splitKey(inp.key);
      worst(pid, 'over');
      findings.push({ sev: 'error', text: `${label(pid, pn)} sees ${fmtV(u)}, but it is a ${inp.logic} V pin`, why: inp.logic === 3.3 ? 'A Raspberry Pi pin takes 3.3 V at most; 5 V slowly or suddenly kills it. Bring the level down with a divider (two resistors) or a level shifter.' : 'More than its logic voltage pushes current into the chip through its protection diodes until they fail.', parts: [pid], wires: [] });
    }
  }
  for (const part of doc.parts) if (!parts.has(part.id) && defs(part.def)?.kind !== 'breadboard') parts.set(part.id, 'off');
  return { volts, amps, partAmps, parts, blown, findings };
}

const round = (x: number) => Math.round(x * 1000) / 1000;

/** 4700 → "4.7 kΩ". */
export function ohmText(ohms: number): string {
  if (ohms >= 1e6) return `${trim(ohms / 1e6)} MΩ`;
  if (ohms >= 1e3) return `${trim(ohms / 1e3)} kΩ`;
  return `${trim(ohms)} Ω`;
}
const trim = (x: number) => String(Math.round(x * 100) / 100);
