/**
 * The parts the lab knows. Every pin sits on an even grid position (0.1 in apart, like a header),
 * so a part dropped on a breadboard lands its pins in the holes. Board pin facts come from the
 * same sources as core/src/arduino/wiring/boards/*.toml.
 */
import type { Cap, Dir, PartDef, PinDef } from './types';

function pin(id: string, x: number, y: number, d: Dir, props: Partial<PinDef> & Pick<PinDef, 'kind'>): PinDef {
  return { id, label: id, x, y, d, ...props };
}

interface BoardPins {
  pwm: Set<number>;
  int: Set<number>;
  sda: number;
  scl: number;
  tx: Set<number>;
  rx: Set<number>;
  sck: number;
  mosi: number;
  miso: number;
}

function digital(n: number, x: number, y: number, d: Dir, b: BoardPins, extra?: Partial<PinDef>): PinDef {
  const caps: Cap[] = ['io'];
  if (b.pwm.has(n)) caps.push('pwm');
  if (b.int.has(n)) caps.push('int');
  if (n === b.sda) caps.push('sda');
  if (n === b.scl) caps.push('scl');
  if (b.tx.has(n)) caps.push('tx');
  if (b.rx.has(n)) caps.push('rx');
  if (n === b.sck) caps.push('sck');
  if (n === b.mosi) caps.push('mosi');
  if (n === b.miso) caps.push('miso');
  return pin('D' + n, x, y, d, { kind: 'io', caps, logic: 5, label: 'D' + n, serial: n <= 1, ...extra });
}

function powerHeader(x0: number, y: number): PinDef[] {
  const names = ['NC', 'IOREF', 'RESET', '3V3', '5V', 'GND', 'GND', 'VIN'];
  const ids = ['NC', 'IOREF', 'RESET', '3V3', '5V1', 'GND2', 'GND3', 'VIN'];
  return names.map((n, i) => {
    const place = 'power header';
    const at = x0 + 2 * i;
    if (n === '3V3') return pin(ids[i]!, at, y, 2, { kind: 'supply', volts: 3.3, label: n, place });
    if (n === '5V') return pin(ids[i]!, at, y, 2, { kind: 'supply', volts: 5, label: n, place, maxMa: 500 });
    if (n === 'GND') return pin(ids[i]!, at, y, 2, { kind: 'gnd', label: n, place });
    // Arduino: 7–12 V recommended, 6–20 V the limits; above 12 V the regulator runs hot.
    if (n === 'VIN') return pin(ids[i]!, at, y, 2, { kind: 'vin', min: 7, max: 12, limit: 20, label: n, place, ma: 100 });
    return pin(ids[i]!, at, y, 2, { kind: 'other', label: n, place });
  });
}

function mega(): PartDef {
  const b: BoardPins = {
    pwm: new Set([2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 44, 45, 46]),
    int: new Set([2, 3, 18, 19, 20, 21]),
    sda: 20,
    scl: 21,
    tx: new Set([1, 14, 16, 18]),
    rx: new Set([0, 15, 17, 19]),
    sck: 52,
    mosi: 51,
    miso: 50,
  };
  const top = { place: 'top header' };
  const P: PinDef[] = [
    pin('USB', 0, 10, 3, { kind: 'port', port: 'usb', host: false, label: 'USB', place: 'USB-B socket', ma: 200 }),
    pin('SCL', 18, 2, 0, { kind: 'io', caps: ['io', 'scl'], logic: 5, ...top }),
    pin('SDA', 20, 2, 0, { kind: 'io', caps: ['io', 'sda'], logic: 5, ...top }),
    pin('AREF', 22, 2, 0, { kind: 'other', ...top }),
    pin('GND1', 24, 2, 0, { kind: 'gnd', label: 'GND', ...top }),
  ];
  [13, 12, 11, 10, 9, 8].forEach((n, i) => P.push(digital(n, 26 + 2 * i, 2, 0, b, top)));
  [7, 6, 5, 4, 3, 2, 1, 0].forEach((n, i) => P.push(digital(n, 40 + 2 * i, 2, 0, b, top)));
  [14, 15, 16, 17, 18, 19, 20, 21].forEach((n, i) => P.push(digital(n, 58 + 2 * i, 2, 0, b, top)));
  P.push(...powerHeader(22, 40));
  for (let i = 0; i < 16; i++) {
    P.push(pin('A' + i, (i < 8 ? 40 : 58) + 2 * (i % 8), 40, 2, { kind: 'io', caps: ['io', 'analog'], logic: 5, place: 'analog header' }));
  }
  const place = 'right double header';
  for (let r = 0; r < 18; r++) {
    const y = 4 + 2 * r;
    if (r === 0) {
      P.push(pin('5V2', 76, y, 1, { kind: 'supply', volts: 5, label: '5V', jog: [0, -1], place }));
      P.push(pin('5V3', 78, y, 1, { kind: 'supply', volts: 5, label: '5V', place }));
    } else if (r === 17) {
      P.push(pin('GND4', 76, y, 1, { kind: 'gnd', label: 'GND', jog: [0, 1], place }));
      P.push(pin('GND5', 78, y, 1, { kind: 'gnd', label: 'GND', place }));
    } else {
      const a = 22 + 2 * (r - 1);
      P.push(digital(a, 76, y, 1, b, { jog: [0, -1], place, pairLabel: `D${a} · ${a + 1}` }));
      P.push(digital(a + 1, 78, y, 1, b, { place, hideLabel: true }));
    }
  }
  return {
    id: 'mega',
    name: 'Arduino Mega 2560',
    covers: 'ATmega2560 · 5 V logic',
    kind: 'board',
    look: 'arduino',
    w: 80,
    h: 42,
    title: 'MEGA 2560',
    strips: [[17, 1, 37, 3], [39, 1, 55, 3], [57, 1, 73, 3], [21, 39, 37, 41], [39, 39, 55, 41], [57, 39, 73, 41], [75, 3, 79, 39]],
    pins: P,
  };
}

function uno(): PartDef {
  const b: BoardPins = { pwm: new Set([3, 5, 6, 9, 10, 11]), int: new Set([2, 3]), sda: -1, scl: -1, tx: new Set([1]), rx: new Set([0]), sck: 13, mosi: 11, miso: 12 };
  const top = { place: 'top header' };
  const P: PinDef[] = [
    pin('USB', 0, 10, 3, { kind: 'port', port: 'usb', host: false, label: 'USB', place: 'USB-B socket', ma: 200 }),
    pin('SCL', 14, 2, 0, { kind: 'io', caps: ['io', 'scl'], logic: 5, ...top }),
    pin('SDA', 16, 2, 0, { kind: 'io', caps: ['io', 'sda'], logic: 5, ...top }),
    pin('AREF', 18, 2, 0, { kind: 'other', ...top }),
    pin('GND1', 20, 2, 0, { kind: 'gnd', label: 'GND', ...top }),
  ];
  [13, 12, 11, 10, 9, 8].forEach((n, i) => P.push(digital(n, 22 + 2 * i, 2, 0, b, top)));
  [7, 6, 5, 4, 3, 2, 1, 0].forEach((n, i) => P.push(digital(n, 36 + 2 * i, 2, 0, b, top)));
  P.push(...powerHeader(18, 40));
  for (let i = 0; i < 6; i++) {
    const caps: Cap[] = ['io', 'analog'];
    if (i === 4) caps.push('sda');
    if (i === 5) caps.push('scl');
    P.push(pin('A' + i, 36 + 2 * i, 40, 2, { kind: 'io', caps, logic: 5, place: 'analog header' }));
  }
  return {
    id: 'uno',
    name: 'Arduino Uno R3',
    covers: 'ATmega328P · 5 V logic',
    kind: 'board',
    look: 'arduino',
    w: 54,
    h: 42,
    title: 'UNO',
    strips: [[13, 1, 33, 3], [35, 1, 51, 3], [17, 39, 33, 41], [35, 39, 47, 41]],
    pins: P,
  };
}

/** The 40-pin header by physical pin number (pin 1 at index 1). */
const PI_HEADER = [
  '', '3V3', '5V', 'GPIO2', '5V', 'GPIO3', 'GND', 'GPIO4', 'GPIO14', 'GND', 'GPIO15', 'GPIO17', 'GPIO18',
  'GPIO27', 'GND', 'GPIO22', 'GPIO23', '3V3', 'GPIO24', 'GPIO10', 'GND', 'GPIO9', 'GPIO25', 'GPIO11', 'GPIO8',
  'GND', 'GPIO7', 'GPIO0', 'GPIO1', 'GPIO5', 'GND', 'GPIO6', 'GPIO12', 'GPIO13', 'GND', 'GPIO19', 'GPIO16',
  'GPIO26', 'GPIO20', 'GND', 'GPIO21',
];

function pi5(): PartDef {
  const pwm = new Set(['GPIO12', 'GPIO13', 'GPIO18', 'GPIO19']);
  const P: PinDef[] = [];
  for (let n = 1; n <= 40; n++) {
    const k = Math.floor((n - 1) / 2);
    const inner = n % 2 === 1;
    const name = PI_HEADER[n]!;
    const base: Partial<PinDef> = { label: name, place: `GPIO header, pin ${n}`, hideLabel: !inner };
    if (inner) base.jog = [1, 0];
    const x = 8 + 2 * k;
    const y = inner ? 4 : 2;
    if (name === '3V3') P.push(pin('P' + n, x, y, 0, { ...base, kind: 'supply', volts: 3.3 }));
    else if (name === '5V') P.push(pin('P' + n, x, y, 0, { ...base, kind: 'supply', volts: 5 }));
    else if (name === 'GND') P.push(pin('P' + n, x, y, 0, { ...base, kind: 'gnd' }));
    else {
      const caps: Cap[] = ['io'];
      if (pwm.has(name)) caps.push('pwm');
      if (name === 'GPIO2') caps.push('sda');
      if (name === 'GPIO3') caps.push('scl');
      // UART0 on 14/15; the Pi 5's extra UARTs (dtoverlay uart2-pi5 … uart4-pi5) on 4/5, 8/9, 12/13.
      if (['GPIO14', 'GPIO4', 'GPIO8', 'GPIO12'].includes(name)) caps.push('tx');
      if (['GPIO15', 'GPIO5', 'GPIO9', 'GPIO13'].includes(name)) caps.push('rx');
      if (name === 'GPIO11') caps.push('sck');
      if (name === 'GPIO10') caps.push('mosi');
      if (name === 'GPIO9') caps.push('miso');
      P.push(pin('P' + n, x, y, 0, { ...base, kind: 'io', caps, logic: 3.3 }));
    }
  }
  const usb = (id: string, y: number, label: string) => pin(id, 68, y, 1, { kind: 'port', port: 'usb', host: true, volts: 5, label, place: 'USB ports', maxMa: 1600 });
  P.push(usb('USB1', 6, 'USB 3'), usb('USB2', 12, 'USB 3'), usb('USB3', 22, 'USB 2'), usb('USB4', 28, 'USB 2'));
  // The Pi 5 wants 5.1 V at up to 5 A through its USB-C socket.
  P.push(pin('USBC', 14, 44, 2, { kind: 'port', port: 'usbc-power', host: false, min: 4.75, max: 5.25, label: 'USB-C power', place: 'USB-C power socket', ma: 3000 }));
  P.push(pin('HDMI0', 26, 44, 2, { kind: 'port', port: 'hdmi', host: true, label: 'HDMI 0', place: 'micro-HDMI 0' }));
  P.push(pin('HDMI1', 34, 44, 2, { kind: 'port', port: 'hdmi', host: true, label: 'HDMI 1', place: 'micro-HDMI 1' }));
  P.push(pin('CAM0', 46, 44, 2, { kind: 'port', port: 'csi', host: true, label: 'CAM 0', place: 'camera socket 0' }));
  P.push(pin('CAM1', 52, 44, 2, { kind: 'port', port: 'csi', host: true, label: 'CAM 1', place: 'camera socket 1' }));
  return {
    id: 'pi5',
    name: 'Raspberry Pi 5',
    covers: '40-pin header · 3.3 V logic',
    kind: 'board',
    look: 'pi',
    w: 68,
    h: 44,
    title: 'Raspberry Pi 5',
    strips: [[7, 1, 47, 5]],
    pins: P,
  };
}

/** Pins centred on their edges, 0.1 in apart, on even positions. */
export function edgePins(list: [string, 'top' | 'right' | 'bottom' | 'left', Omit<PinDef, 'id' | 'label' | 'x' | 'y' | 'd'> & { id?: string; label?: string }][], w: number, h: number): PinDef[] {
  const dirs = { top: 0, right: 1, bottom: 2, left: 3 } as const;
  const byEdge = new Map<string, typeof list>();
  for (const p of list) byEdge.set(p[1], [...(byEdge.get(p[1]) ?? []), p]);
  const out: PinDef[] = [];
  for (const [edge, ps] of byEdge) {
    const len = edge === 'top' || edge === 'bottom' ? w : h;
    let start = Math.round((len - 2 * (ps.length - 1)) / 2);
    if (start % 2) start -= 1;
    if (start < 2) start = 2;
    ps.forEach(([label, , props], i) => {
      const at = start + 2 * i;
      const x = edge === 'left' ? 0 : edge === 'right' ? w : at;
      const y = edge === 'top' ? 0 : edge === 'bottom' ? h : at;
      const { id, ...rest } = props;
      out.push({ id: id ?? label, label, x, y, d: dirs[edge as keyof typeof dirs], ...rest });
    });
  }
  return out;
}

/** A half-size breadboard: 30 columns, rows a–j, a + and − rail along each long side. */
function breadboard(): PartDef {
  const cols = 30;
  const W = 66;
  const H = 46;
  const rows = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'];
  // 0.3 in between rows e and f, as on a real breadboard, so a DIP chip straddles the gap.
  const rowY = [12, 14, 16, 18, 20, 26, 28, 30, 32, 34];
  const pins: PinDef[] = [];
  const buses: string[][] = [];
  const x = (c: number) => 4 + 2 * c;
  for (let c = 0; c < cols; c++) {
    const top: string[] = [];
    const bottom: string[] = [];
    rows.forEach((r, i) => {
      const id = r + (c + 1);
      pins.push({ id, label: id, x: x(c), y: rowY[i]!, d: 0, kind: 'hole', place: `breadboard ${id}` });
      (i < 5 ? top : bottom).push(id);
    });
    buses.push(top, bottom);
  }
  const rails: [string, number, string][] = [['T+', 4, 'top + rail'], ['T-', 6, 'top − rail'], ['B+', 40, 'bottom + rail'], ['B-', 42, 'bottom − rail']];
  for (const [name, y, place] of rails) {
    const bus: string[] = [];
    for (let c = 0; c < cols; c++) {
      if (c % 6 === 5) continue; // the gaps between groups of five
      const id = name + (c + 1);
      pins.push({ id, label: name, x: x(c), y, d: 0, kind: 'hole', place: `breadboard ${place}` });
      bus.push(id);
    }
    buses.push(bus);
  }
  return { id: 'breadboard', name: 'Breadboard (half)', covers: '400 holes · 2 rails each side', kind: 'breadboard', look: 'breadboard', w: W, h: H, pins, buses };
}

export const BUILTIN: PartDef[] = [
  mega(),
  uno(),
  pi5(),
  breadboard(),
  {
    id: 'hcsr04',
    name: 'Ultrasonic sensor',
    covers: 'HC-SR04 · 2 cm to 4 m',
    kind: 'part',
    look: 'sonar',
    w: 36,
    h: 16,
    pins: [
      pin('VCC', 14, 16, 2, { kind: 'vcc', min: 4.5, max: 5.5 }),
      pin('TRIG', 16, 16, 2, { kind: 'sig', needs: ['io'] }),
      pin('ECHO', 18, 16, 2, { kind: 'sig', needs: ['io'], drives: 5, out: true }),
      pin('GND', 20, 16, 2, { kind: 'gnd' }),
    ],
  },
  {
    id: 'vl53l0x',
    name: 'ToF distance sensor',
    covers: 'VL53L0X · I²C · up to 2 m',
    kind: 'part',
    look: 'tof',
    w: 20,
    h: 14,
    pins: edgePins(
      [
        ['VIN', 'bottom', { kind: 'vcc', min: 2.6, max: 5.5 }],
        ['GND', 'bottom', { kind: 'gnd' }],
        ['SCL', 'bottom', { kind: 'sig', needs: ['scl'] }],
        ['SDA', 'bottom', { kind: 'sig', needs: ['sda'] }],
        ['GPIO1', 'bottom', { kind: 'sig', needs: ['io'], optional: true, out: true }],
        ['XSHUT', 'bottom', { kind: 'sig', needs: ['io'], optional: true }],
      ],
      20,
      14,
    ),
  },
  {
    id: 'mpu6050',
    name: 'IMU (gyro + accel)',
    covers: 'MPU-6050 · GY-521 · I²C',
    kind: 'part',
    look: 'imu',
    w: 18,
    h: 14,
    pins: edgePins(
      [
        ['VCC', 'bottom', { kind: 'vcc', min: 3.3, max: 5.5 }],
        ['GND', 'bottom', { kind: 'gnd' }],
        ['SCL', 'bottom', { kind: 'sig', needs: ['scl'] }],
        ['SDA', 'bottom', { kind: 'sig', needs: ['sda'] }],
        ['XDA', 'bottom', { kind: 'sig', optional: true }],
        ['XCL', 'bottom', { kind: 'sig', optional: true }],
        ['AD0', 'bottom', { kind: 'sig', needs: ['io'], optional: true }],
        ['INT', 'bottom', { kind: 'sig', needs: ['int'], optional: true, out: true }],
      ],
      18,
      14,
    ),
  },
  {
    id: 'bts7960',
    name: 'Motor driver 43 A',
    covers: 'BTS7960 · IBT-2 · 6–27 V',
    kind: 'part',
    look: 'bts',
    w: 40,
    h: 40,
    pins: [
      pin('RPWM', 0, 12, 3, { kind: 'sig', needs: ['pwm'] }),
      pin('LPWM', 0, 14, 3, { kind: 'sig', needs: ['pwm'] }),
      pin('R_EN', 0, 16, 3, { kind: 'sig', needs: ['io'], pull: 'down' }),
      pin('L_EN', 0, 18, 3, { kind: 'sig', needs: ['io'], pull: 'down' }),
      pin('R_IS', 0, 20, 3, { kind: 'sig', needs: ['analog'], optional: true, out: true }),
      pin('L_IS', 0, 22, 3, { kind: 'sig', needs: ['analog'], optional: true, out: true }),
      pin('VCC', 0, 24, 3, { kind: 'vcc', min: 3.3, max: 5.5 }),
      pin('GND', 0, 26, 3, { kind: 'gnd' }),
      pin('B+', 40, 8, 1, { kind: 'power-in', min: 6, max: 27, terminal: true }),
      pin('B-', 40, 14, 1, { kind: 'gnd', label: 'B−', terminal: true }),
      pin('M+', 40, 26, 1, { kind: 'motor-out', terminal: true }),
      pin('M-', 40, 32, 1, { kind: 'motor-out', label: 'M−', terminal: true }),
    ],
  },
  {
    id: 'dcmotor',
    name: 'DC gear motor',
    covers: 'Brushed 12 V · RS PRO 834-7641 49:1',
    kind: 'part',
    look: 'motor',
    w: 44,
    h: 22,
    pins: [pin('M+', 0, 8, 3, { kind: 'motor-in' }), pin('M-', 0, 14, 3, { kind: 'motor-in', label: 'M−' })],
    // The datasheet gives no stall current; SENNEP's electronics plan estimates 15–25 A.
    elec: { t: 'motor', a: 'M+', b: 'M-', volts: 12, runA: 2, stallA: 20 },
    notes: ['Running free it draws about 2 A; held still (stalled) an estimated 20 A, not measured yet. Power mode can show both.'],
  },
  {
    id: 'sg90',
    name: 'Servo',
    covers: 'SG90 · MG90S · 4.8–6 V',
    kind: 'part',
    look: 'servo',
    w: 26,
    h: 12,
    pins: [
      pin('GND', 0, 4, 3, { kind: 'gnd' }),
      pin('V+', 0, 6, 3, { kind: 'vcc', min: 4.8, max: 6 }),
      pin('SIG', 0, 8, 3, { kind: 'sig', needs: ['io'] }),
    ],
  },
  {
    id: 'button',
    name: 'Push button',
    covers: 'PBS-110 · momentary, to GND (pull-up)',
    kind: 'part',
    look: 'button',
    w: 10,
    h: 10,
    pins: [pin('1', 4, 10, 2, { kind: 'sig', needs: ['io'], label: 'IN', out: true }), pin('2', 6, 10, 2, { kind: 'gnd', label: 'GND' })],
  },
  {
    id: 'led',
    name: 'LED with resistor',
    covers: 'Status light, 5 mm',
    kind: 'part',
    look: 'led',
    w: 8,
    h: 12,
    pins: [pin('SIG', 2, 12, 2, { kind: 'sig', needs: ['io'] }), pin('GND', 6, 12, 2, { kind: 'gnd' })],
  },
  {
    // Pin rows 0.4 in apart, so it straddles a breadboard's middle gap.
    id: 'shifter',
    name: 'Level shifter 4 ch',
    covers: 'BSS138 · 3.3 V ↔ 5 V',
    kind: 'part',
    look: 'shifter',
    w: 14,
    h: 8,
    pins: [
      pin('LV1', 2, 0, 0, { kind: 'sig', needs: ['io'], drives: 3.3, optional: true }),
      pin('LV2', 4, 0, 0, { kind: 'sig', needs: ['io'], drives: 3.3, optional: true }),
      pin('LV', 6, 0, 0, { kind: 'vcc', min: 1.8, max: 3.6 }),
      pin('LGND', 8, 0, 0, { kind: 'gnd', label: 'GND' }),
      pin('LV3', 10, 0, 0, { kind: 'sig', needs: ['io'], drives: 3.3, optional: true }),
      pin('LV4', 12, 0, 0, { kind: 'sig', needs: ['io'], drives: 3.3, optional: true }),
      pin('HV1', 2, 8, 2, { kind: 'sig', needs: ['io'], drives: 5, optional: true }),
      pin('HV2', 4, 8, 2, { kind: 'sig', needs: ['io'], drives: 5, optional: true }),
      pin('HV', 6, 8, 2, { kind: 'vcc', min: 3.3, max: 5.5 }),
      pin('HGND', 8, 8, 2, { kind: 'gnd', label: 'GND' }),
      pin('HV3', 10, 8, 2, { kind: 'sig', needs: ['io'], drives: 5, optional: true }),
      pin('HV4', 12, 8, 2, { kind: 'sig', needs: ['io'], drives: 5, optional: true }),
    ],
    channels: [['LV1', 'HV1'], ['LV2', 'HV2'], ['LV3', 'HV3'], ['LV4', 'HV4']],
  },


];
