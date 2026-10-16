import { t } from '../i18n';
import { BRIDGE_CONNECTOR, isSubPart } from './bridges';
import { validatePartName } from './partName';
import type {
  BoardPlan,
  GuidedBoardSummary,
  PartInfo,
  PartSetting,
  PartTemplate,
  PlannedPart,
  RoleOptions,
  SettingValue,
} from './types';

/**
 * Guided mode's logic for the shell (docs/wiring.md §3): how each pin looks for the role being
 * filled, the part catalogue's groups, settings, names, and which wires move when a plan
 * changes. Generic over the templates the core serves, so a new template needs no shell change.
 */

/** How a pin is drawn for the role being filled. */
export type PinKind =
  /** On this role now. */
  | 'chosen'
  /** Faber's first pick for the role, when another pin is chosen. */
  | 'suggested'
  /** Free and able to do the job. */
  | 'suitable'
  /** On another role of this part: picking it swaps the two. */
  | 'other-role'
  | 'unsuitable'
  | 'in-use'
  | 'reserved';

export interface PinLook {
  kind: PinKind;
  /** Why it can't be used, who uses it, or which role of this part has it. */
  reason?: string;
}

/** Pins a click can put on the role. */
export function pickable(kind: PinKind): boolean {
  return kind === 'chosen' || kind === 'suggested' || kind === 'suitable' || kind === 'other-role';
}

/**
 * How `pin` looks while `role` is being filled. `pins` are this part's pins now (role → pin),
 * `firstPick` Faber's first suggestion per role, `roles` every role of the part.
 */
export function pinLook(
  role: RoleOptions,
  pin: string,
  pins: Record<string, string>,
  firstPick: Record<string, string>,
  roles: RoleOptions[],
): PinLook {
  if (pins[role.role] === pin) return { kind: 'chosen' };
  const other = roles.find((r) => r.role !== role.role && pins[r.role] === pin);
  const reserved = role.reserved.find((r) => r.pin === pin);
  if (reserved) return { kind: 'reserved', reason: reserved.reason };
  if (other) return { kind: 'other-role', reason: other.name };
  const used = role.inUse.find((u) => u.pin === pin);
  if (used) return { kind: 'in-use', reason: used.by };
  const unsuitable = role.unsuitable.find((u) => u.pin === pin);
  if (unsuitable) return { kind: 'unsuitable', reason: unsuitable.reason };
  if (firstPick[role.role] === pin || role.suggested === pin) return { kind: 'suggested' };
  if (role.suitable.includes(pin)) return { kind: 'suitable' };
  return { kind: 'unsuitable', reason: t('guided.pin.cantDo', { role: role.name }) };
}

/** What a screen reader says for a pin: "D7, suitable for Echo". */
export function pinLabel(pin: string, look: PinLook, role: string): string {
  switch (look.kind) {
    case 'chosen':
      return t('guided.pin.label.chosen', { pin, role });
    case 'suggested':
      return t('guided.pin.label.suggested', { pin, role });
    case 'suitable':
      return t('guided.pin.label.suitable', { pin, role });
    case 'other-role':
      return t('guided.pin.label.otherRole', { pin, other: look.reason ?? '' });
    case 'unsuitable':
      return t('guided.pin.label.unsuitable', { pin, role, reason: look.reason ?? '' });
    case 'in-use':
      return t('guided.pin.label.inUse', { pin, by: look.reason ?? '' });
    case 'reserved':
      return t('guided.pin.label.reserved', { pin, reason: look.reason ?? '' });
  }
}

/** The catalogue's groups, by what a part does. */
export type PartGroup =
  'distance' | 'motion' | 'inputs' | 'outputs' | 'power' | 'sensors' | 'other';

const GROUPS: Record<string, PartGroup> = {
  distance: 'distance',
  obstacle: 'distance',
  'line-sensor': 'distance',
  presence: 'distance',
  'laser-scan': 'distance',
  motor: 'motion',
  servo: 'motion',
  encoder: 'motion',
  'planar-base': 'motion',
  button: 'inputs',
  'digital-in': 'inputs',
  'analog-in': 'inputs',
  keypad: 'inputs',
  'digital-out': 'outputs',
  'pwm-out': 'outputs',
  tone: 'outputs',
  'text-display': 'outputs',
  'pixel-display': 'outputs',
  battery: 'power',
  'power-meter': 'power',
};

/** A capability's group: a part with a capability Faber has no group for is a sensor. */
export function partGroup(capability: string | undefined): PartGroup {
  if (!capability) return 'other';
  return GROUPS[capability.replace(/@\d+$/, '')] ?? 'sensors';
}

const GROUP_ORDER: PartGroup[] = ['distance', 'motion', 'inputs', 'outputs', 'power', 'sensors'];

/** The templates in groups, groups in a fixed order, each in catalogue order. */
export function templateGroups(
  templates: PartTemplate[],
): { group: PartGroup; title: string; templates: PartTemplate[] }[] {
  const order = [...GROUP_ORDER, 'other' as const];
  return order
    .map((group) => ({
      group,
      title: t(`guided.group.${group}`),
      templates: templates.filter((tpl) => partGroup(tpl.capability) === group),
    }))
    .filter((g) => g.templates.length > 0);
}

/** Every setting at its default. */
export function defaultSettings(template: PartTemplate): Record<string, SettingValue> {
  return Object.fromEntries(template.settings.map((s) => [s.id, s.default]));
}

/** A setting as typed, for its input. */
export function settingText(value: SettingValue | undefined): string {
  return value === undefined ? '' : String(value);
}

/**
 * The value typed for a setting, or what is wrong with it, said with the setting's own unit and
 * limits ("Longest distance: 0.1 to 4 m").
 */
export function readSetting(
  s: PartSetting,
  text: string,
): { ok: true; value: SettingValue } | { ok: false; message: string } {
  if (s.type === 'choice') {
    return s.options.some((o) => o.id === text)
      ? { ok: true, value: text }
      : { ok: false, message: t('guided.setting.choose', { name: s.name }) };
  }
  const trimmed = text.trim().replace(',', '.');
  const n = Number(trimmed);
  if (trimmed === '' || !Number.isFinite(n)) {
    return { ok: false, message: t('guided.setting.number', { name: s.name }) };
  }
  if (s.type === 'integer' && !Number.isInteger(n)) {
    return { ok: false, message: t('guided.setting.whole', { name: s.name }) };
  }
  const unit = s.unit ? ` ${s.unit}` : '';
  if ((s.min !== undefined && n < s.min) || (s.max !== undefined && n > s.max)) {
    if (s.min !== undefined && s.max !== undefined) {
      return {
        ok: false,
        message: t('guided.setting.between', { name: s.name, min: s.min, max: s.max, unit }),
      };
    }
    return s.min !== undefined
      ? { ok: false, message: t('guided.setting.atLeast', { name: s.name, min: s.min, unit }) }
      : { ok: false, message: t('guided.setting.atMost', { name: s.name, max: s.max ?? 0, unit }) };
  }
  return { ok: true, value: n };
}

/**
 * What is wrong with a part's name on this board, or null. The core checks more (names the
 * sketch uses, like `loop`); its answer comes back from the suggestion.
 */
export function partNameProblem(name: string, plan: BoardPlan, editing?: string): string | null {
  const basic = validatePartName(name);
  if (basic) return `${basic.message} ${basic.hint}`;
  if (name !== editing && name in plan.parts) return t('guided.name.taken', { name });
  return null;
}

/** A free name: `front`, `front_2`, … (the core's rule for parts; boards use it too) */
export function freeName(base: string, taken: string[]): string {
  if (!taken.includes(base)) return base;
  for (let i = 2; ; i++) if (!taken.includes(`${base}_${i}`)) return `${base}_${i}`;
}

/** The short name a new board gets by default: `mega` for a Mega 2560. */
export function boardNameFor(boardType: string): string {
  return boardType.replace(/2560$/, '').replace(/-old$/, '');
}

/** One wire that changes when the plan is saved. */
export interface WireChange {
  part: string;
  role: string;
  /** The pin it is on now; missing for a new wire. */
  from?: string;
  /** The pin it goes to; missing for a wire that goes. */
  to?: string;
}

/**
 * Which wires move between two plans: new parts' wires, parts that go, and pins that move.
 * `roleName` names a role of a template (`trig` → `Trig`).
 */
export function wireChanges(
  before: BoardPlan | null,
  after: BoardPlan,
  roleName: (part: PlannedPart, role: string) => string,
): WireChange[] {
  const out: WireChange[] = [];
  const names = new Set([...Object.keys(before?.parts ?? {}), ...Object.keys(after.parts)]);
  for (const part of [...names].sort()) {
    const was = before?.parts[part];
    const now = after.parts[part];
    const roles = new Set([...Object.keys(was?.pins ?? {}), ...Object.keys(now?.pins ?? {})]);
    for (const role of roles) {
      const from = was?.pins[role];
      const to = now?.pins[role];
      if (from === to) continue;
      out.push({
        part,
        role: roleName((now ?? was)!, role),
        ...(from ? { from } : {}),
        ...(to ? { to } : {}),
      });
    }
  }
  return out;
}

/** A wire change in words: "front Trig: D7 → D8". */
export function wireChangeText(c: WireChange): string {
  if (c.from && c.to)
    return t('guided.wires.move', { part: c.part, role: c.role, from: c.from, to: c.to });
  if (c.to) return t('guided.wires.add', { part: c.part, role: c.role, to: c.to });
  return t('guided.wires.remove', { part: c.part, role: c.role, from: c.from ?? '' });
}

/**
 * The guided board a FaberLink board in Parts runs: the plan whose name the board says, when
 * its sketch is a guided one. Null for a code-mode board.
 */
export function planOfBridge(p: PartInfo, plans: GuidedBoardSummary[]): string | null {
  if (p.connector !== BRIDGE_CONNECTOR || isSubPart(p)) return null;
  const said = p.board?.board;
  const sketch = p.board?.code?.record?.sketchName;
  const plan = plans.find((x) => !x.takenOver && (x.name === sketch || x.name === said));
  return plan?.name ?? null;
}

/** The role's display name in a template's shape (`trig` → `Trig`), or the id itself. */
export function roleNameIn(templates: PartTemplate[], part: PlannedPart, role: string): string {
  const tpl = templates.find((x) => x.id === part.template);
  const shape = tpl?.shapes.find((s) => s.id === part.shape) ?? tpl?.shapes[0];
  return shape?.roles.find((r) => r.id === role)?.name ?? role;
}
