/**
 * The lab's state: the diagram, what is selected, and everything worked out from the diagram
 * (nets, routes, checks, build steps). Every change goes through an action here, so undo and
 * saving see it.
 */
import { buildSteps } from '../lib/build';
import { runChecks } from '../lib/checks';
import { connectOptions, type ConnectOption } from '../lib/connect';
import { seatShifter, sennepExample } from '../lib/example';
import { geom } from '../lib/geometry';
import { arrange, mountedOn, overlapsAny, settle } from '../lib/layout';
import { computeNets } from '../lib/nets';
import { BUILTIN } from '../lib/parts';
import { routeAll } from '../lib/router';
import type { Diagram, Dir, Part, PartDef, Wire } from '../lib/types';
import { splitKey } from '../lib/types';
import { wirePaths } from './draw';
import { G } from './looks';
import type { MenuItem } from './menu';

const SAVE_KEY = 'faber-wiring-lab';
const CUSTOM_KEY = 'faber-wiring-lab-parts';

export type Selection = { part: string } | { wire: string } | null;
export type Tab = 'sel' | 'checks' | 'build';

function load<T>(key: string): T | null {
  try {
    const s = localStorage.getItem(key);
    return s ? (JSON.parse(s) as T) : null;
  } catch {
    return null;
  }
}
function keep(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // A private window: the lab still works, it just forgets.
  }
}

function example(): Diagram {
  const doc = sennepExample();
  seatShifter(doc);
  arrange(doc, (id) => BUILTIN.find((d) => d.id === id));
  return doc;
}

class Lab {
  doc = $state<Diagram>(load<Diagram>(SAVE_KEY) ?? example());
  custom = $state<PartDef[]>(load<PartDef[]>(CUSTOM_KEY) ?? []);
  sel = $state<Selection>(null);
  /** Parts and wires to show; the rest fades. */
  hl = $state<{ parts: string[]; wires: string[] } | null>(null);
  /** The pin a wire is being drawn from. */
  connecting = $state<string | null>(null);
  /** Wires a menu item would add, drawn dashed while it is hovered. */
  preview = $state<ConnectOption | null>(null);
  tab = $state<Tab>('sel');
  view = $state({ tx: 0, ty: 0, k: 1 });
  hint = $state('');
  private history: string[] = [];

  defs = (id: string): PartDef | undefined => BUILTIN.find((d) => d.id === id) ?? this.custom.find((d) => d.id === id);
  allDefs = $derived([...BUILTIN, ...this.custom]);

  /** A plain copy of the diagram: the routing reads it thousands of times, so not through proxies. */
  snap = $derived.by(() => $state.snapshot(this.doc) as Diagram);
  nets = $derived.by(() => computeNets(this.snap, this.defs));
  routing = $derived.by(() => routeAll(this.snap, this.defs, this.nets));
  issues = $derived.by(() => runChecks(this.snap, this.defs, this.nets, this.routing));
  paths = $derived.by(() => wirePaths(this.snap, this.nets, this.routing));
  steps = $derived.by(() => buildSteps(this.snap, this.defs, this.nets));
  /** The open right-click menu. */
  menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  newPartOpen = $state(false);

  constructor() {
    $effect.root(() => {
      $effect(() => keep(SAVE_KEY, $state.snapshot(this.doc)));
      $effect(() => keep(CUSTOM_KEY, $state.snapshot(this.custom)));
    });
  }

  /** Call before any change that undo should take back. */
  remember(): void {
    this.history.push(JSON.stringify($state.snapshot(this.doc)));
    if (this.history.length > 80) this.history.shift();
  }

  /** A state saved before a change that is already made (a drag). */
  rememberState(json: string): void {
    this.history.push(json);
  }

  undo(): void {
    const prev = this.history.pop();
    if (!prev) return;
    this.doc = JSON.parse(prev) as Diagram;
    this.sel = null;
    this.hl = null;
  }

  part(id: string): Part | undefined {
    return this.doc.parts.find((p) => p.id === id);
  }

  say(text: string): void {
    this.hint = text;
    const mine = text;
    setTimeout(() => {
      if (this.hint === mine && !this.connecting) this.hint = '';
    }, 3600);
  }

  // ---- parts ------------------------------------------------------------------------------

  freeName(base: string): string {
    let n = 1;
    while (this.doc.parts.some((p) => p.name === base + (n > 1 ? `-${n}` : ''))) n += 1;
    return base + (n > 1 ? `-${n}` : '');
  }

  addPart(defId: string, at?: [number, number]): void {
    const d = this.defs(defId);
    if (!d) return;
    this.remember();
    const base = { hcsr04: 'sonar', bts7960: 'driver', dcmotor: 'motor', bat3s: 'battery', buck5: 'buck', mpu6050: 'imu', vl53l0x: 'tof', sg90: 'servo', pi5: 'pi', shifter: 'shifter' }[d.id] ?? d.id;
    const [cx, cy] = at ?? this.viewCentre();
    const part: Part = { id: 'p' + Date.now().toString(36), def: d.id, name: this.freeName(base), x: even(cx - d.w / 2), y: even(cy - d.h / 2), rot: 0 };
    this.doc.parts.push(part);
    const live = this.doc.parts[this.doc.parts.length - 1]!;
    settle(this.doc, this.defs, live);
    this.sel = { part: part.id };
    this.tab = 'sel';
  }

  viewCentre(): [number, number] {
    const el = document.getElementById('canvas');
    const r = el?.getBoundingClientRect();
    if (!r) return [0, 0];
    return [((r.width / 2 - this.view.tx) / this.view.k) / G, ((r.height / 2 - this.view.ty) / this.view.k) / G];
  }

  /** Parts plugged into a breadboard move with it. */
  riders(id: string): Part[] {
    const on = mountedOn(this.doc, this.defs);
    return this.doc.parts.filter((p) => on.get(p.id) === id);
  }

  /** After a drag: keep clear of other parts, unless the part was set down on a breadboard. */
  dropped(part: Part): void {
    const plugged = mountedOn(this.doc, this.defs).has(part.id);
    if (plugged) {
      this.say(`${part.name} is plugged into the breadboard.`);
      return;
    }
    if (overlapsAny(this.doc, this.defs, part, 8, new Set(this.riders(part.id).map((p) => p.id)))) {
      const before = [part.x, part.y];
      settle(this.doc, this.defs, part, undefined, new Set(this.riders(part.id).map((p) => p.id)));
      const [dx, dy] = [part.x - before[0]!, part.y - before[1]!];
      if (this.defs(part.def)?.kind === 'breadboard') {
        for (const r of this.doc.parts.filter((p) => p !== part && mountedOnBefore(this, p, part, dx, dy))) {
          r.x += dx;
          r.y += dy;
        }
      }
    }
  }

  rotate(id: string): void {
    const p = this.part(id);
    if (!p || this.defs(p.def)?.kind === 'breadboard') return;
    this.remember();
    const g = geom(p, this.defs);
    const cx = g.x0 + g.w / 2;
    const cy = g.y0 + g.h / 2;
    p.rot = ((p.rot + 1) & 3) as Dir;
    const g2 = geom(p, this.defs);
    p.x = even(cx - g2.w / 2);
    p.y = even(cy - g2.h / 2);
    if (!mountedOn(this.doc, this.defs).has(p.id)) settle(this.doc, this.defs, p);
  }

  duplicate(id: string): void {
    const p = this.part(id);
    if (!p) return;
    this.remember();
    const copy: Part = { ...$state.snapshot(p), id: 'p' + Date.now().toString(36), name: this.freeName(p.name.replace(/-\d+$/, '')) };
    copy.x += 8;
    copy.y += 8;
    this.doc.parts.push(copy);
    settle(this.doc, this.defs, this.doc.parts[this.doc.parts.length - 1]!);
    this.sel = { part: copy.id };
  }

  removePart(id: string): void {
    this.remember();
    this.doc.parts = this.doc.parts.filter((p) => p.id !== id);
    this.doc.wires = this.doc.wires.filter((w) => splitKey(w.a)[0] !== id && splitKey(w.b)[0] !== id);
    this.sel = null;
    this.hl = null;
  }

  rename(id: string, name: string): void {
    const p = this.part(id);
    const clean = name.trim();
    if (!p || !clean || clean === p.name) return;
    this.remember();
    p.name = clean;
  }

  // ---- wires ------------------------------------------------------------------------------

  addWire(a: string, b: string): Wire {
    const w: Wire = { id: `w${this.doc.seq++}`, a, b };
    this.doc.wires.push(w);
    return w;
  }

  connectPins(a: string, b: string): void {
    this.remember();
    const w = this.addWire(a, b);
    this.sel = { wire: w.id };
    this.tab = 'sel';
  }

  removeWire(id: string): void {
    this.remember();
    this.doc.wires = this.doc.wires.filter((w) => w.id !== id);
    if (this.sel && 'wire' in this.sel && this.sel.wire === id) this.sel = null;
  }

  removeWiresAt(key: string): void {
    this.remember();
    this.doc.wires = this.doc.wires.filter((w) => w.a !== key && w.b !== key);
  }

  disconnect(partId: string): void {
    this.remember();
    this.doc.wires = this.doc.wires.filter((w) => splitKey(w.a)[0] !== partId && splitKey(w.b)[0] !== partId);
  }

  colour(id: string, color: string | undefined): void {
    const w = this.doc.wires.find((x) => x.id === id);
    if (!w) return;
    this.remember();
    if (color) w.color = color;
    else delete w.color;
  }

  options(from: string, to: string) {
    return connectOptions($state.snapshot(this.doc) as Diagram, this.defs, from, to);
  }

  apply(option: ConnectOption): void {
    this.remember();
    for (const p of option.addParts) {
      this.doc.parts.push({ ...p });
      settle(this.doc, this.defs, this.doc.parts[this.doc.parts.length - 1]!);
    }
    for (const [a, b] of option.wires) this.addWire(a, b);
    this.preview = null;
    this.say(`${option.wires.length} wire${option.wires.length > 1 ? 's' : ''} added${option.notes.length ? ' · ' + option.notes[0] : ''}.`);
  }

  // ---- the whole drawing ------------------------------------------------------------------

  arrange(): void {
    this.remember();
    const copy = $state.snapshot(this.doc) as Diagram;
    arrange(copy, this.defs);
    for (const p of this.doc.parts) {
      const q = copy.parts.find((x) => x.id === p.id)!;
      p.x = q.x;
      p.y = q.y;
      p.rot = q.rot;
    }
  }

  reset(): void {
    this.remember();
    this.doc = example();
    this.sel = null;
    this.hl = null;
  }

  fit(): void {
    const el = document.getElementById('canvas');
    if (!el || !this.doc.parts.length) return;
    let minx = Infinity;
    let miny = Infinity;
    let maxx = -Infinity;
    let maxy = -Infinity;
    for (const p of this.doc.parts) {
      const g = geom(p, this.defs);
      minx = Math.min(minx, g.x0);
      miny = Math.min(miny, g.y0);
      maxx = Math.max(maxx, g.x1);
      maxy = Math.max(maxy, g.y1);
    }
    for (const r of this.routing.routes.values()) {
      for (const [x, y] of r.pts) {
        minx = Math.min(minx, x);
        miny = Math.min(miny, y);
        maxx = Math.max(maxx, x);
        maxy = Math.max(maxy, y);
      }
    }
    const r = el.getBoundingClientRect();
    const pad = 36;
    const k = Math.min((r.width - pad * 2) / ((maxx - minx) * G), (r.height - pad * 2 - 40) / ((maxy - miny) * G), 2.2);
    this.view = { k, tx: r.width / 2 - ((minx + maxx) / 2) * G * k, ty: (r.height - 40) / 2 - ((miny + maxy) / 2) * G * k };
  }

  zoomAt(sx: number, sy: number, f: number): void {
    const k = Math.min(5, Math.max(0.15, this.view.k * f));
    this.view = { k, tx: sx - ((sx - this.view.tx) / this.view.k) * k, ty: sy - ((sy - this.view.ty) / this.view.k) * k };
  }

  tick(id: string, done: boolean): void {
    this.doc.done[id] = done;
  }
}

function mountedOnBefore(lab: Lab, rider: Part, board: Part, dx: number, dy: number): boolean {
  // Was `rider` plugged into `board` before the board moved by (dx, dy)?
  rider.x += dx;
  rider.y += dy;
  const on = mountedOn(lab.doc, lab.defs).get(rider.id) === board.id;
  rider.x -= dx;
  rider.y -= dy;
  return on;
}

export function even(n: number): number {
  return Math.round(n / 2) * 2;
}

export const lab = new Lab();
