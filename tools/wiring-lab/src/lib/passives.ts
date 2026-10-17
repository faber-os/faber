/**
 * Parts with two legs: resistors, LEDs, diodes, capacitors and fuses. Their legs are `pass` pins
 * (they only conduct, through the part), 0.1 or 0.4 in apart so they plug into a breadboard.
 * Figures: 5 mm LEDs at 20 mA (30 mA absolute); 1N4007 and 1N5819 and P6KE15A from their
 * datasheets; a ¼ W resistor.
 */
import type { Dir, Elec, ParamDef, PartDef, PinDef } from './types';

const leg = (id: string, label: string, x: number, y: number, d: Dir): PinDef => ({ id, label, x, y, d, kind: 'pass' });

/** The standard E12 values in one decade. */
export const E12 = [1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];

/** The E12 value at or above `ohms` (a resistor that is a little bigger is the safe side). */
export function e12Up(ohms: number): number {
  if (ohms <= 1) return 1;
  const dec = 10 ** Math.floor(Math.log10(ohms));
  for (const m of E12) if (m * dec >= ohms * 0.999) return round(m * dec);
  return round(10 * dec);
}

/** The E12 value nearest `ohms`. */
export function e12Near(ohms: number): number {
  const dec = 10 ** Math.floor(Math.log10(Math.max(ohms, 1)));
  let best = dec;
  for (const m of [...E12, 10]) if (Math.abs(Math.log(m * dec) - Math.log(ohms)) < Math.abs(Math.log(best) - Math.log(ohms))) best = m * dec;
  return round(best);
}
const round = (x: number) => Math.round(x * 100) / 100;

export const RESISTOR_PARAMS: ParamDef[] = [
  { id: 'ohms', label: 'Resistance', unit: 'Ω', min: 1, max: 10000000, step: 1, default: 220 },
  { id: 'w', label: 'Power rating', unit: 'W', min: 0.125, max: 10, step: 0.125, default: 0.25 },
];

export function resistor(v: Record<string, number>): PartDef {
  const ohms = v.ohms!;
  const elec: Elec = { t: 'resistor', a: '1', b: '2', ohms, watts: v.w! };
  return {
    id: 'res',
    name: `Resistor ${ohmShort(ohms)}`,
    covers: `${ohmShort(ohms)} · ${v.w} W`,
    kind: 'part',
    look: 'resistor',
    w: 12,
    h: 4,
    pins: [leg('1', '1', 2, 4, 2), leg('2', '2', 10, 4, 2)],
    elec,
    notes: [
      'A resistor holds current back: amps = volts across it ÷ ohms. Either way round.',
      'Three jobs, mostly: limit an LED\'s current, hold an input high or low (pull-up, pull-down), or split a voltage (a divider).',
      'It turns amps × amps × ohms into heat; a ¼ W one gets too hot above 0.25 W.',
    ],
  };
}

export function ohmShort(ohms: number): string {
  if (ohms >= 1e6) return `${trim(ohms / 1e6)}M`;
  if (ohms >= 1e3) return `${trim(ohms / 1e3)}k`;
  return `${trim(ohms)}Ω`;
}
const trim = (x: number) => String(Math.round(x * 100) / 100);

export const CAP_PARAMS: ParamDef[] = [
  { id: 'uf', label: 'Capacitance', unit: 'µF', min: 0.000001, max: 100000, step: 0.1, default: 100 },
  { id: 'v', label: 'Voltage rating', unit: 'V', min: 1, max: 1000, step: 1, default: 25 },
  { id: 'polar', label: 'Electrolytic (1) or ceramic (0)', min: 0, max: 1, step: 1, default: 1 },
];

export function capacitor(v: Record<string, number>): PartDef {
  const polar = v.polar! >= 1;
  const uf = v.uf!;
  const value = uf >= 1 ? `${trim(uf)} µF` : uf >= 0.001 ? `${trim(uf * 1000)} nF` : `${trim(uf * 1e6)} pF`;
  return {
    id: 'cap',
    name: `Capacitor ${value}`,
    covers: `${value} · ${v.v} V · ${polar ? 'electrolytic (+ and −)' : 'ceramic (either way)'}`,
    kind: 'part',
    look: polar ? 'elcap' : 'ceramic',
    w: 6,
    h: 8,
    pins: [leg('+', polar ? '+' : '1', 2, 8, 2), leg('-', polar ? '−' : '2', 4, 8, 2)],
    elec: { t: 'cap', a: '+', b: '-', uf, volts: v.v!, polar },
    notes: [
      'A capacitor is a tiny, fast battery: it smooths dips and spikes on a supply. It passes no steady current.',
      polar ? 'Electrolytic: the stripe marks −. Backwards or above its voltage it can burst.' : 'Ceramic: either way round. 100 nF next to a chip\'s power pins catches its fast spikes.',
    ],
  };
}

export const FUSE_PARAMS: ParamDef[] = [{ id: 'a', label: 'Rating', unit: 'A', min: 0.1, max: 60, step: 0.5, default: 5 }];

export function fuse(v: Record<string, number>): PartDef {
  return {
    id: 'fuse',
    name: `Fuse ${v.a} A`,
    covers: '5×20 mm glass fuse in a holder',
    kind: 'part',
    look: 'fuse',
    w: 22,
    h: 8,
    pins: [
      { id: 'A', label: 'A', x: 0, y: 4, d: 3, kind: 'pass', terminal: true },
      { id: 'B', label: 'B', x: 22, y: 4, d: 1, kind: 'pass', terminal: true },
    ],
    passes: [['A', 'B']],
    trip: v.a,
    notes: ['As close to the battery as it goes. It protects the wires after it: pick it below what the thinnest of them takes.'],
  };
}

export const BLADE_PARAMS: ParamDef[] = [{ id: 'a', label: 'Rating', unit: 'A', min: 1, max: 40, step: 1, default: 15 }];

export function bladeFuse(v: Record<string, number>): PartDef {
  return { ...fuse(v), id: 'blade', name: `Blade fuse ${v.a} A`, covers: 'Car blade fuse in an inline holder', look: 'blade', trip: v.a };
}

interface LedColour {
  id: string;
  name: string;
  vf: number;
  fill: string;
}
export const LED_COLOURS: LedColour[] = [
  { id: 'red', name: 'red', vf: 2.0, fill: '#e23b3b' },
  { id: 'yellow', name: 'yellow', vf: 2.1, fill: '#f2c037' },
  { id: 'green', name: 'green', vf: 2.2, fill: '#35b45a' },
  { id: 'blue', name: 'blue', vf: 3.0, fill: '#3b7be2' },
  { id: 'white', name: 'white', vf: 3.0, fill: '#f4f1e6' },
];

function led(c: LedColour): PartDef {
  return {
    id: `led-${c.id}`,
    name: `LED ${c.name}`,
    covers: `5 mm · about ${c.vf} V at 20 mA`,
    kind: 'part',
    look: 'led5',
    color: c.fill,
    w: 6,
    h: 8,
    pins: [leg('A', 'A (+, long leg)', 2, 8, 2), leg('K', 'K (−, flat side)', 4, 8, 2)],
    elec: { t: 'diode', a: 'A', k: 'K', vf: c.vf, rs: 12, maxMa: 30, led: c.fill, ma: 20 },
    notes: [
      'An LED lights when current runs from its long leg (A) to its short leg (K). It never limits its own current: it always needs a resistor in series.',
      `Resistor = (supply − ${c.vf} V) ÷ the current. From 5 V at 10 mA: (5 − ${c.vf}) ÷ 0.010 ≈ ${Math.round((5 - c.vf) / 0.01)} Ω.`,
    ],
  };
}

export const PASSIVES: PartDef[] = [
  ...LED_COLOURS.map(led),
  {
    id: 'irled',
    name: 'IR LED 940 nm',
    covers: 'Infrared · about 1.3 V at 20 mA (invisible)',
    kind: 'part',
    look: 'led5',
    color: '#7a6aa8',
    w: 6,
    h: 8,
    pins: [leg('A', 'A (+, long leg)', 2, 8, 2), leg('K', 'K (−)', 4, 8, 2)],
    elec: { t: 'diode', a: 'A', k: 'K', vf: 1.3, rs: 2, maxMa: 100, led: '#7a6aa8', ma: 20 },
    notes: ['Like any LED it needs a series resistor. A phone camera shows when it is on.'],
  },
  {
    id: '1n4007',
    name: 'Diode 1N4007',
    covers: '1 A · 1000 V · about 0.8 V drop',
    kind: 'part',
    look: 'diode',
    w: 12,
    h: 4,
    pins: [leg('A', 'A (+)', 2, 4, 2), leg('K', 'K (stripe)', 10, 4, 2)],
    elec: { t: 'diode', a: 'A', k: 'K', vf: 0.8, rs: 0.1, maxMa: 1000 },
    notes: ['A one-way valve for current: from A to the striped end only.', 'Across a relay coil or motor (stripe to +), it swallows the spike when the coil switches off.'],
  },
  {
    id: '1n5819',
    name: 'Schottky diode 1N5819',
    covers: '1 A · 40 V · about 0.4 V drop',
    kind: 'part',
    look: 'diode',
    w: 12,
    h: 4,
    pins: [leg('A', 'A (+)', 2, 4, 2), leg('K', 'K (stripe)', 10, 4, 2)],
    elec: { t: 'diode', a: 'A', k: 'K', vf: 0.4, rs: 0.1, maxMa: 1000 },
    notes: ['Like the 1N4007 but drops only about 0.4 V, so it wastes less.'],
  },
  {
    id: 'p6ke15a',
    name: 'TVS diode P6KE15A',
    covers: 'Clamps spikes above about 15 V · 600 W',
    kind: 'part',
    look: 'diode',
    w: 12,
    h: 4,
    pins: [leg('A', 'A (to −)', 2, 4, 2), leg('K', 'K (stripe, to +)', 10, 4, 2)],
    elec: { t: 'tvs', a: 'A', k: 'K', vbr: 15.5 },
    notes: ['Across a supply (stripe to +), it does nothing until a spike passes about 15 V; then it conducts and eats the spike. This is what protects the Mega when the motor relay opens.'],
  },
];

export function ledVf(d: PartDef): number | undefined {
  return d.elec?.t === 'diode' && d.elec.led ? d.elec.vf : undefined;
}
