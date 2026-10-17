/**
 * The parts Faber already runs, as a drawing to wire up (docs/circuits.md §4). Each Faber part
 * becomes the lab's part for it: a FaberLink board becomes its Arduino, the parts on it the
 * modules its pins say (`PINS`), wired to those pins; the LD19 gets its USB adapter; parts on a
 * USB port hang off the machine itself (a Raspberry Pi when Faber runs on one, else "this
 * computer"). What Faber can't tell (a module's power, a driver's motor) is left for the person
 * to wire, and the checks say so.
 */
import type { Diagram, Part, Wire } from './types';

/** What the lab needs to know about a Faber part (from the core's part list). */
export interface FaberPart {
  name: string;
  connector: string;
  port?: string;
  /** On an I²C bus of the machine. */
  address?: number;
  /** The FaberLink board it is on. */
  bridge?: string;
  /** The board pins it uses, from `PINS`: role and printed label. */
  pins?: { role: string; label: string }[];
  /** Its signals' capabilities, e.g. `distance@1`. */
  capabilities: string[];
  simulated: boolean;
}

interface Made {
  parts: Part[];
  wires: [string, string][];
  skipped: string[];
}

/** Which Arduino a FaberLink board is: from its name (the core doesn't report the board type). */
function boardDef(p: FaberPart): string {
  const n = p.name.toLowerCase();
  if (n.includes('mega')) return 'mega';
  if (n.includes('nano')) return 'nano';
  return 'uno';
}

const has = (p: FaberPart, cap: string) => p.capabilities.some((c) => c.startsWith(cap));
const role = (p: FaberPart, r: string) => p.pins?.find((x) => x.role === r)?.label;

export function diagramFromParts(
  list: FaberPart[],
  host: 'pi' | 'computer',
  name: string,
): Diagram {
  const made: Made = { parts: [], wires: [], skipped: [] };
  const ids = new Set<string>();
  const add = (base: string, def: string, label: string): string => {
    let id = base.replace(/[^a-zA-Z0-9]/g, '-');
    while (ids.has(id)) id += '-';
    ids.add(id);
    made.parts.push({ id, def, name: label, x: 0, y: 0, rot: 0 });
    return id;
  };
  const wire = (a: string, b: string) => made.wires.push([a, b]);

  const hostId = add('host', host === 'pi' ? 'pi5' : 'computer', host === 'pi' ? 'pi' : 'computer');
  const usbPorts =
    host === 'pi' ? ['USB1', 'USB2', 'USB3', 'USB4'] : ['USB1', 'USB2', 'USB3', 'USB4'];
  let nextUsb = 0;
  const usb = (devicePin: string) => {
    const port = usbPorts[nextUsb++];
    if (port) wire(devicePin, `${hostId}.${port}`);
  };

  // Boards first, so the parts on them can wire to their pins.
  const boards = new Map<string, string>();
  const boardKind = new Map<string, string>();
  for (const p of list.filter((x) => x.connector === 'faber-bridge')) {
    const id = add(p.name, boardDef(p), p.name);
    boards.set(p.name, id);
    boardKind.set(id, boardDef(p));
    if (p.port && !p.simulated) usb(`${id}.USB`);
  }

  for (const p of list) {
    if (p.connector === 'faber-bridge') continue;
    const board = p.bridge ? boards.get(p.bridge) : undefined;
    const nano = board ? boardKind.get(board) === 'nano' : false;
    const V5 = nano ? '5V' : '5V1';
    const GND = nano ? 'GND' : 'GND1';
    const SDA = nano ? 'A4' : 'SDA';
    const SCL = nano ? 'A5' : 'SCL';
    const pin = (label?: string) => (board && label ? `${board}.${label}` : undefined);
    const to = (from: string, label?: string) => {
      const b = pin(label);
      if (b) wire(from, b);
    };

    if (p.connector.includes('ld19') || has(p, 'laser-scan')) {
      const id = add(p.name, 'ld19', p.name);
      if (p.port && !p.simulated) {
        const ad = add(`${p.name}-usb`, 'cp2102', `${p.name}-usb`);
        wire(`${id}.TX`, `${ad}.RXD`);
        wire(`${id}.P5V`, `${ad}.5V`);
        wire(`${id}.GND`, `${ad}.GND`);
        usb(`${ad}.USB`);
      }
    } else if (has(p, 'distance') && role(p, 'trig')) {
      const id = add(p.name, 'hcsr04', p.name);
      to(`${id}.TRIG`, role(p, 'trig'));
      to(`${id}.ECHO`, role(p, 'echo'));
      if (board) (wire(`${id}.VCC`, `${board}.${V5}`), wire(`${id}.GND`, `${board}.${GND}`));
    } else if (
      has(p, 'distance') &&
      (role(p, 'sda') || role(p, 'xshut') || p.address !== undefined)
    ) {
      const id = add(p.name, 'vl53l0x', p.name);
      to(`${id}.XSHUT`, role(p, 'xshut'));
      if (board) {
        wire(`${id}.SDA`, `${board}.${SDA}`);
        wire(`${id}.SCL`, `${board}.${SCL}`);
        wire(`${id}.VIN`, `${board}.${V5}`);
        wire(`${id}.GND`, `${board}.${GND}`);
      } else if (host === 'pi') {
        wire(`${id}.SDA`, `${hostId}.P3`);
        wire(`${id}.SCL`, `${hostId}.P5`);
        wire(`${id}.VIN`, `${hostId}.P1`);
        wire(`${id}.GND`, `${hostId}.P9`);
      }
    } else if (has(p, 'distance')) {
      // A distance part that doesn't say how it is wired: the sonar is the commonest.
      add(p.name, 'hcsr04', p.name);
    } else if (has(p, 'imu')) {
      const id = add(p.name, 'mpu6050', p.name);
      if (board)
        (wire(`${id}.SDA`, `${board}.${SDA}`),
          wire(`${id}.SCL`, `${board}.${SCL}`),
          wire(`${id}.VCC`, `${board}.${V5}`),
          wire(`${id}.GND`, `${board}.${GND}`));
      else if (host === 'pi')
        (wire(`${id}.SDA`, `${hostId}.P3`),
          wire(`${id}.SCL`, `${hostId}.P5`),
          wire(`${id}.VCC`, `${hostId}.P1`),
          wire(`${id}.GND`, `${hostId}.P9`));
    } else if (has(p, 'servo')) {
      const id = add(p.name, 'sg90', p.name);
      to(`${id}.SIG`, role(p, 'signal') ?? role(p, 'pwm'));
    } else if (has(p, 'obstacle')) {
      const id = add(p.name, 'e18', p.name);
      to(`${id}.OUT`, role(p, 'signal') ?? role(p, 'out'));
    } else if (has(p, 'motor')) {
      // A driver with direction pins is an L298N channel; PWM only, a BTS7960.
      if (role(p, 'in1') || role(p, 'in2')) {
        const id = add(p.name, 'l298n', `${p.name}-driver`);
        to(`${id}.IN1`, role(p, 'in1'));
        to(`${id}.IN2`, role(p, 'in2'));
        to(`${id}.ENA`, role(p, 'en') ?? role(p, 'pwm'));
        const m = add(`${p.name}-motor`, 'dcmotor', p.name);
        wire(`${id}.OUT1`, `${m}.M+`);
        wire(`${id}.OUT2`, `${m}.M-`);
      } else {
        const id = add(p.name, 'bts7960', `${p.name}-driver`);
        to(`${id}.RPWM`, role(p, 'pwm'));
        to(`${id}.LPWM`, role(p, 'dir'));
        to(`${id}.R_EN`, role(p, 'en'));
        const m = add(`${p.name}-motor`, 'dcmotor', p.name);
        wire(`${id}.M+`, `${m}.M+`);
        wire(`${id}.M-`, `${m}.M-`);
      }
    } else if (has(p, 'battery')) {
      add(p.name, 'bat?s=3&p=6&cell=4.2&nom=3.6&mah=3400', p.name);
    } else {
      made.skipped.push(p.name);
    }
  }

  made.parts.forEach((p, i) => {
    p.x = (i % 6) * 120;
    p.y = Math.floor(i / 6) * 100;
  });
  const wires: Wire[] = made.wires.map(([a, b], i) => ({ id: `w${i + 1}`, a, b }));
  return {
    name,
    parts: made.parts,
    wires,
    done: {},
    seq: wires.length + 1,
    skipped: made.skipped.length ? made.skipped : undefined,
  };
}
