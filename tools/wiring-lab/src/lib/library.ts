/**
 * Every electrical part in SENNEP's parts list (components/inventory.csv), plus parts whose
 * settings change them (a battery's cells, a buck's output). Pin orders are the modules' own
 * silkscreen as commonly sold; a clone can differ, so the inspector says to check yours.
 *
 * A parametric part's id carries its settings: `bat?s=3&p=6&cell=4.2&mah=3400`. `variant` turns
 * that into a part; the same settings always give the same part.
 */
import { edgePins } from './parts';
import { BLADE_PARAMS, bladeFuse, CAP_PARAMS, capacitor, FUSE_PARAMS, fuse, PASSIVES, RESISTOR_PARAMS, resistor } from './passives';
import type { Cap, Dir, ParamDef, PartDef, PinDef } from './types';

type P = Omit<PinDef, 'id' | 'label' | 'x' | 'y' | 'd'> & { id?: string; label?: string };
type Edge = 'top' | 'right' | 'bottom' | 'left';
const sig = (needs: Cap[], extra: Partial<PinDef> = {}): P => ({ kind: 'sig', needs, ...extra });
const vcc = (min: number, max: number, extra: Partial<PinDef> = {}): P => ({ kind: 'vcc', min, max, ...extra });
const gnd: P = { kind: 'gnd' };
const pass = (extra: Partial<PinDef> = {}): P => ({ kind: 'pass', ...extra });
const even = (n: number) => Math.round(n / 2) * 2;

function mod(id: string, name: string, covers: string, look: string, w: number, h: number, pins: [string, Edge, P][], more: Partial<PartDef> = {}): PartDef {
  return { id, name, covers, kind: 'part', look, w, h, pins: edgePins(pins, w, h), ...more };
}

function nano(): PartDef {
  // USB-C at the left end; the rows 0.6 in apart, so it straddles a breadboard.
  const top = ['D12', 'D11', 'D10', 'D9', 'D8', 'D7', 'D6', 'D5', 'D4', 'D3', 'D2', 'GND', 'RST', 'D0', 'D1'];
  const bottom = ['D13', '3V3', 'AREF', 'A0', 'A1', 'A2', 'A3', 'A4', 'A5', 'A6', 'A7', '5V', 'RST2', 'GND2', 'VIN'];
  const pwm = new Set([3, 5, 6, 9, 10, 11]);
  const mk = (name: string, x: number, y: number, d: Dir): PinDef => {
    const base = { id: name, label: name.replace(/2$/, ''), x, y, d, place: d === 0 ? 'top row' : 'bottom row' };
    const n = /^D(\d+)$/.exec(name)?.[1];
    if (n !== undefined) {
      const k = Number(n);
      const caps: Cap[] = ['io'];
      if (pwm.has(k)) caps.push('pwm');
      if (k === 2 || k === 3) caps.push('int');
      if (k === 0) caps.push('rx');
      if (k === 1) caps.push('tx');
      if (k === 13) caps.push('sck');
      if (k === 11) caps.push('mosi');
      if (k === 12) caps.push('miso');
      return { ...base, kind: 'io', caps, logic: 5, serial: k <= 1 };
    }
    const a = /^A(\d)$/.exec(name)?.[1];
    if (a !== undefined) {
      const k = Number(a);
      const caps: Cap[] = k >= 6 ? ['analog'] : ['io', 'analog'];
      if (k === 4) caps.push('sda');
      if (k === 5) caps.push('scl');
      return { ...base, kind: 'io', caps, logic: 5 };
    }
    if (name.startsWith('GND')) return { ...base, kind: 'gnd' };
    if (name === '5V') return { ...base, kind: 'supply', volts: 5, maxMa: 400 };
    if (name === '3V3') return { ...base, kind: 'supply', volts: 3.3, maxMa: 50 };
    if (name === 'VIN') return { ...base, kind: 'vin', min: 7, max: 12, limit: 20 };
    return { ...base, kind: 'other' };
  };
  const pins: PinDef[] = [
    ...top.map((n, i) => mk(n, 6 + 2 * i, 0, 0)),
    ...bottom.map((n, i) => mk(n, 6 + 2 * i, 12, 2)),
    { id: 'USB', label: 'USB-C', x: 0, y: 6, d: 3, kind: 'port', port: 'usb', host: false, place: 'USB-C socket', ma: 100 },
  ];
  return { id: 'nano', name: 'Arduino Nano', covers: 'ATmega328P · CH340 · USB-C', kind: 'board', look: 'nano', w: 38, h: 12, title: 'NANO', pins, pluggable: true };
}

// ---- parametric parts -------------------------------------------------------------------------

const BATTERY_PARAMS: ParamDef[] = [
  { id: 's', label: 'Cells in series (S)', min: 1, max: 8, step: 1, default: 3 },
  { id: 'p', label: 'Cells in parallel (P)', min: 1, max: 10, step: 1, default: 6 },
  { id: 'cell', label: 'Full cell voltage', unit: 'V', min: 1, max: 4.4, step: 0.05, default: 4.2 },
  { id: 'nom', label: 'Nominal cell voltage', unit: 'V', min: 1, max: 4, step: 0.05, default: 3.6 },
  { id: 'mah', label: 'Cell capacity', unit: 'mAh', min: 100, max: 10000, step: 100, default: 3400 },
];

function battery(v: Record<string, number>): PartDef {
  const s = v.s!;
  const p = v.p!;
  const full = +(s * v.cell!).toFixed(2);
  const nom = +(s * v.nom!).toFixed(1);
  const ah = +((p * v.mah!) / 1000).toFixed(1);
  const w = even(20 + 12 * s);
  const h = even(Math.max(14, 8 + 3 * p));
  return {
    id: 'bat',
    name: `Battery ${s}S${p}P`,
    covers: `${nom} V nominal · ${full} V full · ${ah} Ah`,
    kind: 'part',
    look: 'battery',
    title: String(s),
    w,
    h,
    params: BATTERY_PARAMS,
    pins: [
      { id: '+', label: '+', x: w, y: even(h / 2 - 3), d: 1, kind: 'supply', volts: full, terminal: true, maxMa: Math.round(p * 10000) },
      { id: '-', label: '−', x: w, y: even(h / 2 + 3), d: 1, kind: 'gnd', terminal: true },
    ],
    notes: [`Li-ion: never below ${(s * 2.8).toFixed(1)} V (2.8 V a cell). Charge it through a BMS.`],
  };
}

const BUCK_PARAMS: ParamDef[] = [
  { id: 'v', label: 'Output', unit: 'V', min: 1.25, max: 30, step: 0.1, default: 5.1 },
  { id: 'a', label: 'Most current out', unit: 'A', min: 0.5, max: 20, step: 0.5, default: 3 },
];

function buck(v: Record<string, number>): PartDef {
  return {
    id: 'buck5',
    name: `Buck converter ${v.v} V`,
    covers: 'LM2596 / XL4015 / 5 A DC-DC · in up to 36 V',
    kind: 'part',
    look: 'buck',
    w: 34,
    h: 18,
    params: BUCK_PARAMS,
    pins: [
      { id: 'IN+', label: 'IN+', x: 0, y: 6, d: 3, kind: 'power-in', min: v.v! + 1.5, max: 36, terminal: true },
      { id: 'IN-', label: 'IN−', x: 0, y: 12, d: 3, kind: 'gnd', terminal: true },
      { id: 'OUT+', label: 'OUT+', x: 34, y: 6, d: 1, kind: 'supply', volts: v.v!, terminal: true, maxMa: (v.a ?? 3) * 1000 },
      { id: 'OUT-', label: 'OUT−', x: 34, y: 12, d: 1, kind: 'gnd', terminal: true },
    ],
    notes: ['Set the output with the trim pot and a multimeter before anything is connected to it.', 'An LM2596 module gives about 3 A; the XL4015-type 5 A modules 4–5 A with cooling.'],
  };
}

const STRIP_PARAMS: ParamDef[] = [{ id: 'n', label: 'LEDs', min: 1, max: 1000, step: 1, default: 150 }];

function strip(v: Record<string, number>): PartDef {
  const n = v.n!;
  return mod('ws2812', `LED strip (${n} LEDs)`, 'WS2812B · 5 V · 60 mA a LED at full white', 'strip', 40, 8, [
    ['5V', 'left', vcc(4.5, 5.5, { ma: n * 60 })],
    ['DIN', 'left', sig(['io'])],
    ['GND', 'left', gnd],
  ], { params: STRIP_PARAMS, notes: [`${n} LEDs draw up to ${((n * 60) / 1000).toFixed(1)} A at full white: power them from a buck, not a board pin. Put 330 Ω in DIN and 1000 µF across 5V/GND.`] });
}

const MB102_PARAMS: ParamDef[] = [
  { id: 'top', label: 'Top rails', unit: 'V', min: 3.3, max: 5, step: 1.7, default: 5 },
  { id: 'bottom', label: 'Bottom rails', unit: 'V', min: 3.3, max: 5, step: 1.7, default: 3.3 },
];

function mb102(v: Record<string, number>): PartDef {
  // Sits on the end of a breadboard; its legs land in the four rails (see parts.ts breadboard).
  return {
    id: 'mb102',
    name: 'Breadboard power supply',
    covers: `MB102 · top ${v.top} V, bottom ${v.bottom} V · in 6.5–12 V or USB`,
    kind: 'part',
    look: 'mb102',
    w: 22,
    h: 46,
    params: MB102_PARAMS,
    pins: [
      { id: 'T+', label: '+', x: 4, y: 4, d: 0, kind: 'supply', volts: v.top!, maxMa: 700 },
      { id: 'T-', label: '−', x: 4, y: 6, d: 0, kind: 'gnd' },
      { id: 'B+', label: '+', x: 4, y: 40, d: 2, kind: 'supply', volts: v.bottom!, maxMa: 700 },
      { id: 'B-', label: '−', x: 4, y: 42, d: 2, kind: 'gnd' },
      { id: 'IN', label: 'DC in', x: 22, y: 16, d: 1, kind: 'power-in', min: 6.5, max: 12, terminal: true },
      { id: 'IN-', label: 'DC −', x: 22, y: 22, d: 1, kind: 'gnd', terminal: true },
      { id: 'USB', label: 'USB', x: 22, y: 30, d: 1, kind: 'port', port: 'usb', host: false, optional: true },
    ],
    notes: ['Set each side to 5 V or 3.3 V with its jumper. 700 mA at most.'],
  };
}

interface Parametric {
  base: string;
  params: ParamDef[];
  make: (v: Record<string, number>) => PartDef;
}

const PARAMETRIC: Parametric[] = [
  { base: 'bat', params: BATTERY_PARAMS, make: battery },
  { base: 'buck5', params: BUCK_PARAMS, make: buck },
  { base: 'ws2812', params: STRIP_PARAMS, make: strip },
  { base: 'mb102', params: MB102_PARAMS, make: mb102 },
  { base: 'res', params: RESISTOR_PARAMS, make: resistor },
  { base: 'cap', params: CAP_PARAMS, make: capacitor },
  { base: 'fuse', params: FUSE_PARAMS, make: fuse },
  { base: 'blade', params: BLADE_PARAMS, make: bladeFuse },
];

/** `bat?s=4&p=2` → that battery. Missing settings take their defaults. */
export function variant(id: string): PartDef | undefined {
  const [base, query] = id.split('?') as [string, string | undefined];
  const p = PARAMETRIC.find((x) => x.base === base);
  if (!p) return undefined;
  const given = new URLSearchParams(query ?? '');
  const v: Record<string, number> = {};
  for (const d of p.params) {
    const raw = Number(given.get(d.id));
    const n = given.has(d.id) && Number.isFinite(raw) ? raw : d.default;
    v[d.id] = Math.min(d.max ?? n, Math.max(d.min ?? n, n));
  }
  const def = p.make(v);
  return { ...def, id: variantId(base, p.params, v) };
}

export function variantId(base: string, params: ParamDef[], v: Record<string, number>): string {
  return `${base}?${params.map((d) => `${d.id}=${v[d.id] ?? d.default}`).join('&')}`;
}

export function paramValues(id: string): Record<string, number> {
  const [, query] = id.split('?');
  const out: Record<string, number> = {};
  for (const [k, val] of new URLSearchParams(query ?? '')) out[k] = Number(val);
  return out;
}

export function isParametric(id: string): boolean {
  return PARAMETRIC.some((p) => p.base === id.split('?')[0]);
}

// ---- the parts list ---------------------------------------------------------------------------

export const LIBRARY: PartDef[] = [
  nano(),
  mod('l298n', 'Motor driver 2 A ×2', 'L298N dual H-bridge · 5–35 V', 'l298n', 34, 34, [
    ['OUT1', 'left', { kind: 'motor-out', terminal: true }],
    ['OUT2', 'left', { kind: 'motor-out', terminal: true }],
    ['OUT3', 'right', { kind: 'motor-out', terminal: true }],
    ['OUT4', 'right', { kind: 'motor-out', terminal: true }],
    ['12V', 'bottom', { kind: 'power-in', min: 5, max: 35, terminal: true }],
    ['GND', 'bottom', { ...gnd, terminal: true }],
    ['5V', 'bottom', { kind: 'supply', volts: 5, terminal: true, maxMa: 500 }],
    ['ENA', 'bottom', sig(['pwm'], { optional: true })],
    ['IN1', 'bottom', sig(['io'])],
    ['IN2', 'bottom', sig(['io'])],
    ['IN3', 'bottom', sig(['io'], { optional: true })],
    ['IN4', 'bottom', sig(['io'], { optional: true })],
    ['ENB', 'bottom', sig(['pwm'], { optional: true })],
  ], { notes: ['ENA/ENB have jumpers (full speed); pull them to drive speed by PWM. The 5V terminal is an output while the 12V input is above 7 V and the 5V jumper is on.', 'Drops about 2 V: a 12 V motor sees about 10 V.'] }),
  mod('bms3s', 'BMS 3S 40 A', 'HW-288 · balancing · cuts on the minus side', 'bms', 40, 20, [
    ['B-', 'left', pass({ label: 'B−', terminal: true })],
    ['B1', 'left', pass({ optional: true, label: 'B1 (3.7 V)' })],
    ['B2', 'left', pass({ optional: true, label: 'B2 (7.4 V)' })],
    ['B+', 'left', pass({ terminal: true })],
    ['P+', 'right', pass({ terminal: true })],
    ['P-', 'right', pass({ label: 'P−', terminal: true })],
  ], { passes: [['B+', 'P+'], ['B-', 'P-']], trip: 40, notes: ['B− to the pack\'s −, B1/B2 to the joints between the cell groups, B+ to the pack\'s +. Loads and the charger go on P+ / P−.', 'B+ and P+ are the same copper; the BMS switches the minus side.'] }),
  mod('charger3s', 'Charger 3S 12.6 V', '2 A · EU plug', 'adapter', 24, 14, [
    ['+', 'right', { kind: 'supply', volts: 12.6, terminal: true, maxMa: 2000 }],
    ['-', 'right', { ...gnd, label: '−', terminal: true }],
  ], { notes: ['Charge through the BMS: + to P+, − to P−.'] }),
  mod('adapter12', 'Power adapter 12 V', 'Mains adapter', 'adapter', 24, 14, [
    ['+', 'right', { kind: 'supply', volts: 12, terminal: true, maxMa: 2000 }],
    ['-', 'right', { ...gnd, label: '−', terminal: true }],
  ]),
  mod('buckusb', 'Buck 5 V with USB-C', 'Battery to a Pi 5: 5.1 V 5 A, USB-C out', 'buckusb', 34, 18, [
    ['IN+', 'left', { kind: 'power-in', min: 7, max: 32, terminal: true }],
    ['IN-', 'left', { ...gnd, label: 'IN−', terminal: true }],
    ['USBC', 'right', { kind: 'port', port: 'usbc-power', host: true, volts: 5.1, label: 'USB-C out', maxMa: 5000 }],
  ], { notes: ['A Pi 5 wants 5.1 V at 5 A for full USB power; at 3 A it limits its USB ports to 600 mA.'] }),
  mod('l7805', '5 V regulator', 'L7805 · TO-220 · 1 A · in 7–35 V', 'to220', 12, 14, [
    ['IN', 'bottom', { kind: 'power-in', min: 7, max: 35 }],
    ['GND', 'bottom', gnd],
    ['OUT', 'bottom', { kind: 'supply', volts: 5, maxMa: 1000 }],
  ], { notes: ['Burns (in − 5 V) × current as heat: from 12 V at 0.5 A that is 3.5 W, so use the heatsink. Add 0.33 µF on IN and 0.1 µF on OUT.'] }),
  mod('ld1117', '3.3 V regulator', 'LD1117V33 · TO-220 · 800 mA', 'to220', 12, 14, [
    ['GND', 'bottom', gnd],
    ['OUT', 'bottom', { kind: 'supply', volts: 3.3, maxMa: 800 }],
    ['IN', 'bottom', { kind: 'power-in', min: 4.75, max: 15 }],
  ], { notes: ['Needs 10 µF (tantalum or low-ESR) on OUT to stay stable.'] }),
  mod('lm2596t12', '12 V switching regulator', 'LM2596T-12 · TO-220-5 · 3 A', 'to220', 14, 14, [
    ['VIN', 'bottom', { kind: 'power-in', min: 15, max: 40 }],
    ['OUT', 'bottom', { kind: 'supply', volts: 12, maxMa: 3000 }],
    ['GND', 'bottom', gnd],
    ['FB', 'bottom', pass({ optional: true })],
    ['ON', 'bottom', pass({ optional: true, label: 'ON/OFF' })],
  ], { notes: ['A bare chip: it needs the inductor, Schottky diode and capacitors from its datasheet. FB goes to OUT, ON/OFF to GND to run.'] }),
  mod('irfz44n', 'MOSFET (N, 55 V)', 'IRFZ44N · TO-220 · not logic level', 'to220', 12, 14, [
    ['G', 'bottom', sig(['io'], { pull: 'down', caution: 'The IRFZ44N needs about 10 V on its gate to switch fully. From a 5 V pin it is half on and runs hot: use a logic-level MOSFET or a gate driver.' })],
    ['D', 'bottom', pass({ label: 'D (load −)' })],
    ['S', 'bottom', gnd],
  ], { notes: ['The load goes between + and D; S to GND. Add 10 kΩ from G to S so it stays off while the board boots.'] }),
  mod('relay40', 'Car relay 40 A', '12 V coil · 5-pin (30, 85, 86, 87, 87a)', 'relay', 22, 22, [
    ['86', 'bottom', { kind: 'power-in', min: 9, max: 15, label: '86 coil +' }],
    ['85', 'bottom', { ...gnd, label: '85 coil −' }],
    ['30', 'top', pass({ terminal: true })],
    ['87', 'top', pass({ terminal: true })],
    ['87a', 'top', pass({ terminal: true, optional: true })],
  ], { passes: [['30', '87']], notes: ['With 12 V on its coil (86 + to 85 −) it pulls in: 30 joins 87. Without, 30 joins 87a. In power mode it follows its coil, so an e-stop in the coil wire cuts the motors.', 'The coil draws about 150 mA: switch it through a MOSFET or an e-stop, never straight from a board pin, and put a 1N4007 across it (stripe to 86) for the spike when it lets go.'] }),
  mod('xt60', 'XT60 connector', 'Male + female pair', 'xt60', 16, 10, [
    ['+A', 'left', pass({ label: '+' })],
    ['-A', 'left', pass({ label: '−' })],
    ['+B', 'right', pass({ label: '+' })],
    ['-B', 'right', pass({ label: '−' })],
  ], { passes: [['+A', '+B'], ['-A', '-B']] }),
  mod('estop', 'Emergency stop', '40 mm mushroom · NC contact', 'estop', 14, 14, [
    ['1', 'bottom', pass({ terminal: true })],
    ['2', 'bottom', pass({ terminal: true })],
  ], { passes: [['1', '2']], switch: 'push', notes: ['Normally closed: pressing it breaks the circuit. In power mode, click it to press.'] }),
  mod('keyswitch', 'Key switch', 'S1203 12 mm · on/off', 'switch', 10, 10, [
    ['1', 'bottom', pass({ terminal: true })],
    ['2', 'bottom', pass({ terminal: true })],
  ], { passes: [['1', '2']], switch: 'toggle' }),
  mod('toggle', 'Toggle switch', 'MTS-103 · on-off-on · 6 A', 'switch', 12, 10, [
    ['1', 'bottom', pass({ terminal: true })],
    ['C', 'bottom', pass({ terminal: true })],
    ['3', 'bottom', pass({ terminal: true, optional: true })],
  ], { passes: [['C', '1']], switch: 'toggle' }),
  mod('e18', 'IR obstacle sensor', 'E18-D80NK · 3–80 cm · NPN', 'e18', 26, 10, [
    ['VCC', 'left', vcc(4.5, 5.5, { label: 'VCC (brown)', ma: 25 })],
    ['OUT', 'left', sig(['io'], { label: 'OUT (black)', out: true })],
    ['GND', 'left', { ...gnd, label: 'GND (blue)' }],
  ], { notes: ['Open collector: use INPUT_PULLUP. LOW = something in front.'] }),
  mod('qmc5883l', 'Compass', 'GY-271 · QMC5883L · I²C', 'imu', 16, 12, [
    ['VCC', 'bottom', vcc(3, 5.5)],
    ['GND', 'bottom', gnd],
    ['SCL', 'bottom', sig(['scl'])],
    ['SDA', 'bottom', sig(['sda'])],
    ['DRDY', 'bottom', sig(['int'], { optional: true, out: true })],
  ], { notes: ['Keep it away from motors and the battery cables; they bend the field it measures.'] }),
  mod('neo6m', 'GPS', 'GY-NEO6MV2 · u-blox NEO-6M · UART 9600', 'gps', 24, 20, [
    ['VCC', 'bottom', vcc(3.3, 5, { ma: 45 })],
    ['RX', 'bottom', sig(['tx'], { label: 'RX' })],
    ['TX', 'bottom', sig(['rx'], { label: 'TX', drives: 3.3, out: true })],
    ['GND', 'bottom', gnd],
  ], { notes: ['Its RX takes 3.3 V: from a 5 V TX put a divider (1 kΩ / 2 kΩ) in the line.'] }),
  mod('ina219', 'Current monitor', 'INA219 · I²C · up to 26 V, ±3.2 A', 'imu', 20, 14, [
    ['VCC', 'left', vcc(3, 5.5)],
    ['GND', 'left', gnd],
    ['SCL', 'left', sig(['scl'])],
    ['SDA', 'left', sig(['sda'])],
    ['VIN+', 'right', pass({ terminal: true })],
    ['VIN-', 'right', pass({ label: 'VIN−', terminal: true })],
  ], { passes: [['VIN+', 'VIN-']], notes: ['Goes in the + line of what it measures: supply to VIN+, load to VIN−.'] }),
  mod('tsl2561', 'Light sensor', 'TSL2561 · I²C · 3.3 V only', 'tof', 16, 12, [
    ['VCC', 'bottom', vcc(2.7, 3.6)],
    ['GND', 'bottom', gnd],
    ['SCL', 'bottom', sig(['scl'])],
    ['SDA', 'bottom', sig(['sda'])],
    ['ADDR', 'bottom', sig(['io'], { optional: true })],
    ['INT', 'bottom', sig(['int'], { optional: true, out: true })],
  ], { notes: ['Most of these boards have no regulator: 3.3 V only.'] }),
  mod('ds3231', 'Real-time clock', 'DS3231 · ZS-042 · I²C', 'rtc', 30, 18, [
    ['32K', 'left', sig(['io'], { optional: true, out: true })],
    ['SQW', 'left', sig(['int'], { optional: true, out: true })],
    ['SCL', 'left', sig(['scl'])],
    ['SDA', 'left', sig(['sda'])],
    ['VCC', 'left', vcc(3.3, 5.5)],
    ['GND', 'left', gnd],
  ], { notes: ['The ZS-042 tries to charge its coin cell: with a CR2032 (not rechargeable), remove the diode or the 200 Ω resistor next to it.'] }),
  mod('mic', 'Microphone module', 'KY-038 type · analog + threshold', 'mic', 22, 10, [
    ['AO', 'left', sig(['analog'], { optional: true, out: true })],
    ['GND', 'left', gnd],
    ['VCC', 'left', vcc(3.3, 5.5)],
    ['DO', 'left', sig(['io'], { out: true })],
  ]),
  mod('vs1838b', 'IR receiver', 'VS1838B · 38 kHz', 'irrx', 8, 10, [
    ['OUT', 'bottom', sig(['io'], { out: true })],
    ['GND', 'bottom', gnd],
    ['VCC', 'bottom', vcc(2.7, 5.5)],
  ]),
  mod('doorswitch', 'Door switch', 'MC-38 magnetic · NC', 'switch', 14, 8, [
    ['IN', 'left', sig(['io'], { out: true })],
    ['GND', 'left', gnd],
  ], { switch: 'toggle', notes: ['Closed while the magnet is near. Use INPUT_PULLUP.'] }),
  mod('ld19', 'LiDAR', 'LDROBOT LD19 / STL-19P · 360° · UART 230400', 'lidar', 30, 30, [
    ['P5V', 'bottom', vcc(4.5, 5.5, { label: 'P5V', ma: 290 })],
    ['GND', 'bottom', gnd],
    ['TX', 'bottom', sig(['rx'], { drives: 3.3, out: true })],
    ['PWM', 'bottom', sig(['pwm'], { optional: true })],
  ], { notes: ['TX is 3.3 V UART at 230400 baud. Leave PWM free for the default 10 Hz. On SENNEP it goes to the Pi through the CP2102 adapter.'] }),
  mod('cp2102', 'USB-UART adapter', 'CP2102 · 3.3 V logic', 'cp2102', 26, 12, [
    ['USB', 'left', { kind: 'port', port: 'usb', host: false, ma: 20 }],
    ['5V', 'right', { kind: 'supply', volts: 5, maxMa: 500 }],
    ['GND', 'right', gnd],
    ['TXD', 'right', { kind: 'io', caps: ['io', 'tx'], logic: 3.3 }],
    ['RXD', 'right', { kind: 'io', caps: ['io', 'rx'], logic: 3.3 }],
    ['3V3', 'right', { kind: 'supply', volts: 3.3, maxMa: 50 }],
  ], { notes: ['Its 5V pin is the USB host\'s 5 V: a LiDAR on it shares the Pi\'s USB budget.'] }),
  mod('aicam', 'AI camera', 'Raspberry Pi AI Camera · IMX500 · CSI', 'camera', 16, 16, [
    ['CSI', 'bottom', { kind: 'port', port: 'csi', host: false, label: 'CSI ribbon' }],
  ], { notes: ['The Pi 5 needs the 22-to-15-pin ribbon (the camera comes with one).'] }),
  mod('nrf24', '2.4 GHz radio', 'NRF24L01 · SPI · 3.3 V only', 'nrf', 30, 12, [
    ['GND', 'left', gnd],
    ['VCC', 'left', vcc(1.9, 3.6, { ma: 15 })],
    ['CE', 'left', sig(['io'])],
    ['CSN', 'left', sig(['io'])],
    ['SCK', 'left', sig(['sck'])],
    ['MOSI', 'left', sig(['mosi'])],
    ['MISO', 'left', sig(['miso'], { drives: 3.3, out: true })],
    ['IRQ', 'left', sig(['int'], { optional: true, out: true })],
  ], { notes: ['3.3 V only, but its data pins take 5 V. Solder 10 µF across VCC and GND or it drops packets: the 3.3 V pin of a board is weak for it.'] }),
  mod('esp01', 'Wi-Fi module', 'ESP8266 ESP-01 · 3.3 V only', 'esp01', 24, 14, [
    ['GND', 'left', gnd],
    ['GPIO2', 'left', sig(['io'], { optional: true })],
    ['GPIO0', 'left', sig(['io'], { optional: true })],
    ['RX', 'left', sig(['tx'])],
    ['TX', 'left', sig(['rx'], { drives: 3.3, out: true })],
    ['EN', 'left', vcc(3, 3.6, { label: 'EN (CH_PD)' })],
    ['RST', 'left', sig(['io'], { optional: true })],
    ['VCC', 'left', vcc(3, 3.6, { ma: 250 })],
  ], { notes: ['Pulls 250 mA bursts: a board\'s 3.3 V pin can\'t; use a regulator. Its RX takes 3.3 V.'] }),
  mod('tca9548a', 'I²C multiplexer', 'TCA9548A · 8 buses', 'mux', 20, 34, [
    ['VIN', 'left', vcc(1.65, 5.5)],
    ['GND', 'left', gnd],
    ['SDA', 'left', sig(['sda'])],
    ['SCL', 'left', sig(['scl'])],
    ['RST', 'left', sig(['io'], { optional: true })],
    ['A0', 'left', sig(['io'], { optional: true })],
    ['A1', 'left', sig(['io'], { optional: true })],
    ['A2', 'left', sig(['io'], { optional: true })],
    ...[0, 1, 2, 3, 4, 5, 6, 7].flatMap((n): [string, Edge, P][] => [
      [`SD${n}`, 'right', { kind: 'io', caps: ['sda'], logic: 5 }],
      [`SC${n}`, 'right', { kind: 'io', caps: ['scl'], logic: 5 }],
    ]),
  ], { notes: ['Eight sensors with the same address (VL53L0X) each get their own bus.'] }),
  mod('screen7', '7" touch screen', 'Waveshare 7" HDMI LCD (C) · 1024×600', 'screen', 70, 44, [
    ['HDMI', 'bottom', { kind: 'port', port: 'hdmi', host: false }],
    ['USB', 'bottom', { kind: 'port', port: 'usb', host: false, label: 'USB touch + power', ma: 500 }],
  ], { notes: ['Powered and touched through its USB cable; the picture comes over HDMI.'] }),
  mod('lcd2004', 'LCD 20×4 (I²C)', 'HD44780 + PCF8574 backpack · 5 V', 'lcd', 50, 24, [
    ['GND', 'left', gnd],
    ['VCC', 'left', vcc(4.5, 5.5, { ma: 30 })],
    ['SDA', 'left', sig(['sda'])],
    ['SCL', 'left', sig(['scl'])],
  ]),
  mod('lcd1602', 'LCD 16×2 (I²C)', 'HD44780 + PCF8574 backpack · 5 V', 'lcd', 42, 18, [
    ['GND', 'left', gnd],
    ['VCC', 'left', vcc(4.5, 5.5, { ma: 25 })],
    ['SDA', 'left', sig(['sda'])],
    ['SCL', 'left', sig(['scl'])],
  ]),
  mod('keypad', 'Keypad 3×4', 'Membrane · 7 wires', 'keypad', 24, 30, [
    ...['R1', 'R2', 'R3', 'R4', 'C1', 'C2', 'C3'].map((n): [string, Edge, P] => [n, 'bottom', sig(['io'])]),
  ]),
  mod('voltmeter', 'Voltmeter display', '0.28" · 0–30 V · 3 wires', 'voltmeter', 18, 10, [
    ['V+', 'left', { kind: 'power-in', min: 4.5, max: 30, label: 'V+ (red)' }],
    ['GND', 'left', { ...gnd, label: 'GND (black)' }],
    ['SENSE', 'left', { kind: 'power-in', min: 0, max: 30, label: 'Sense (yellow)' }],
  ]),
  mod('dfplayer', 'MP3 player', 'DFPlayer Mini · UART 9600 · 3 W out', 'dfplayer', 20, 20, [
    ['VCC', 'left', vcc(3.2, 5, { ma: 200 })],
    ['RX', 'left', sig(['tx'])],
    ['TX', 'left', sig(['rx'], { drives: 3.3, out: true })],
    ['DAC_R', 'left', { kind: 'io', caps: ['audio'], logic: 3.3, optional: true }],
    ['DAC_L', 'left', { kind: 'io', caps: ['audio'], logic: 3.3, optional: true }],
    ['SPK1', 'left', { kind: 'speaker-out' }],
    ['GND', 'left', gnd],
    ['SPK2', 'left', { kind: 'speaker-out' }],
    ['BUSY', 'right', sig(['io'], { optional: true, out: true })],
  ], { notes: ['Put 1 kΩ in its RX line from a 5 V board, or it hums.'] }),
  mod('speaker', 'Speaker 1 W 8 Ω', '8 Ω · 1 W', 'speaker', 16, 16, [
    ['+', 'left', { kind: 'speaker-in' }],
    ['-', 'left', { kind: 'speaker-in', label: '−' }],
  ]),
  mod('pam8403', 'Amplifier 2×3 W', 'PAM8403 · 5 V', 'amp', 20, 16, [
    ['5V', 'left', vcc(2.5, 5.5, { ma: 600 })],
    ['GND', 'left', gnd],
    ['L', 'left', sig(['audio'])],
    ['G', 'left', { ...gnd, label: 'G (audio)' }],
    ['R', 'left', sig(['audio'], { optional: true })],
    ['L+', 'right', { kind: 'speaker-out' }],
    ['L-', 'right', { kind: 'speaker-out', label: 'L−' }],
    ['R+', 'right', { kind: 'speaker-out', optional: true }],
    ['R-', 'right', { kind: 'speaker-out', label: 'R−', optional: true }],
  ], { notes: ['Bridged outputs: never join L− or R− to GND.'] }),
  mod('led3w', 'LED 3 W', 'High-power LED on a star · about 3.3 V, 700 mA', 'led3w', 14, 14, [
    ['+', 'left', { kind: 'power-in', min: 3, max: 3.6, ma: 700 }],
    ['-', 'left', { ...gnd, label: '−' }],
  ], { notes: ['Needs a constant-current driver (or a resistor and a MOSFET), never a board pin.'] }),
  mod('joystick', 'Joystick', 'KY-023 · XY + button', 'joystick', 20, 20, [
    ['GND', 'left', gnd],
    ['+5V', 'left', vcc(3.3, 5.5)],
    ['VRx', 'left', sig(['analog'], { out: true })],
    ['VRy', 'left', sig(['analog'], { out: true })],
    ['SW', 'left', sig(['io'], { out: true })],
  ], { notes: ['The button needs INPUT_PULLUP.'] }),
  ...PASSIVES,
  mod('pot', 'Potentiometer', '10 kΩ', 'pot', 14, 12, [
    ['1', 'bottom', { kind: 'power-in', min: 0, max: 50, label: '1 (+)' }],
    ['W', 'bottom', sig(['analog'], { label: 'wiper', out: true })],
    ['3', 'bottom', { ...gnd, label: '3 (−)' }],
  ]),
];

/** Ready-made settings for parametric parts in the parts list. */
export const PRESETS: { id: string; name: string; covers: string }[] = [
  { id: 'bat?s=3&p=6&cell=4.2&nom=3.6&mah=3400', name: 'Battery 3S6P (SENNEP)', covers: 'Samsung 35E 18650 · 11 V · 20 Ah' },
  { id: 'bat?s=3&p=1&cell=4.2&nom=3.7&mah=1000', name: 'LiPo 3S 1000 mAh', covers: 'Zippy Compact 35C · XT60' },
  { id: 'bat?s=1&p=1&cell=4.2&nom=3.6&mah=3400', name: '18650 cell', covers: 'Samsung INR18650-35E · 3400 mAh' },
  { id: 'buck5?v=5.1', name: 'Buck converter', covers: 'Adjustable output · LM2596 / 5 A DC-DC' },
  { id: 'ws2812?n=150', name: 'LED strip', covers: 'WS2812B · 5 m, 30 a metre' },
  { id: 'mb102?top=5&bottom=3.3', name: 'Breadboard power supply', covers: 'MB102 · 5 V and 3.3 V rails' },
  { id: 'res?ohms=220&w=0.25', name: 'Resistor 220 Ω', covers: 'For an LED from 5 V' },
  { id: 'res?ohms=1000&w=0.25', name: 'Resistor 1 kΩ', covers: 'Series resistor, dividers' },
  { id: 'res?ohms=10000&w=0.25', name: 'Resistor 10 kΩ', covers: 'Pull-up, pull-down, dividers' },
  { id: 'res?ohms=47000&w=0.25', name: 'Resistor 47 kΩ', covers: 'The top of SENNEP’s battery divider' },
  { id: 'cap?uf=0.1&v=50&polar=0', name: 'Capacitor 100 nF', covers: 'Ceramic · next to a chip or across a motor' },
  { id: 'cap?uf=10&v=25&polar=1', name: 'Capacitor 10 µF', covers: 'Electrolytic · on an NRF24 or a regulator' },
  { id: 'cap?uf=2200&v=25&polar=1', name: 'Capacitor 2200 µF', covers: 'Electrolytic · on SENNEP’s motor bus' },
  { id: 'fuse?a=5', name: 'Fuse 5 A', covers: '5×20 mm glass fuse in a holder' },
  { id: 'blade?a=15', name: 'Blade fuse 15 A', covers: 'Per motor driver' },
  { id: 'blade?a=30', name: 'Blade fuse 30 A', covers: 'Main fuse at the battery' },
];
