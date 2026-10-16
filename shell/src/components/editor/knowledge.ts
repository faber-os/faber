/**
 * The catalogue looked up by name: what completion, hover and signature help ask
 * ("what is `pinMode`?", "what can follow `Serial.`?").
 */
import {
  ARDUINO_CLASSES,
  ARDUINO_CONSTANTS,
  ARDUINO_FUNCTIONS,
  ARDUINO_OBJECTS,
  ARDUINO_TYPES,
} from './arduinoApi';
import { FABERLINK_CLASSES, FABERLINK_CONSTANTS } from './faberlinkApi';
import type { ApiClass, ApiConstant, ApiFunction, ApiObject, ApiType } from './catalogue';

const byName = <T extends { name: string }>(list: T[]): Map<string, T> =>
  new Map(list.map((x) => [x.name, x]));

export const FUNCTIONS = byName(ARDUINO_FUNCTIONS);
/** Constants used by their bare name (`HIGH`); FaberLink's are under `faberlink::`. */
export const CONSTANTS = byName(ARDUINO_CONSTANTS);
export const CLASSES = byName<ApiClass>([...ARDUINO_CLASSES, ...FABERLINK_CLASSES]);
export const OBJECTS = byName(ARDUINO_OBJECTS);
export const TYPES = byName(ARDUINO_TYPES);
export const NAMESPACES: Map<string, ApiConstant[]> = new Map([['faberlink', FABERLINK_CONSTANTS]]);
export const FABERLINK_CLASS_NAMES = new Set(FABERLINK_CLASSES.map((c) => c.name));

/** A class's members, its base classes' included (nearest first, no repeated names). */
export function apiMembers(className: string): ApiFunction[] {
  const out: ApiFunction[] = [];
  const seen = new Set<string>();
  for (
    let cls = CLASSES.get(className);
    cls;
    cls = cls.extends ? CLASSES.get(cls.extends) : undefined
  ) {
    for (const m of cls.members) {
      if (seen.has(m.name)) continue;
      seen.add(m.name);
      out.push(m);
    }
  }
  return out;
}

/** What a bare name means in the catalogue, if anything. */
export type ApiThing =
  | { kind: 'function'; entry: ApiFunction }
  | { kind: 'constant'; entry: ApiConstant }
  | { kind: 'class'; entry: ApiClass }
  | { kind: 'object'; entry: ApiObject }
  | { kind: 'type'; entry: ApiType };

export function lookup(name: string): ApiThing | undefined {
  const f = FUNCTIONS.get(name);
  if (f) return { kind: 'function', entry: f };
  const c = CONSTANTS.get(name);
  if (c) return { kind: 'constant', entry: c };
  const cls = CLASSES.get(name);
  if (cls) return { kind: 'class', entry: cls };
  const o = OBJECTS.get(name);
  if (o) return { kind: 'object', entry: o };
  const t = TYPES.get(name);
  if (t) return { kind: 'type', entry: t };
  return undefined;
}
