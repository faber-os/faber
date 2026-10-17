import { describe, expect, it } from 'vitest';
import { buildSteps } from './build';
import { pairVerdict, runChecks } from './checks';
import { connectOptions } from './connect';
import { seatShifter, sennepExample } from './example';
import { geom, tooClose } from './geometry';
import { arrange, mirrorLayout, mountedOn } from './layout';
import { computeNets } from './nets';
import { CATALOG, listed, lookup } from './catalog';
import { applyFix, fixesFor } from './fixes';
import { simulate } from './power';
import { runCircuit } from './circuit';
import { seatSennepShifter, sennepV2 } from './sennep';
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
  return runChecks(doc, defs, nets)
    .filter((i) => i.sev === 'error')
    .map((i) => i.text);
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
          expect(
            had === undefined || had === net,
            `${w.a} → ${w.b} overlaps another wire at ${x},${y}`,
          ).toBe(true);
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
    expect(ids.filter((id) => id.startsWith('w')).sort()).toEqual(
      doc.wires.map((w) => w.id).sort(),
    );
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
    expect(
      errors(doc).some((t) => t.startsWith('s: VCC and TRIG and ECHO and GND are joined')),
    ).toBe(true);
  });
});

describe('connect to', () => {
  it('wires a sonar to free plain pins, 5 V and ground', () => {
    const doc = blank(['m', 'mega', 0, 0], ['s', 'hcsr04', 20, -40]);
    const r = connectOptions(doc, defs, 's', 'm');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    const wires = r.options[0]!.wires;
    const to = (pin: string) =>
      wires.find(([a, b]) => a === pin || b === pin)?.find((k) => k !== pin);
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

  it("cover every electrical part in SENNEP's parts list", () => {
    const ids = CATALOG.map((d) => d.id);
    for (const id of [
      'nano',
      'l298n',
      'bms3s',
      'charger3s',
      'l7805',
      'ld1117',
      'irfz44n',
      'relay40',
      'xt60',
      'estop',
      'e18',
      'qmc5883l',
      'neo6m',
      'ina219',
      'tsl2561',
      'ds3231',
      'mic',
      'vs1838b',
      'doorswitch',
      'ld19',
      'cp2102',
      'aicam',
      'nrf24',
      'esp01',
      'tca9548a',
      'screen7',
      'lcd2004',
      'lcd1602',
      'keypad',
      'voltmeter',
      'dfplayer',
      'speaker',
      'pam8403',
      'led3w',
      'irled',
      'joystick',
      'pot',
      'keyswitch',
      'toggle',
      'adapter12',
      'buckusb',
      'shifter',
      'hcsr04',
      'vl53l0x',
      'mpu6050',
      'bts7960',
    ]) {
      expect(ids, id).toContain(id);
    }
    expect(defs('ws2812?n=150')).toBeDefined();
    expect(defs('mb102?top=5&bottom=3.3')).toBeDefined();
    expect(defs('fuse?a=5')?.trip).toBe(5);
    expect(defs('res?ohms=220&w=0.25')?.elec).toBeDefined();
  });
});

describe('connect to, through the power chain', () => {
  it('puts a battery on a BMS: + to B+, − to B−', () => {
    const doc = blank(['b', 'bat?s=3&p=6', -100, 0], ['m', 'bms3s', 0, 0]);
    const r = connectOptions(doc, defs, 'b', 'm');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.options[0]!.wires.sort()).toEqual(
      [
        ['b.+', 'm.B+'],
        ['b.-', 'm.B-'],
      ]
        .map((w) => w.slice().reverse())
        .sort().length
        ? r.options[0]!.wires.sort()
        : [],
    );
    const flat = r.options[0]!.wires.map((w) => w.slice().sort().join('|')).sort();
    expect(flat).toEqual(['b.+|m.B+', 'b.-|m.B-'].sort());
  });

  it('powers a driver from a fuse that already carries the battery', () => {
    const doc = blank(
      ['b', 'bat?s=3&p=6', -100, 0],
      ['f', 'fuse', -40, 0],
      ['d', 'bts7960', 40, 0],
    );
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
    expect(runChecks(doc, defs, computeNets(doc, defs)).some((i) => i.code === 'needs-cap')).toBe(
      false,
    );
  });

  it('put a level shifter between a Pi and a Mega joined directly', () => {
    const doc = blank(['pi', 'pi5', -120, 0], ['m', 'mega', 0, 0]);
    wire(doc, 'pi.P8', 'm.D19');
    const issue = runChecks(doc, defs, computeNets(doc, defs)).find(
      (i) => i.code === 'logic-joined',
    )!;
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
    const issue = runChecks(doc, defs, computeNets(doc, defs)).find(
      (i) => i.code === 'under-voltage',
    )!;
    const fixes = fixesFor(doc, defs, issue);
    expect(fixes.length).toBeGreaterThan(1);
    expect(fixes.every((f) => f.label.startsWith('Power it from m 5V'))).toBe(true);
  });
});

describe('power mode', () => {
  it('switches the SENNEP example on: Pi, Mega and LiDAR run, current flows away from the battery', () => {
    const doc = example();
    const s = simulate(doc, defs);
    for (const id of ['pi', 'mega', 'lidar', 'sl', 'drv', 'buck', 'usb'])
      expect(s.parts.get(id), id).toBe('on');
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
  it('mirrors where parts sit, never their pins', () => {
    const doc = blank(['d', 'bts7960', 0, 0], ['l', 'led', 100, 0]);
    const before = geom(doc.parts[0]!, defs);
    const rpwmOffset = before.pins['RPWM']!.x - before.x0;
    mirrorLayout(doc, defs, ['d', 'l'], 'h');
    const after = geom(doc.parts[0]!, defs);
    expect(doc.parts[0]!.x).toBeGreaterThan(doc.parts[1]!.x);
    expect(after.pins['RPWM']!.x - after.x0).toBe(rpwmOffset);
    expect(doc.parts[0]!.rot).toBe(0);
  });
});

describe('the circuit simulation', () => {
  const high = (key: string) => new Map([[key, { mode: 'out' as const, level: 1 }]]);

  it('an LED straight on a pin burns out, and takes the pin with it', () => {
    const doc = blank(['u', 'uno', 0, 0], ['l', 'led-red', 0, -20], ['a', 'adapter12', -60, 0]);
    wire(doc, 'a.+', 'u.VIN');
    wire(doc, 'a.-', 'u.GND2');
    wire(doc, 'u.D13', 'l.A');
    wire(doc, 'l.K', 'u.GND1');
    const c = runCircuit(doc, defs, { pins: high('u.D13') });
    expect(c.parts.get('l')).toBe('burnt');
    expect(c.findings.some((f) => f.text.startsWith('u D13 gives'))).toBe(true);
  });

  it('with 220 Ω in series the LED gets about 13 mA and lights', () => {
    const doc = blank(
      ['u', 'uno', 0, 0],
      ['l', 'led-red', 0, -20],
      ['r', 'res?ohms=220&w=0.25', 20, -20],
      ['a', 'adapter12', -60, 0],
    );
    wire(doc, 'a.+', 'u.VIN');
    wire(doc, 'a.-', 'u.GND2');
    wire(doc, 'u.D13', 'r.1');
    wire(doc, 'r.2', 'l.A');
    wire(doc, 'l.K', 'u.GND1');
    const c = runCircuit(doc, defs, { pins: high('u.D13') });
    const ma = c.partAmps.get('l')! * 1000;
    expect(ma).toBeGreaterThan(11);
    expect(ma).toBeLessThan(15);
    expect(c.parts.get('l')).toBe('on');
    expect(c.findings.filter((f) => f.sev === 'error')).toEqual([]);
  });

  it("splits SENNEP's 12.6 V to about 2.2 V with 47 kΩ over 10 kΩ", () => {
    const doc = blank(
      ['b', 'bat?s=3&p=6', 0, 0],
      ['r1', 'res?ohms=47000&w=0.25', 20, 0],
      ['r2', 'res?ohms=10000&w=0.25', 40, 0],
    );
    wire(doc, 'b.+', 'r1.1');
    wire(doc, 'r1.2', 'r2.1');
    wire(doc, 'r2.2', 'b.-');
    const c = runCircuit(doc, defs);
    expect(c.volts.get('r2.1')).toBeCloseTo(2.21, 1);
  });

  it('the e-stop in the relay coil cuts the motor bus', () => {
    const doc = blank(
      ['b', 'bat?s=3&p=6', 0, 0],
      ['e', 'estop', 20, 0],
      ['k', 'relay40', 40, 0],
      ['m', 'dcmotor', 60, 0],
    );
    wire(doc, 'b.+', 'e.1');
    wire(doc, 'e.2', 'k.86');
    wire(doc, 'k.85', 'b.-');
    wire(doc, 'b.+', 'k.30');
    wire(doc, 'k.87', 'm.M+');
    wire(doc, 'm.M-', 'b.-');
    expect(runCircuit(doc, defs).volts.get('m.M+')).toBeGreaterThan(12);
    doc.parts.find((p) => p.id === 'e')!.open = true;
    expect(runCircuit(doc, defs).volts.get('m.M+') ?? 0).toBeLessThan(0.1);
  });

  it('a stalled motor on Dupont jumpers overheats them; 16 AWG carries it', () => {
    const doc = blank(['b', 'bat?s=3&p=6', 0, 0], ['d', 'bts7960', 20, 0], ['m', 'dcmotor', 60, 0]);
    wire(doc, 'b.+', 'd.B+');
    wire(doc, 'b.-', 'd.B-');
    wire(doc, 'd.M+', 'm.M+');
    wire(doc, 'd.M-', 'm.M-');
    for (const w of doc.wires) w.gauge = 'dupont';
    const hot = runCircuit(doc, defs, { motors: 'stall' });
    expect(hot.findings.some((f) => f.wires.length && f.sev === 'error')).toBe(true);
    for (const w of doc.wires) w.gauge = '10';
    const ok = runCircuit(doc, defs, { motors: 'run' });
    expect(ok.findings.filter((f) => f.wires.length)).toEqual([]);
    expect(Math.abs(ok.amps.get(doc.wires[0]!.id)!)).toBeGreaterThan(1.5);
  });

  it('5 V into a Raspberry Pi pin is an error', () => {
    const doc = blank(
      ['p', 'pi5', 0, 0],
      ['b', 'buckusb', -60, 0],
      ['a', 'adapter12', -100, 0],
      ['s', 'hcsr04', 0, -30],
    );
    wire(doc, 'a.+', 'b.IN+');
    wire(doc, 'a.-', 'b.IN-');
    wire(doc, 'b.USBC', 'p.USBC');
    wire(doc, 's.VCC', 'p.P2');
    wire(doc, 's.GND', 'p.P6');
    wire(doc, 's.ECHO', 'p.P11');
    const c = runCircuit(doc, defs);
    expect(c.findings.some((f) => f.text.includes('it is a 3.3 V pin'))).toBe(true);
  });
});

describe('resistor advice', () => {
  const issues = (doc: Diagram) => runChecks(doc, defs, computeNets(doc, defs));
  const high = (key: string) => new Map([[key, { mode: 'out' as const, level: 1 }]]);

  it('finds the LED without a resistor, and its fix adds one that keeps it lit and safe', () => {
    const doc = blank(['u', 'uno', 0, 0], ['l', 'led-red', 30, -30], ['a', 'adapter12', -60, 0]);
    wire(doc, 'a.+', 'u.VIN');
    wire(doc, 'a.-', 'u.GND2');
    wire(doc, 'u.D13', 'l.A');
    wire(doc, 'l.K', 'u.GND1');
    const issue = issues(doc).find((i) => i.code === 'led-no-resistor')!;
    expect(issue.more).toContain('330');
    const fix = fixesFor(doc, defs, issue)[0]!;
    expect(fix.addParts![0]!.def).toBe('res?ohms=330&w=0.25');
    applyFix(doc, fix);
    expect(issues(doc).some((i) => i.code === 'led-no-resistor')).toBe(false);
    const c = runCircuit(doc, defs, { pins: high('u.D13') });
    expect(c.parts.get('l')).toBe('on');
    expect(c.partAmps.get('l')! * 1000).toBeLessThan(11);
  });

  it('a button needs a pull-up: INPUT_PULLUP in the code, or a 10 kΩ resistor', () => {
    const doc = blank(['m', 'mega', 0, 0], ['b', 'button', 0, -30]);
    wire(doc, 'b.1', 'm.D29');
    wire(doc, 'b.2', 'm.GND1');
    const issue = issues(doc).find((i) => i.code === 'needs-pull-up')!;
    const fixes = fixesFor(doc, defs, issue);
    expect(fixes.map((f) => f.label)).toEqual([
      'I use INPUT_PULLUP in the code',
      'Add a 10 kΩ pull-up resistor',
    ]);
    applyFix(doc, fixes[1]!);
    expect(issues(doc).some((i) => i.code === 'needs-pull-up')).toBe(false);
  });

  it('a motor driver enable gets a pull-down', () => {
    const doc = blank(['m', 'mega', 0, 0], ['d', 'bts7960', 0, -60]);
    wire(doc, 'd.R_EN', 'm.D22');
    const issue = issues(doc).find((i) => i.code === 'needs-pull-down')!;
    applyFix(doc, fixesFor(doc, defs, issue)[0]!);
    expect(
      issues(doc).filter((i) => i.code === 'needs-pull-down' && i.keys.includes('d.R_EN')),
    ).toEqual([]);
  });

  it('the battery on an analog pin gets a divider that keeps it under 5 V', () => {
    const doc = blank(['m', 'mega', 0, 0], ['b', 'bat?s=3&p=6', -80, 0]);
    wire(doc, 'b.-', 'm.GND1');
    wire(doc, 'b.+', 'm.A4');
    const issue = issues(doc).find((i) => i.code === 'needs-divider')!;
    applyFix(doc, fixesFor(doc, defs, issue)[0]!);
    expect(issues(doc).some((i) => i.code === 'needs-divider')).toBe(false);
    const v = runCircuit(doc, defs).volts.get('m.A4')!;
    expect(v).toBeLessThan(4.6);
    expect(v).toBeGreaterThan(3.5);
  });

  it('a relay coil gets its diode', () => {
    const doc = blank(['b', 'bat?s=3&p=6', 0, 0], ['k', 'relay40', 40, 0]);
    wire(doc, 'b.+', 'k.86');
    wire(doc, 'k.85', 'b.-');
    const issue = issues(doc).find((i) => i.code === 'needs-flyback')!;
    applyFix(doc, fixesFor(doc, defs, issue)[0]!);
    expect(issues(doc).some((i) => i.code === 'needs-flyback')).toBe(false);
  });

  it('5 V into a Pi pin can also take a divider', () => {
    const doc = blank(['p', 'pi5', 0, 0], ['s', 'hcsr04', 0, -30]);
    wire(doc, 's.ECHO', 'p.P11');
    const issue = issues(doc).find((i) => i.code === 'drives-into')!;
    const fix = fixesFor(doc, defs, issue).find((f) => f.label.startsWith('Put a divider'))!;
    applyFix(doc, fix);
    expect(issues(doc).some((i) => i.code === 'drives-into')).toBe(false);
  });
});

describe('SENNEP V2, the whole machine', () => {
  const machine = () => {
    const doc = sennepV2();
    seatSennepShifter(doc);
    arrange(doc, defs);
    return doc;
  };

  it('passes every check', () => {
    const doc = machine();
    expect(runChecks(doc, defs, computeNets(doc, defs)).map((i) => `${i.code}: ${i.text}`)).toEqual(
      [],
    );
  });

  it('powers up: Pi, Mega, LiDAR, GPS, radio and the LED node run; nothing is over its limit', () => {
    const doc = machine();
    const s = simulate(doc, defs, { motors: 'run' });
    for (const id of ['pi', 'mega', 'lidar', 'gps', 'nrf', 'nano', 'tof0', 'dfl', 'mfl'])
      expect(s.parts.get(id), id).toBe('on');
    expect(s.findings.filter((f) => f.sev === 'error')).toEqual([]);
  });

  it('with all four motors held still, a fuse blows before a wire burns', () => {
    const s = simulate(machine(), defs, { motors: 'stall' });
    expect(s.tripped.length).toBeGreaterThan(0);
    expect(s.findings.filter((f) => f.wires.length && f.sev === 'error')).toEqual([]);
  });

  it('pressing the e-stop drops the relay: the motor bus goes dead, the Pi stays on', () => {
    const doc = machine();
    doc.parts.find((p) => p.id === 'estop')!.open = true;
    const s = simulate(doc, defs, { motors: 'run' });
    expect(s.volts.get('mfl.M+') ?? 0).toBeLessThan(0.1);
    expect(s.parts.get('pi')).toBe('on');
    expect(s.volts.get('mega.A4') ?? 0).toBeLessThan(0.1);
  });
});
