/**
 * Power mode: switch the drawing on and see what happens. The circuit simulation (circuit.ts)
 * gives volts at every pin and amps in every wire; this turns that into what the drawing shows:
 * which nets are live, which way current runs, what each part makes of it, and what went wrong.
 */
import {
  GAUGES,
  gaugeOf,
  runCircuit,
  type Finding,
  type PartPower,
  type SimOptions,
} from './circuit';
import { computeNets, type NetIndex } from './nets';
import type { DefLookup, Diagram } from './types';

export type { PartPower } from './circuit';

export interface PowerState {
  /** Net id → volts, for every live net. */
  live: Map<number, number>;
  /** Nets carrying a short circuit that nothing stopped. */
  shorts: Set<number>;
  parts: Map<string, PartPower>;
  /** Wire id → 1 if current runs from `a` to `b`, −1 the other way. */
  flow: Map<string, 1 | -1>;
  /** Parts that cut the power (a blown fuse, a tripped BMS). */
  tripped: string[];
  messages: { sev: 'ok' | 'warn' | 'error'; text: string }[];
  /** The nets the simulation ran on (switches as they are, tripped parts open). */
  nets: NetIndex;
  /** Volts at each pin. */
  volts: Map<string, number>;
  /** Amps in each wire, positive from `a` to `b`. */
  amps: Map<string, number>;
  partAmps: Map<string, number>;
  findings: Finding[];
}

export function simulate(doc: Diagram, defs: DefLookup, opt: SimOptions = {}): PowerState {
  const c = runCircuit(doc, defs, opt);
  const view: Diagram = {
    ...doc,
    parts: doc.parts.map((p) => (c.blown.includes(p.id) ? { ...p, open: true } : p)),
  };
  const nets = computeNets(view, defs);
  const live = new Map<number, number>();
  for (const n of nets.nets) {
    const v = Math.max(...n.keys.map((k) => c.volts.get(k) ?? 0));
    if (v > 0.3) live.set(n.id, Math.round(v * 100) / 100);
  }
  const flow = new Map<string, 1 | -1>();
  const shorts = new Set<number>();
  for (const w of doc.wires) {
    const a = c.amps.get(w.id) ?? 0;
    if (Math.abs(a) > 1e-3) flow.set(w.id, a > 0 ? 1 : -1);
    if (Math.abs(a) > 5 && Math.abs(a) > 3 * GAUGES[gaugeOf(w, doc, defs)].amps) {
      const n = nets.of.get(w.a);
      if (n) shorts.add(n.id);
    }
  }
  const name = (id: string) => doc.parts.find((p) => p.id === id)?.name ?? id;
  const messages: PowerState['messages'] = [];
  if (shorts.size)
    messages.push({
      sev: 'error',
      text: 'Short circuit with nothing to stop it: the battery dumps its current through the wires. Fire risk.',
    });
  for (const f of c.findings) if (f.sev !== 'info') messages.push({ sev: f.sev, text: f.text });
  const on = [...c.parts].filter(([, s]) => s === 'on').map(([id]) => name(id));
  const off = [...c.parts]
    .filter(([id, s]) => s === 'off' && hasPowerIn(doc, defs, id))
    .map(([id]) => name(id));
  if (on.length) messages.push({ sev: 'ok', text: `On: ${on.join(', ')}` });
  if (off.length) messages.push({ sev: 'warn', text: `No power: ${off.join(', ')}` });
  return {
    live,
    shorts,
    parts: c.parts,
    flow,
    tripped: c.blown,
    messages,
    nets,
    volts: c.volts,
    amps: c.amps,
    partAmps: c.partAmps,
    findings: c.findings,
  };
}

function hasPowerIn(doc: Diagram, defs: DefLookup, id: string): boolean {
  const d = defs(doc.parts.find((p) => p.id === id)?.def ?? '');
  return !!d?.pins.some(
    (p) =>
      p.kind === 'vcc' ||
      p.kind === 'vin' ||
      p.kind === 'power-in' ||
      (p.kind === 'port' && !p.host && (p.port === 'usb' || p.port === 'usbc-power')),
  );
}
