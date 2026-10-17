/**
 * Where a resistor (or a diode) belongs, and how big, said for someone who has never placed one.
 * Each advice is a check issue with a fix that adds the real part with its wires, so the drawing
 * stays the circuit you build. The reasons are the three jobs a resistor mostly does:
 *
 *   limit current   an LED takes all the current it can get; a resistor in series sets it
 *   pull up/down    an input wired to nothing (an open button) floats; a resistor holds it
 *   divide          two resistors scale a voltage down for a pin that can't take it
 *
 * plus a diode across a relay coil, which swallows the spike when the coil lets go.
 */
import type { Issue } from './issue';
import { geom } from './geometry';
import type { NetIndex } from './nets';
import { e12Up, ohmShort } from './passives';
import type { DefLookup, Diagram, Part, PinDef } from './types';
import { splitKey } from './types';

export type { AdviceCode } from './issue';

interface Ref {
  key: string;
  part: Part;
  pin: PinDef;
}

/** The LED current advice aims for: bright enough, kind to pins and to the LED. */
const LED_MA = 10;

export function advise(doc: Diagram, defs: DefLookup, nets: NetIndex): Issue[] {
  const out: Issue[] = [];
  const ref = (key: string): Ref | undefined => {
    const [pid, pn] = splitKey(key);
    const part = doc.parts.find((p) => p.id === pid);
    const pin = part ? defs(part.def)?.pins.find((p) => p.id === pn) : undefined;
    return part && pin ? { key, part, pin } : undefined;
  };
  const netRefs = (key: string): Ref[] =>
    (nets.of.get(key)?.keys ?? [key])
      .map(ref)
      .filter((r): r is Ref => !!r && r.pin.kind !== 'hole');
  const isResistorLeg = (r: Ref) => defs(r.part.def)?.elec?.t === 'resistor';
  const wiresOf = (keys: string[]) =>
    doc.wires.filter((w) => keys.some((k) => nets.of.get(k)?.keys.includes(w.a))).map((w) => w.id);
  const pullups = new Set(doc.pullups ?? []);

  for (const part of doc.parts) {
    const d = defs(part.def);
    const e = d?.elec;

    // LEDs: a resistor in series, and the right way round.
    if (e?.t === 'diode' && e.led) {
      const a = netRefs(`${part.id}.${e.a}`).filter((r) => r.part.id !== part.id);
      const k = netRefs(`${part.id}.${e.k}`).filter((r) => r.part.id !== part.id);
      const drive = (rs: Ref[]) => Math.max(0, ...rs.map(drives));
      const grounded = (rs: Ref[]) => rs.some((r) => r.pin.kind === 'gnd');
      if (grounded(a) && drive(k) > 0) {
        out.push({
          sev: 'warn',
          code: 'led-backwards',
          text: `${part.name} is the wrong way round: it won't light`,
          more: 'Current only goes through an LED from its long leg (A, +) to its short leg (K, −, the flat side). Turn it round.',
          parts: [part.id],
          wires: wiresOf([`${part.id}.${e.a}`, `${part.id}.${e.k}`]),
          keys: [`${part.id}.${e.a}`, `${part.id}.${e.k}`],
        });
        continue;
      }
      const v = drive(a);
      if (v > e.vf && !a.some(isResistorLeg) && !k.some(isResistorLeg)) {
        const target = e.ma ?? LED_MA;
        const ma = e.vf < 1.8 ? target : LED_MA;
        const ohms = Math.max(10, e12Up((v - e.vf) / (ma / 1000)));
        out.push({
          sev: 'error',
          code: 'led-no-resistor',
          text: `${part.name} has no resistor: it burns out (and can take the pin with it)`,
          more: `An LED doesn't hold its own current back. A ${ohmShort(ohms)} resistor in series lets about ${Math.round(((v - e.vf) / ohms) * 1000)} mA through: (${v} V − ${e.vf} V) ÷ ${ma} mA = ${Math.round((v - e.vf) / (ma / 1000))} Ω, and ${ohmShort(ohms)} is the next standard value up.`,
          parts: [part.id],
          wires: wiresOf([`${part.id}.${e.a}`]),
          keys: [`${part.id}.${e.a}`, `${part.id}.${e.k}`],
        });
      }
    }

    // A relay coil without a diode across it.
    if (d && d.id === 'relay40') {
      const coilP = netRefs(`${part.id}.86`);
      const coilN = netRefs(`${part.id}.85`);
      const diode = coilP.some((r) => {
        const x = defs(r.part.def)?.elec;
        return (
          x?.t === 'diode' &&
          !x.led &&
          r.pin.id === x.k &&
          coilN.some((n) => n.part.id === r.part.id && n.pin.id === x.a)
        );
      });
      if (coilP.length > 1 && !diode) {
        out.push({
          sev: 'warn',
          code: 'needs-flyback',
          text: `${part.name}'s coil has no diode across it`,
          more: 'A coil is a magnet made of wire. Switch it off and it kicks back a spike of a hundred volts or more, which can reset or kill the Mega. A 1N4007 across the coil (stripe to 86, the + side) swallows it.',
          parts: [part.id],
          wires: [],
          keys: [`${part.id}.86`, `${part.id}.85`],
        });
      }
    }

    for (const p of d?.pins ?? []) {
      const key = `${part.id}.${p.id}`;
      // A switch or an open-collector output on a board pin: it needs a pull-up.
      if (p.kind === 'sig' && p.out && !p.drives && p.needs?.includes('io')) {
        const rs = netRefs(key);
        const board = rs.find((r) => r.pin.kind === 'io');
        if (board && !pullups.has(board.key) && !rs.some((r) => isResistorLeg(r))) {
          out.push({
            sev: 'warn',
            code: 'needs-pull-up',
            text: `${board.part.name} ${board.pin.label} floats while ${part.name} is open`,
            more: `${part.name} only ever pulls the line to GND. When it lets go, nothing holds the pin, and it reads random noise. A pull-up holds it at 5 V: either INPUT_PULLUP in the code (the chip's own 35 kΩ), or a 10 kΩ resistor from the pin to 5V.`,
            parts: [part.id, board.part.id],
            wires: wiresOf([key]),
            keys: [board.key, key],
          });
        }
      }
      // An enable or gate that must stay off while the board boots: a pull-down.
      if (p.pull === 'down') {
        const rs = netRefs(key);
        const pulled = rs.some(
          (r) => isResistorLeg(r) && netRefs(otherLeg(r, defs)).some((x) => x.pin.kind === 'gnd'),
        );
        const board = rs.find((r) => r.pin.kind === 'io');
        if (board && !pulled) {
          out.push({
            sev: 'warn',
            code: 'needs-pull-down',
            text: `${part.name} ${p.label} has no pull-down`,
            more: `While a board starts up or resets, its pins float for a moment, and ${part.name} could switch on by itself. A 10 kΩ resistor from ${p.label} to GND keeps it off until the code says otherwise.`,
            parts: [part.id],
            wires: wiresOf([key]),
            keys: [board.key, key],
          });
        }
      }
    }
  }

  // A board pin on a voltage above its logic: two resistors bring it down.
  for (const net of nets.nets) {
    const rs = net.keys.map(ref).filter((r): r is Ref => !!r);
    const pins = rs.filter(
      (r) => r.pin.kind === 'io' && (r.pin.caps?.includes('analog') || r.pin.caps?.includes('io')),
    );
    const sup = rs.filter((r) => r.pin.kind === 'supply');
    for (const io of pins) {
      const high = sup.find((s) => (s.pin.volts ?? 0) > (io.pin.logic ?? 5) + 0.3);
      if (!high) continue;
      out.push({
        sev: 'error',
        code: 'needs-divider',
        text: `${io.part.name} ${io.pin.label} is on ${high.part.name}'s ${high.pin.volts} V; it takes ${io.pin.logic} V at most`,
        more: `To measure a voltage higher than the pin takes, split it with two resistors (a divider): the pin sees the bottom one's share. ${dividerText(high.pin.volts ?? 0, io.pin.logic ?? 5)}`,
        parts: [io.part.id, high.part.id],
        wires: doc.wires.filter((w) => net.keys.includes(w.a)).map((w) => w.id),
        keys: [io.key, high.key],
      });
    }
  }
  return out;
}

/** What a pin can put on its line: a board pin its logic level, a supply its volts, a sensor its output. */
function drives(r: Ref): number {
  if (r.pin.kind === 'io') return r.pin.logic ?? 5;
  if (r.pin.kind === 'supply') return r.pin.volts ?? 0;
  if (r.pin.kind === 'sig' && r.pin.drives) return r.pin.drives;
  return 0;
}

function otherLeg(r: Ref, defs: DefLookup): string {
  const e = defs(r.part.def)?.elec;
  if (e?.t !== 'resistor') return r.key;
  return `${r.part.id}.${r.pin.id === e.a ? e.b : e.a}`;
}

/** Two E12 resistors that bring `vmax` below 90 % of `logic`: the top one and a 10 kΩ bottom one. */
export function divider(vmax: number, logic: number): { top: number; bottom: number; out: number } {
  const bottom = 10000;
  const top = e12Up(bottom * (vmax / (0.9 * logic) - 1));
  return { top, bottom, out: Math.round(((vmax * bottom) / (top + bottom)) * 100) / 100 };
}

function dividerText(vmax: number, logic: number): string {
  const d = divider(vmax, logic);
  return `${ohmShort(d.top)} on top and ${ohmShort(d.bottom)} below: ${vmax} V × ${ohmShort(d.bottom)} ÷ (${ohmShort(d.top)} + ${ohmShort(d.bottom)}) = ${d.out} V. In the code, multiply what you read by ${Math.round(((d.top + d.bottom) / d.bottom) * 100) / 100}.`;
}

// ---- fixes ----------------------------------------------------------------------------------

export interface AdviceFix {
  label: string;
  hint?: string;
  remove?: string[];
  add?: [string, string][];
  addParts?: Part[];
  /** Board pins the code sets to INPUT_PULLUP. */
  pullups?: string[];
}

export function adviceFixes(doc: Diagram, defs: DefLookup, issue: Issue): AdviceFix[] {
  const names = new Set(doc.parts.map((p) => p.name));
  let n = 1;
  const rname = (prefix: string) => {
    while (names.has(`${prefix}${n}`)) n++;
    const out = `${prefix}${n}`;
    names.add(out);
    return out;
  };
  const near = (key: string, dx: number, dy: number): { x: number; y: number } => {
    const [pid, pn] = splitKey(key);
    const part = doc.parts.find((p) => p.id === pid);
    if (!part) return { x: 0, y: 0 };
    const g = geom(part, defs).pins[pn];
    return g ? { x: even(g.x + dx), y: even(g.y + dy) } : { x: part.x + dx, y: part.y + dy };
  };
  const resistor = (ohms: number, at: { x: number; y: number }): Part => {
    const name = rname('R');
    return {
      id: `adv${name}${Date.now().toString(36)}`,
      def: `res?ohms=${ohms}&w=0.25`,
      name,
      x: at.x,
      y: at.y,
      rot: 0,
    };
  };
  const wiresAt = (key: string) => doc.wires.filter((w) => w.a === key || w.b === key);
  const other = (w: { a: string; b: string }, key: string) => (w.a === key ? w.b : w.a);
  const gndOf = (partId: string) => {
    const part = doc.parts.find((p) => p.id === partId);
    const g = part ? defs(part.def)?.pins.find((p) => p.kind === 'gnd') : undefined;
    return g ? `${partId}.${g.id}` : undefined;
  };
  const supplyOf = (partId: string, volts: number) => {
    const part = doc.parts.find((p) => p.id === partId);
    const g = part
      ? defs(part.def)?.pins.find(
          (p) => p.kind === 'supply' && Math.abs((p.volts ?? 0) - volts) < 0.2,
        )
      : undefined;
    return g ? `${partId}.${g.id}` : undefined;
  };
  const [k0, k1] = issue.keys;
  if (!k0) return [];
  switch (issue.code as string) {
    case 'led-no-resistor': {
      const ohms = parseOhms(issue.more);
      // Put it on the anode side if a wire goes there, else on the cathode side.
      for (const key of [k0, k1].filter((x): x is string => !!x)) {
        const ws = wiresAt(key);
        if (!ws.length) continue;
        const r = resistor(ohms, near(key, -6, -12));
        return [
          {
            label: `Add a ${ohmShort(ohms)} resistor in series`,
            hint: "Limits the LED's current",
            remove: ws.map((w) => w.id),
            add: [
              ...ws.map((w): [string, string] => [other(w, key), `${r.id}.1`]),
              [`${r.id}.2`, key],
            ],
            addParts: [r],
          },
        ];
      }
      return [];
    }
    case 'led-backwards': {
      const a = wiresAt(k0);
      const k = k1 ? wiresAt(k1) : [];
      if (!k1 || (!a.length && !k.length)) return [];
      return [
        {
          label: 'Turn it round',
          hint: 'Swaps its two legs',
          remove: [...a, ...k].map((w) => w.id),
          add: [
            ...a.map((w): [string, string] => [other(w, k0), k1]),
            ...k.map((w): [string, string] => [other(w, k1), k0]),
          ],
        },
      ];
    }
    case 'needs-pull-up': {
      const [pid] = splitKey(k0);
      const five = supplyOf(pid, 5) ?? supplyOf(pid, 3.3);
      const fixes: AdviceFix[] = [
        {
          label: 'I use INPUT_PULLUP in the code',
          hint: "The chip's own pull-up: no part needed",
          pullups: [k0],
        },
      ];
      if (five) {
        const r = resistor(10000, near(k0, 0, -14));
        fixes.push({
          label: 'Add a 10 kΩ pull-up resistor',
          hint: 'From the pin to 5V',
          add: [
            [k0, `${r.id}.1`],
            [`${r.id}.2`, five],
          ],
          addParts: [r],
        });
      }
      return fixes;
    }
    case 'needs-pull-down': {
      const [pid] = splitKey(k0);
      const gnd = gndOf(pid);
      if (!gnd) return [];
      const r = resistor(10000, near(k1 ?? k0, -8, 10));
      return [
        {
          label: 'Add a 10 kΩ pull-down resistor',
          hint: 'From the pin to GND',
          add: [
            [k1 ?? k0, `${r.id}.1`],
            [`${r.id}.2`, gnd],
          ],
          addParts: [r],
        },
      ];
    }
    case 'needs-divider': {
      if (!k1) return [];
      const [pid] = splitKey(k0);
      const gnd = gndOf(pid);
      const vmax = Number(/on [^']*'s ([\d.]+) V/.exec(issue.text)?.[1] ?? 0);
      const logic = Number(/takes ([\d.]+) V/.exec(issue.text)?.[1] ?? 5);
      if (!gnd || !vmax) return [];
      const d = divider(vmax, logic);
      const top = resistor(d.top, near(k0, -14, -16));
      const bottom = resistor(d.bottom, near(k0, 2, -16));
      const direct = issue.wires.filter((id) => {
        const w = doc.wires.find((x) => x.id === id)!;
        return [w.a, w.b].includes(k0);
      });
      return [
        {
          label: `Add a divider: ${ohmShort(d.top)} and ${ohmShort(d.bottom)}`,
          hint: `The pin sees at most ${d.out} V`,
          remove: direct,
          add: [
            [k1, `${top.id}.1`],
            [`${top.id}.2`, k0],
            [k0, `${bottom.id}.1`],
            [`${bottom.id}.2`, gnd],
          ],
          addParts: [top, bottom],
        },
      ];
    }
    case 'needs-flyback': {
      if (!k1) return [];
      const at = near(k0, 0, 12);
      const name = rname('D');
      const dd: Part = {
        id: `adv${name}${Date.now().toString(36)}`,
        def: '1n4007',
        name,
        x: at.x,
        y: at.y,
        rot: 0,
      };
      return [
        {
          label: 'Add a 1N4007 across the coil',
          hint: 'Stripe to 86 (+)',
          add: [
            [`${dd.id}.K`, k0],
            [`${dd.id}.A`, k1],
          ],
          addParts: [dd],
        },
      ];
    }
  }
  return [];
}

function parseOhms(text: string): number {
  const m = /A ([\d.]+)(k|M|Ω)? resistor/.exec(text);
  if (!m) return 330;
  const x = Number(m[1]);
  return m[2] === 'k' ? x * 1000 : m[2] === 'M' ? x * 1e6 : x;
}

const even = (v: number) => Math.round(v / 2) * 2;
