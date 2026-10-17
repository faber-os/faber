import { describe, expect, it } from 'vitest';
import { buildSteps } from './build';
import { pairVerdict, runChecks } from './checks';
import { connectOptions } from './connect';
import { seatShifter, sennepExample } from './example';
import { geom, tooClose } from './geometry';
import { arrange, mountedOn } from './layout';
import { computeNets } from './nets';
import { CATALOG, listed, lookup } from './catalog';
import { applyFix, fixesFor } from './fixes';
import { simulate } from './power';
import { snapToBreadboard } from './layout';
import { routeAll } from './router';
import type { Diagram, Part } from './types';

const defs = lookup();

function example(): Diagram {
  const doc = sennepExample();
  seatShifter(doc);
  arrange(doc, defs);
  return doc;
}

function blank(...parts: [string, string, number, number][]): Diagram {
  return {
    name: 't',
    parts: parts.map(([id, def, x, y]): Part => ({ id, def, name: id, x, y, rot: 0 })),
    wires: [],
    done: {},
    seq: 1,
  };
}

function wire(doc: Diagram, a: string, b: string) {
  doc.wires.push({ id: 'w' + doc.seq++, a, b });
}

function errors(doc: Diagram) {
  const nets = computeNets(doc, defs);
  return runChecks(doc, defs, nets).filter((i) => i.sev === 'error').map((i) => i.text);
}

describe('parts', () => {
  it('sit on the 0.1 in grid, so they plug into a breadboard', () => {
    for (const d of listed()) {
      expect(d.w % 2, d.id).toBe(0);
      expect(d.h % 2, d.id).toBe(0);
      for (const p of d.pins) {
        expect(p.x % 2, `${d.id}.${p.id}`).toBe(0);
        expect(p.y % 2, `${d.id}.${p.id}`).toBe(0);
      }
      expect(new Set(d.pins.map((p) => p.id)).size, d.id).toBe(d.pins.length);
    }
  });
});

describe('the SENNEP example', () => {
  it('arranges with every part clear of the others', () => {
    const doc = example();
    const mounted = mountedOn(doc, defs);
    expect(mounted.get('lsh')).toBe('bb');
    const free = doc.parts.filter((p) => !mounted.has(p.id));
    for (let i = 0; i < free.length; i++) {
      for (let j = i + 1; j < free.length; j++) {
        const a = free[i]!;
        const b = free[j]!;
        if (defs(a.def)?.kind === 'breadboard' || defs(b.def)?.kind === 'breadboard') continue;
        expect(tooClose(geom(a, defs), geom(b, defs), 4), `${a.id} and ${b.id}`).toBe(false);
      }
    }
  });

  it('routes every wire, and no two nets ever share track', () => {
    const doc = example();
    const nets = computeNets(doc, defs);
    const { routes } = routeAll(doc, defs, nets);
    const owner = new Map<string, number>();
    for (const w of doc.wires) {
      const r = routes.get(w.id)!;
      expect(r.failed, `${w.a} → ${w.b}`).toBe(false);
      const net = nets.of.get(w.a)!.id;
      for (let i = 0; i < r.pts.length - 1; i++) {
        const [x0, y0] = r.pts[i]!;
        const [x1, y1] = r.pts[i + 1]!;
        const horiz = y0 === y1;
        const n = Math.abs(x1 - x0) + Math.abs(y1 - y0);
        for (let s = 1; s < n; s++) {
          const x = x0 + Math.sign(x1 - x0) * s;
          const y = y0 + Math.sign(y1 - y0) * s;
          const key = `${x},${y},${horiz ? 'h' : 'v'}`;
          const had = owner.get(key);
          expect(had === undefined || had === net, `${w.a} → ${w.b} overlaps another wire at ${x},${y}`).toBe(true);
          owner.set(key, net);
        }
      }
    }
  });

  it('passes every check', () => {
    expect(errors(example())).toEqual([]);
  });

  it('lists every wire once in the build steps, and the plugged shifter', () => {
    const doc = example();
    const groups = buildSteps(doc, defs, computeNets(doc, defs));
    const ids = groups.flatMap((g) => g.steps.map((s) => s.id));
    expect(ids.filter((id) => id.startsWith('w')).sort()).toEqual(doc.wires.map((w) => w.id).sort());
    expect(ids).toContain('plug:lsh');
  });
});

describe('checks', () => {
  it('catch a battery on a Mega: VIN runs hot on 12.6 V, its 5 V pin cannot take it', () => {
    const doc = blank(['m', 'mega', 0, 0], ['b', 'bat?s=3&p=1', -100, 0]);
    wire(doc, 'b.+', 'm.VIN');
    const nets = computeNets(doc, defs);
    expect(runChecks(doc, defs, nets).map((i) => i.code)).toContain('hot-regulator');
    const doc2 = blank(['m', 'mega', 0, 0], ['b', 'bat?s=3&p=1', -100, 0]);
    wire(doc2, 'b.+', 'm.5V1');
    expect(errors(doc2)).toContain('Two supplies fight: 5 V and 12.6 V are joined');
  });

  it('catch 5 V logic into a Pi pin', () => {
    const doc = blank(['pi', 'pi5', 0, 0], ['s', 'hcsr04', 0, -40]);
    wire(doc, 's.ECHO', 'pi.P11');
    expect(errors(doc)).toContain('s ECHO sends 5 V into pi GPIO17 (a 3.3 V pin)');
    expect(pairVerdict(doc, defs, 's.ECHO', 'pi.P11')).toBe('bad');
  });

  it('catch two supplies joined and a part over its voltage', () => {
    const doc = blank(['m', 'mega', 0, 0], ['b', 'bat?s=3&p=6', -100, 0], ['s', 'hcsr04', 0, -40]);
    wire(doc, 'b.+', 'm.5V1');
    wire(doc, 's.VCC', 'b.+');
    const e = errors(doc);
    expect(e).toContain('Two supplies fight: 5 V and 12.6 V are joined');
    expect(e).toContain('s VCC gets 12.6 V, more than its 5.5 V');
  });

  it('catch a motor on a board pin, and PWM asked of a pin without it', () => {
    const doc = blank(['m', 'mega', 0, 0], ['mo', 'dcmotor', -80, 0], ['d', 'bts7960', 0, -60]);
    wire(doc, 'mo.M+', 'm.D7');
    wire(doc, 'd.RPWM', 'm.D22');
    const e = errors(doc);
    expect(e).toContain('mo M+ must go to a motor driver');
    expect(e.some((t) => t.startsWith('d RPWM needs PWM; D22 can'))).toBe(true);
  });

  it('catch 3.3 V and 5 V board pins joined, and TX to TX through a shifter', () => {
    const direct = blank(['pi', 'pi5', -120, 0], ['m', 'mega', 0, 0]);
    wire(direct, 'pi.P8', 'm.D19');
    expect(errors(direct).some((t) => t.includes('logic joined'))).toBe(true);

    const shifted = blank(['pi', 'pi5', -120, 0], ['m', 'mega', 0, 0], ['s', 'shifter', -40, 60]);
    wire(shifted, 'pi.P8', 's.LV1');
    wire(shifted, 's.HV1', 'm.D18');
    expect(errors(shifted).some((t) => t.startsWith('TX to TX through s'))).toBe(true);
  });

  it('catch a part whose pins share a breadboard strip', () => {
    // An HC-SR04 turned so its four pins stand in one column of five holes.
    const doc = blank(['bb', 'breadboard', 0, 0], ['s', 'hcsr04', 0, 0], ['m', 'mega', 0, -100]);
    const s = doc.parts[1]!;
    s.rot = 1;
    const g = geom(s, defs);
    const vcc = g.pins.VCC!;
    // Column 3 (x = 8), VCC in row a (y = 12): move the sensor so its pins land there.
    s.x += 8 - vcc.x;
    s.y += 12 - vcc.y;
    expect(errors(doc).some((t) => t.startsWith('s: VCC and TRIG and ECHO and GND are joined'))).toBe(true);
  });
});

describe('connect to', () => {
  it('wires a sonar to free plain pins, 5 V and ground', () => {
    const doc = blank(['m', 'mega', 0, 0], ['s', 'hcsr04', 20, -40]);
    const r = connectOptions(doc, defs, 's', 'm');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    const wires = r.options[0]!.wires;
    const to = (pin: string) => wires.find(([a, b]) => a === pin || b === pin)?.find((k) => k !== pin);
    expect(to('s.VCC')).toMatch(/^m\.5V/);
    expect(to('s.GND')).toMatch(/^m\.GND/);
    for (const sig of ['s.TRIG', 's.ECHO']) {
      const pin = defs('mega')!.pins.find((p) => `m.${p.id}` === to(sig))!;
      expect(pin.caps, sig).not.toContain('pwm');
      expect(pin.caps, sig).not.toContain('tx');
    }
  });

  it('joins the I²C bus that is already there', () => {
    const doc = blank(['m', 'mega', 0, 0], ['i', 'mpu6050', 0, -40], ['t', 'vl53l0x', 40, -40]);
    wire(doc, 'i.SDA', 'm.D20');
    wire(doc, 'i.SCL', 'm.D21');
    const r = connectOptions(doc, defs, 't', 'm');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.options[0]!.wires).toContainEqual(['t.SDA', 'm.D20']);
    expect(r.options[0]!.wires).toContainEqual(['t.SCL', 'm.D21']);
  });

  it('offers a Pi and a Mega a USB cable and serial through a new level shifter', () => {
    const doc = blank(['pi', 'pi5', -120, 0], ['m', 'mega', 0, 0]);
    const r = connectOptions(doc, defs, 'pi', 'm');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.options.map((o) => o.label)[0]).toBe('USB cable');
    const serial = r.options[1]!;
    expect(serial.addParts.map((p) => p.def)).toEqual(['shifter']);
    const s = serial.addParts[0]!.id;
    expect(serial.wires).toContainEqual(['pi.P8', `${s}.LV1`]);
    expect(serial.wires).toContainEqual([`${s}.HV1`, 'm.D19']);
    expect(serial.wires).toContainEqual(['m.D18', `${s}.HV2`]);
    expect(serial.wires).toContainEqual([`${s}.LV2`, 'pi.P10']);
    // Applied, the result passes the checks.
    doc.parts.push(...serial.addParts);
    for (const [a, b] of serial.wires) wire(doc, a, b);
    expect(errors(doc)).toEqual([]);
  });

  it('joins a battery to a driver by B+ and B−', () => {
    const doc = blank(['b', 'bat?s=3&p=6', -100, 0], ['d', 'bts7960', 0, 0]);
    const r = connectOptions(doc, defs, 'b', 'd');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.options[0]!.wires).toEqual([
      ['d.B+', 'b.+'],
      ['b.-', 'd.B-'],
    ]);
  });

  it('says why when nothing fits', () => {
    const doc = blank(['s', 'hcsr04', 0, 0], ['l', 'led', 40, 0]);
    const r = connectOptions(doc, defs, 's', 'l');
    expect(r.ok).toBe(false);
  });
});

describe('parametric parts', () => {
  it('make a battery from its settings', () => {
    const b = defs('bat?s=4&p=2&cell=4.2&nom=3.6&mah=3000')!;
    expect(b.name).toBe('Battery 4S2P');
    expect(b.pins.find((p) => p.id === '+')!.volts).toBe(16.8);
    expect(b.covers).toContain('6 Ah');
  });

  it('cover every electrical part in SENNEP\'s parts list', () => {
    const ids = CATALOG.map((d) => d.id);
    for (const id of ['nano', 'l298n', 'bms3s', 'charger3s', 'l7805', 'ld1117', 'irfz44n', 'relay40', 'fuse', 'xt60', 'estop', 'e18', 'qmc5883l', 'neo6m', 'ina219', 'tsl2561', 'ds3231', 'mic', 'vs1838b', 'doorswitch', 'ld19', 'cp2102', 'aicam', 'nrf24', 'esp01', 'tca9548a', 'screen7', 'lcd2004', 'lcd1602', 'keypad', 'voltmeter', 'dfplayer', 'speaker', 'pam8403', 'led3w', 'irled', 'joystick', 'pot', 'keyswitch', 'toggle', 'adapter12', 'buckusb', 'shifter', 'hcsr04', 'vl53l0x', 'mpu6050', 'bts7960']) {
      expect(ids, id).toContain(id);
    }
    expect(defs('ws2812?n=150')).toBeDefined();
    expect(defs('mb102?top=5&bottom=3.3')).toBeDefined();
  });
});

describe('connect to, through the power chain', () => {
  it('puts a battery on a BMS: + to B+, − to B−', () => {
    const doc = blank(['b', 'bat?s=3&p=6', -100, 0], ['m', 'bms3s', 0, 0]);
    const r = connectOptions(doc, defs, 'b', 'm');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.options[0]!.wires.sort()).toEqual([['b.+', 'm.B+'], ['b.-', 'm.B-']].map((w) => w.slice().reverse()).sort().length ? r.options[0]!.wires.sort() : []);
    const flat = r.options[0]!.wires.map((w) => w.slice().sort().join('|')).sort();
    expect(flat).toEqual(['b.+|m.B+', 'b.-|m.B-'].sort());
  });

  it('powers a driver from a fuse that already carries the battery', () => {
    const doc = blank(['b', 'bat?s=3&p=6', -100, 0], ['f', 'fuse', -40, 0], ['d', 'bts7960', 40, 0]);
    wire(doc, 'b.+', 'f.A');
    const r = connectOptions(doc, defs, 'd', 'f');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.options[0]!.wires).toContainEqual(['d.B+', 'f.B']);
  });

  it('plugs a LiDAR into its USB adapter', () => {
    const doc = blank(['l', 'ld19', 0, 0], ['u', 'cp2102', 60, 0]);
    const r = connectOptions(doc, defs, 'l', 'u');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    const w = r.options[0]!.wires;
    expect(w).toContainEqual(['l.TX', 'u.RXD']);
    expect(w).toContainEqual(['l.P5V', 'u.5V']);
  });
});

describe('fixes', () => {
  it('move a PWM job to a pin that can do it', () => {
    const doc = blank(['m', 'mega', 0, 0], ['d', 'bts7960', 0, -60]);
    wire(doc, 'd.RPWM', 'm.D22');
    const issue = runChecks(doc, defs, computeNets(doc, defs)).find((i) => i.code === 'needs-cap')!;
    const fixes = fixesFor(doc, defs, issue);
    expect(fixes.length).toBeGreaterThan(0);
    applyFix(doc, fixes[0]!);
    expect(runChecks(doc, defs, computeNets(doc, defs)).some((i) => i.code === 'needs-cap')).toBe(false);
  });

  it('put a level shifter between a Pi and a Mega joined directly', () => {
    const doc = blank(['pi', 'pi5', -120, 0], ['m', 'mega', 0, 0]);
    wire(doc, 'pi.P8', 'm.D19');
    const issue = runChecks(doc, defs, computeNets(doc, defs)).find((i) => i.code === 'logic-joined')!;
    const fix = fixesFor(doc, defs, issue).find((f) => f.label === 'Put a level shifter between')!;
    expect(fix.addParts?.map((p) => p.def)).toEqual(['shifter']);
    applyFix(doc, fix);
    expect(errors(doc)).toEqual([]);
  });

  it('offer several supplies for a part with no power, each one working', () => {
    const doc = blank(['m', 'mega', 0, 0], ['s', 'hcsr04', 0, -40]);
    wire(doc, 's.TRIG', 'm.D30');
    wire(doc, 's.ECHO', 'm.D31');
    wire(doc, 's.GND', 'm.GND1');
    wire(doc, 's.VCC', 'm.3V3');
    const issue = runChecks(doc, defs, computeNets(doc, defs)).find((i) => i.code === 'under-voltage')!;
    const fixes = fixesFor(doc, defs, issue);
    expect(fixes.length).toBeGreaterThan(1);
    expect(fixes.every((f) => f.label.startsWith('Power it from m 5V'))).toBe(true);
  });
});

describe('power mode', () => {
  it('switches the SENNEP example on: Pi, Mega and LiDAR run, current flows away from the battery', () => {
    const doc = example();
    const s = simulate(doc, defs);
    for (const id of ['pi', 'mega', 'lidar', 'sl', 'drv', 'buck', 'usb']) expect(s.parts.get(id), id).toBe('on');
    expect(s.shorts.size).toBe(0);
    const w = doc.wires.find((x) => x.a === 'bat.+' && x.b === 'bms.B+')!;
    expect(s.flow.get(w.id)).toBe(1);
    const g = doc.wires.find((x) => x.a === 'bat.-' && x.b === 'bms.B-')!;
    expect(s.flow.get(g.id)).toBe(-1);
  });

  it('pressing the e-stop cuts the driver, not the Pi', () => {
    const doc = example();
    doc.parts.find((p) => p.id === 'estop')!.open = true;
    const s = simulate(doc, defs);
    expect(s.parts.get('drv')).toBe('on'); // its logic side runs off the Mega's 5 V
    const bplus = computeNets(doc, defs).of.get('drv.B+')!;
    expect(s.live.has(bplus.id)).toBe(false);
    expect(s.parts.get('pi')).toBe('on');
  });


  it('a short after the BMS trips it', () => {
    const doc = example();
    wire(doc, 'fuse.B', 'mega.GND1');
    const s = simulate(doc, defs);
    expect(s.tripped.length).toBeGreaterThan(0);
    expect(s.messages[0]!.sev).toBe('error');
  });
});

describe('the breadboard', () => {
  it('pulls a module dropped near the holes into them', () => {
    const doc = blank(['bb', 'breadboard', 0, 0], ['l', 'led', 0, 0]);
    const led = doc.parts[1]!;
    led.rot = 2;
    led.x = 4 + 2 * 9 - 2 + 2;
    led.y = 20 + 2;
    expect(snapToBreadboard(doc, defs, led)).toBe(true);
    expect(computeNets(doc, defs).plugs.length).toBe(2);
  });
});

describe('mirroring', () => {
  it("swaps a part's left and right pins", () => {
    const doc = blank(['d', 'bts7960', 0, 0]);
    const plain = geom(doc.parts[0]!, defs).pins['RPWM']!;
    doc.parts[0]!.flip = true;
    const flipped = geom(doc.parts[0]!, defs).pins['RPWM']!;
    expect(plain.x).toBe(0);
    expect(flipped.x).toBe(40);
    expect(flipped.d).toBe(1);
  });
});
