/**
 * One Circuits window's state (docs/circuits.md): the drawing, what is selected, and everything
 * worked out from the drawing (nets, routes, checks, build steps, power). Every change goes
 * through an action here, so undo, redo and saving see it. Each window has its own, through
 * Svelte's context.
 */
import { getContext, setContext } from 'svelte';
import { t } from '../../lib/i18n';
import { buildSteps } from '../../lib/circuit/build';
import { listed, lookup } from '../../lib/circuit/catalog';
import { runChecks } from '../../lib/circuit/checks';
import type { PinState } from '../../lib/circuit/circuit';
import { connectOptions, type ConnectOption } from '../../lib/circuit/connect';
import { seatShifter, sennepExample } from '../../lib/circuit/example';
import { applyFix, fixesFor, type Fix } from '../../lib/circuit/fixes';
import { geom } from '../../lib/circuit/geometry';
import { isParametric, paramValues, variantId } from '../../lib/circuit/library';
import {
  arrange,
  mirrorLayout,
  mountedOn,
  overlapsAny,
  settle,
  snapToBreadboard,
} from '../../lib/circuit/layout';
import { computeNets } from '../../lib/circuit/nets';
import { simulate } from '../../lib/circuit/power';
import { quickRoutes, routeAll, type Routing } from '../../lib/circuit/router';
import { placeSennep, sennepV2 } from '../../lib/circuit/sennep';
import type { Diagram, Dir, Gauge, Part, PartDef, Wire } from '../../lib/circuit/types';
import { splitKey } from '../../lib/circuit/types';
import { wirePaths } from './draw';
import { G } from './looks';
import type { MenuItem } from './menuItem';

const PANELS_KEY = 'faber.circuit.panels.v1';

export type Tab = 'sel' | 'checks' | 'build';

/** What a `.fbcircuit` file holds. */
export interface CircuitFile {
  faberCircuit: 1;
  doc: Diagram;
  /** Parts drawn for this circuit (New part), so the file opens anywhere. */
  custom: PartDef[];
}

export function blankDiagram(name: string): Diagram {
  return { name, parts: [], wires: [], done: {}, seq: 1 };
}

/** The examples: SENNEP V2 whole, or one corner of it (small enough to read at a glance). */
export function example(which: 'machine' | 'corner'): Diagram {
  if (which === 'machine') {
    // The whole machine has its own layout by job (Arrange packs 66 parts too tightly).
    const doc = sennepV2();
    placeSennep(doc);
    return doc;
  }
  const doc = sennepExample();
  seatShifter(doc);
  arrange(doc, lookup());
  return doc;
}

/** Reads a file's bytes; `null` when they aren't a circuit. */
export function parseCircuit(text: string): CircuitFile | null {
  try {
    const v = JSON.parse(text) as Partial<CircuitFile>;
    if (
      v.faberCircuit !== 1 ||
      !v.doc ||
      !Array.isArray(v.doc.parts) ||
      !Array.isArray(v.doc.wires)
    )
      return null;
    return { faberCircuit: 1, doc: migrate(v.doc, v.custom ?? []), custom: v.custom ?? [] };
  } catch {
    return null;
  }
}

/** Drawings from an older version: renamed parts, and parts it no longer knows. */
function migrate(doc: Diagram, custom: PartDef[]): Diagram {
  const look = lookup(custom);
  const renamed: Record<string, string> = {
    bat3s: 'bat?s=3&p=6&cell=4.2&nom=3.6&mah=3400',
    buck5: 'buck5?v=5.1',
    fuse: 'fuse?a=5',
  };
  for (const p of doc.parts) {
    p.def = renamed[p.def] ?? p.def;
    delete (p as Part & { flip?: boolean }).flip;
  }
  const known = new Set(doc.parts.filter((p) => look(p.def)).map((p) => p.id));
  doc.parts = doc.parts.filter((p) => known.has(p.id));
  doc.wires = doc.wires.filter((w) => {
    const [a, pa] = splitKey(w.a);
    const [b, pb] = splitKey(w.b);
    const ok = (id: string, pin: string) =>
      known.has(id) &&
      look(doc.parts.find((p) => p.id === id)!.def)!.pins.some((x) => x.id === pin);
    return ok(a, pa) && ok(b, pb);
  });
  doc.done ??= {};
  doc.seq ??= doc.wires.length + 1;
  return doc;
}

function loadPanels(): { left: number; right: number; leftOpen: boolean; rightOpen: boolean } {
  try {
    const s = localStorage.getItem(PANELS_KEY);
    if (s) return JSON.parse(s);
  } catch {
    // A private window: default panels.
  }
  return { left: 236, right: 340, leftOpen: true, rightOpen: true };
}

let uid = 0;
const newId = (prefix: string) => `${prefix}${Date.now().toString(36)}${(uid++).toString(36)}`;

interface Clip {
  parts: Part[];
  wires: Wire[];
}

export class CircuitStore {
  custom = $state<PartDef[]>([]);
  doc = $state<Diagram>(blankDiagram(''));
  /** The drawing as last saved, to tell whether it has changes. */
  saved = $state('');
  /** Selected parts, and at most one selected wire. */
  selParts = $state<string[]>([]);
  selWire = $state<string | null>(null);
  /** Parts and wires to show; the rest fades. */
  hl = $state<{ parts: string[]; wires: string[] } | null>(null);
  connecting = $state<string | null>(null);
  preview = $state<ConnectOption | null>(null);
  /** A fix being looked at: its wires drawn dashed. */
  fixPreview = $state<Fix | null>(null);
  tab = $state<Tab>('sel');
  view = $state({ tx: 0, ty: 0, k: 1 });
  hint = $state('');
  power = $state(false);
  /** Power mode's motors: off, driving freely, or held still (the worst case for wires and fuses). */
  motors = $state<'off' | 'run' | 'stall'>('off');
  /** Board pins set by running code (the emulator) or a live board, for power mode. */
  pins = $state.raw<Map<string, PinState>>(new Map());
  /** The real parts' state in Faber, by part name (running, failed …): a part on the drawing with the same name shows it. */
  live = $state.raw<Map<string, string>>(new Map());
  panels = $state(loadPanels());
  menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  newPartOpen = $state(false);
  /** The drawing's element, for sizes (fit, the middle of the view). */
  el: HTMLElement | null = null;
  private undoStack: string[] = [];
  private redoStack: string[] = [];
  private clip: Clip | null = null;

  defs = $derived(lookup(this.custom));
  allDefs = $derived(listed(this.custom));

  /** A plain copy of the diagram: the routing reads it thousands of times, so not through proxies. */
  snap = $derived.by(() => $state.snapshot(this.doc) as Diagram);
  dirty = $derived(JSON.stringify(this.snap) !== this.saved);
  nets = $derived.by(() => computeNets(this.snap, this.defs));
  /** Parts being dragged: their wires are drawn straight until the drop (routing a big machine is slow). */
  dragging = $state<Set<string> | null>(null);
  private lastRouting: Routing | null = null;
  routing = $derived.by(() => {
    if (this.dragging && this.lastRouting)
      return quickRoutes(this.snap, this.defs, this.lastRouting, this.dragging);
    const r = routeAll(this.snap, this.defs, this.nets);
    this.lastRouting = r;
    return r;
  });
  issues = $derived.by(() => runChecks(this.snap, this.defs, this.nets, this.routing));
  paths = $derived.by(() => wirePaths(this.snap, this.nets, this.routing));
  steps = $derived.by(() => buildSteps(this.snap, this.defs, this.nets));
  sim = $derived.by(() =>
    this.power ? simulate(this.snap, this.defs, { motors: this.motors, pins: this.pins }) : null,
  );

  constructor(file?: CircuitFile) {
    if (file) this.load(file);
    $effect(() => {
      try {
        localStorage.setItem(PANELS_KEY, JSON.stringify($state.snapshot(this.panels)));
      } catch {
        // A private window: the panels are as they are.
      }
    });
  }

  load(file: CircuitFile): void {
    this.custom = file.custom;
    this.doc = file.doc;
    this.saved = JSON.stringify(file.doc);
    this.undoStack = [];
    this.redoStack = [];
    this.clearSelection();
  }

  /** The file to write, and marks the drawing as saved. */
  toFile(): CircuitFile {
    const doc = $state.snapshot(this.doc) as Diagram;
    this.saved = JSON.stringify(doc);
    return { faberCircuit: 1, doc, custom: $state.snapshot(this.custom) as PartDef[] };
  }

  // ---- history ----------------------------------------------------------------------------

  /** Call before any change that undo should take back. */
  remember(): void {
    this.rememberState(JSON.stringify($state.snapshot(this.doc)));
  }

  /** A state saved before a change that is already made (a drag). */
  rememberState(json: string): void {
    this.undoStack.push(json);
    if (this.undoStack.length > 100) this.undoStack.shift();
    this.redoStack = [];
  }

  undo(): void {
    const prev = this.undoStack.pop();
    if (!prev) return this.say(t('circuit.say.nothingToUndo'));
    this.redoStack.push(JSON.stringify($state.snapshot(this.doc)));
    this.doc = JSON.parse(prev) as Diagram;
    this.tidySelection();
  }

  redo(): void {
    const next = this.redoStack.pop();
    if (!next) return this.say(t('circuit.say.nothingToRedo'));
    this.undoStack.push(JSON.stringify($state.snapshot(this.doc)));
    this.doc = JSON.parse(next) as Diagram;
    this.tidySelection();
  }

  private tidySelection(): void {
    this.selParts = this.selParts.filter((id) => this.part(id));
    if (this.selWire && !this.doc.wires.some((w) => w.id === this.selWire)) this.selWire = null;
    this.hl = null;
  }

  // ---- selection --------------------------------------------------------------------------

  part(id: string): Part | undefined {
    return this.doc.parts.find((p) => p.id === id);
  }

  select(ids: string[], add = false): void {
    this.selParts = add ? [...new Set([...this.selParts, ...ids])] : ids;
    this.selWire = null;
    this.hl = null;
    this.tab = 'sel';
  }

  toggle(id: string): void {
    this.selParts = this.selParts.includes(id)
      ? this.selParts.filter((x) => x !== id)
      : [...this.selParts, id];
    this.selWire = null;
  }

  selectWire(id: string): void {
    this.selParts = [];
    this.selWire = id;
    this.hl = null;
    this.tab = 'sel';
  }

  selectAll(): void {
    this.select(this.doc.parts.map((p) => p.id));
    this.say(t('circuit.say.selected', { count: this.selParts.length }));
  }

  clearSelection(): void {
    this.selParts = [];
    this.selWire = null;
    this.hl = null;
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

  addPart(defId: string, at?: [number, number], name?: string): Part | undefined {
    const d = this.defs(defId);
    if (!d) return undefined;
    this.remember();
    const names: Record<string, string> = {
      hcsr04: 'sonar',
      bts7960: 'driver',
      dcmotor: 'motor',
      bat: 'battery',
      buck5: 'buck',
      mpu6050: 'imu',
      vl53l0x: 'tof',
      sg90: 'servo',
      pi5: 'pi',
      ld19: 'lidar',
      cp2102: 'usb-uart',
      bms3s: 'bms',
      ws2812: 'leds',
      res: 'R',
      cap: 'C',
    };
    const base =
      name ?? names[defId.split('?')[0]!] ?? defId.split('?')[0]!.replace(/^custom-/, '');
    const [cx, cy] = at ?? this.viewCentre();
    const part: Part = {
      id: newId('p'),
      def: defId,
      name: this.freeName(base),
      x: even(cx - d.w / 2),
      y: even(cy - d.h / 2),
      rot: 0,
    };
    this.doc.parts.push(part);
    const live = this.doc.parts[this.doc.parts.length - 1]!;
    if (!snapToBreadboard(this.doc, this.defs, live)) settle(this.doc, this.defs, live);
    this.select([part.id]);
    return live;
  }

  viewCentre(): [number, number] {
    const r = this.el?.getBoundingClientRect();
    if (!r) return [0, 0];
    return [
      (r.width / 2 - this.view.tx) / this.view.k / G,
      (r.height / 2 - this.view.ty) / this.view.k / G,
    ];
  }

  /** Parts plugged into a breadboard move with it. */
  riders(id: string): Part[] {
    const on = mountedOn(this.doc, this.defs);
    return this.doc.parts.filter((p) => on.get(p.id) === id);
  }

  /** After a drag: drop into a breadboard if close, else keep clear of other parts. */
  dropped(parts: Part[]): void {
    const moving = new Set(parts.flatMap((p) => [p.id, ...this.riders(p.id).map((r) => r.id)]));
    let plugged = 0;
    for (const part of parts) {
      if (snapToBreadboard(this.doc, this.defs, part)) {
        plugged += 1;
        continue;
      }
      if (parts.length === 1 && overlapsAny(this.doc, this.defs, part, 8, moving)) {
        const before = [part.x, part.y];
        const riders = this.riders(part.id);
        settle(this.doc, this.defs, part, undefined, moving);
        for (const r of riders) {
          r.x += part.x - before[0]!;
          r.y += part.y - before[1]!;
        }
      }
    }
    if (plugged)
      this.say(
        plugged === 1 && parts.length === 1
          ? t('circuit.say.pluggedOne', { name: parts[0]!.name })
          : t('circuit.say.plugged', { count: plugged }),
      );
  }

  private around(ids: string[]): { cx: number; cy: number } {
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    for (const id of ids) {
      const g = geom(this.part(id)!, this.defs);
      x0 = Math.min(x0, g.x0);
      y0 = Math.min(y0, g.y0);
      x1 = Math.max(x1, g.x1);
      y1 = Math.max(y1, g.y1);
    }
    return { cx: (x0 + x1) / 2, cy: (y0 + y1) / 2 };
  }

  /** Turns the selection a quarter (clockwise, or back with `ccw`) about its middle. */
  rotate(ids = this.selParts, ccw = false): void {
    const parts = ids
      .map((id) => this.part(id))
      .filter((p): p is Part => !!p && this.defs(p.def)?.kind !== 'breadboard');
    if (!parts.length) return;
    this.remember();
    const { cx, cy } = this.around(parts.map((p) => p.id));
    for (const p of parts) {
      const g = geom(p, this.defs);
      const [mx, my] = [g.x0 + g.w / 2 - cx, g.y0 + g.h / 2 - cy];
      const [nx, ny] = parts.length > 1 ? (ccw ? [my, -mx] : [-my, mx]) : [0, 0];
      p.rot = ((p.rot + (ccw ? 3 : 1)) & 3) as Dir;
      const g2 = geom(p, this.defs);
      p.x = even(cx + nx - g2.w / 2);
      p.y = even(cy + ny - g2.h / 2);
    }
    if (parts.length === 1 && !mountedOn(this.doc, this.defs).has(parts[0]!.id))
      settle(this.doc, this.defs, parts[0]!);
  }

  /**
   * Mirrors the layout of the selection left–right (`h`) or top–bottom (`v`): where the parts sit,
   * not the parts. A real board can't have its pins mirrored, so every part keeps its own face and
   * turn; parts on a breadboard move with it.
   */
  mirror(axis: 'h' | 'v', ids = this.selParts): void {
    const parts = ids.map((id) => this.part(id)).filter((p): p is Part => !!p);
    if (parts.length < 2) {
      this.say(t('circuit.say.mirrorNeedsTwo'));
      return;
    }
    this.remember();
    mirrorLayout(
      this.doc,
      this.defs,
      parts.map((p) => p.id),
      axis,
    );
    this.say(axis === 'h' ? t('circuit.say.mirroredH') : t('circuit.say.mirroredV'));
  }

  nudge(dx: number, dy: number): void {
    if (!this.selParts.length) return;
    this.remember();
    const moving = new Set<string>();
    for (const id of this.selParts) {
      moving.add(id);
      for (const r of this.riders(id)) moving.add(r.id);
    }
    for (const id of moving) {
      const p = this.part(id)!;
      p.x += dx;
      p.y += dy;
    }
  }

  // ---- clipboard --------------------------------------------------------------------------

  copy(): void {
    if (!this.selParts.length) return this.say(t('circuit.say.selectToCopy'));
    const ids = new Set(this.selParts.flatMap((id) => [id, ...this.riders(id).map((r) => r.id)]));
    const snap = $state.snapshot(this.doc) as Diagram;
    this.clip = {
      parts: snap.parts.filter((p) => ids.has(p.id)),
      wires: snap.wires.filter((w) => ids.has(splitKey(w.a)[0]) && ids.has(splitKey(w.b)[0])),
    };
    this.say(
      t('circuit.say.copied', { count: this.clip.parts.length, wires: this.clip.wires.length }),
    );
  }

  cut(): void {
    if (!this.selParts.length) return;
    this.copy();
    this.removeParts(this.selParts);
  }

  paste(offset = 12): void {
    if (!this.clip) return this.say(t('circuit.say.nothingCopied'));
    this.remember();
    const map = new Map<string, string>();
    const added: string[] = [];
    for (const p of this.clip.parts) {
      const id = newId('p');
      map.set(p.id, id);
      this.doc.parts.push({
        ...p,
        id,
        name: this.freeName(p.name.replace(/-\d+$/, '')),
        x: p.x + offset,
        y: p.y + offset,
      });
      added.push(id);
    }
    for (const w of this.clip.wires) {
      const [a, pa] = splitKey(w.a);
      const [b, pb] = splitKey(w.b);
      this.doc.wires.push({
        ...w,
        id: `w${this.doc.seq++}`,
        a: `${map.get(a)}.${pa}`,
        b: `${map.get(b)}.${pb}`,
      });
    }
    const copies = added.map((id) => this.part(id)!);
    for (
      let step = 0;
      step < 60 && copies.some((c) => overlapsAny(this.doc, this.defs, c, 6, new Set(added)));
      step++
    ) {
      for (const c of copies) {
        c.x += 8;
        c.y += 8;
      }
    }
    this.clip = {
      parts: this.clip.parts.map((p) => ({ ...p, x: p.x + offset, y: p.y + offset })),
      wires: this.clip.wires,
    };
    this.select(added);
  }

  duplicate(): void {
    if (!this.selParts.length) return;
    const keep = this.clip;
    this.copy();
    this.paste();
    this.clip = keep;
  }

  removeParts(ids: string[]): void {
    if (!ids.length) return;
    this.remember();
    const gone = new Set(ids);
    this.doc.parts = this.doc.parts.filter((p) => !gone.has(p.id));
    this.doc.wires = this.doc.wires.filter(
      (w) => !gone.has(splitKey(w.a)[0]) && !gone.has(splitKey(w.b)[0]),
    );
    this.clearSelection();
  }

  removeSelection(): void {
    if (this.selWire) this.removeWire(this.selWire);
    else this.removeParts(this.selParts);
  }

  /** A wire's gauge and real length, for the current it can carry and the voltage it loses. */
  setWire(id: string, change: { gauge?: Gauge; len?: number }): void {
    const w = this.doc.wires.find((x) => x.id === id);
    if (!w) return;
    this.remember();
    if (change.gauge) w.gauge = change.gauge;
    if (change.len !== undefined && change.len > 0) w.len = change.len;
  }

  rename(id: string, name: string): void {
    const p = this.part(id);
    const clean = name.trim();
    if (!p || !clean || clean === p.name) return;
    this.remember();
    p.name = clean;
  }

  /** Changes a parametric part's settings (a battery's cells). */
  setParam(id: string, key: string, value: number): void {
    const p = this.part(id);
    const d = p ? this.defs(p.def) : undefined;
    if (!p || !d?.params || !isParametric(p.def) || !Number.isFinite(value)) return;
    this.remember();
    const v = { ...paramValues(p.def), [key]: value };
    p.def = variantId(p.def.split('?')[0]!, d.params, v);
  }

  /** Power mode: open or close a switch, press an e-stop. */
  flick(id: string): void {
    const p = this.part(id);
    if (!p || !this.defs(p.def)?.switch) return;
    this.remember();
    p.open = !p.open;
    this.say(
      p.open
        ? t('circuit.say.opened', { name: p.name })
        : t('circuit.say.closed', { name: p.name }),
    );
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
    this.selectWire(w.id);
  }

  removeWire(id: string): void {
    this.remember();
    this.doc.wires = this.doc.wires.filter((w) => w.id !== id);
    if (this.selWire === id) this.selWire = null;
  }

  removeWiresAt(key: string): void {
    this.remember();
    this.doc.wires = this.doc.wires.filter((w) => w.a !== key && w.b !== key);
  }

  disconnect(ids: string[]): void {
    this.remember();
    const set = new Set(ids);
    this.doc.wires = this.doc.wires.filter(
      (w) => !set.has(splitKey(w.a)[0]) && !set.has(splitKey(w.b)[0]),
    );
  }

  colour(id: string, color: string | undefined): void {
    const w = this.doc.wires.find((x) => x.id === id);
    if (!w) return;
    this.remember();
    if (color) w.color = color;
    else delete w.color;
  }

  options(from: string, to: string) {
    return connectOptions(this.snap, this.defs, from, to);
  }

  apply(option: ConnectOption): void {
    this.remember();
    for (const p of option.addParts) {
      this.doc.parts.push({ ...p });
      settle(this.doc, this.defs, this.doc.parts[this.doc.parts.length - 1]!);
    }
    for (const [a, b] of option.wires) this.addWire(a, b);
    this.preview = null;
    this.say(
      t('circuit.say.wiresAdded', { count: option.wires.length }) +
        (option.notes.length ? ` · ${option.notes[0]}` : ''),
    );
  }

  fixes(index: number): Fix[] {
    const issue = this.issues[index];
    return issue ? fixesFor(this.snap, this.defs, issue) : [];
  }

  fix(f: Fix): void {
    if (f.arrange) {
      this.arrange();
      return;
    }
    this.remember();
    const before = new Set(this.doc.parts.map((p) => p.id));
    applyFix(this.doc, JSON.parse(JSON.stringify(f)) as Fix);
    for (const p of this.doc.parts.filter((x) => !before.has(x.id))) settle(this.doc, this.defs, p);
    this.fixPreview = null;
    this.hl = null;
    this.say(t('circuit.say.fixed', { what: f.label }));
  }

  /** Every issue that has exactly one fix, fixed. */
  fixAll(): void {
    let n = 0;
    this.remember();
    for (let round = 0; round < 8; round++) {
      const snap = $state.snapshot(this.doc) as Diagram;
      const issues = runChecks(snap, this.defs, computeNets(snap, this.defs));
      const one = issues
        .map((i) => fixesFor(snap, this.defs, i))
        .find((f) => f.length === 1 && !f[0]!.arrange);
      if (!one) break;
      applyFix(this.doc, JSON.parse(JSON.stringify(one[0])) as Fix);
      n += 1;
    }
    this.say(n ? t('circuit.say.fixedMany', { count: n }) : t('circuit.say.noClearFix'));
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

  /** Replaces the drawing (an example, parts from Faber), keeping undo. */
  replace(doc: Diagram): void {
    this.remember();
    this.doc = doc;
    this.clearSelection();
  }

  fit(): void {
    const el = this.el;
    if (!el || !this.doc.parts.length) return;
    let minx = Infinity;
    let miny = Infinity;
    let maxx = -Infinity;
    let maxy = -Infinity;
    const grow = (x: number, y: number) => {
      minx = Math.min(minx, x);
      miny = Math.min(miny, y);
      maxx = Math.max(maxx, x);
      maxy = Math.max(maxy, y);
    };
    for (const p of this.doc.parts) {
      const g = geom(p, this.defs);
      grow(g.x0, g.y0);
      grow(g.x1, g.y1);
    }
    for (const r of this.routing.routes.values()) for (const [x, y] of r.pts) grow(x, y);
    const r = el.getBoundingClientRect();
    const pad = 36;
    const k = Math.min(
      (r.width - pad * 2) / ((maxx - minx) * G),
      (r.height - pad * 2 - 40) / ((maxy - miny) * G),
      2.2,
    );
    this.view = {
      k,
      tx: r.width / 2 - ((minx + maxx) / 2) * G * k,
      ty: (r.height - 40) / 2 - ((miny + maxy) / 2) * G * k,
    };
  }

  zoomAt(sx: number, sy: number, f: number): void {
    const k = Math.min(5, Math.max(0.15, this.view.k * f));
    this.view = {
      k,
      tx: sx - ((sx - this.view.tx) / this.view.k) * k,
      ty: sy - ((sy - this.view.ty) / this.view.k) * k,
    };
  }

  tick(id: string, done: boolean): void {
    this.doc.done[id] = done;
  }
}

export function even(n: number): number {
  return Math.round(n / 2) * 2;
}

const KEY = Symbol('circuit');

export function provideCircuit(store: CircuitStore): void {
  setContext(KEY, store);
}

export function useCircuit(): CircuitStore {
  return getContext<CircuitStore>(KEY);
}
