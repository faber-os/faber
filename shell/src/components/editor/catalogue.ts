/**
 * The shape of the editor's API catalogue (arduinoApi.ts, faberlinkApi.ts): what completion,
 * hover docs and signature help know about the Arduino core, its common libraries and FaberLink.
 *
 * Entries are written as C++ signatures (`void pinMode(uint8_t pin, uint8_t mode)`) and parsed
 * here, so the data reads like the code it describes. Descriptions are data, not UI text: they
 * stay in English, like the code they explain (docs/arduino-editor.md).
 */

import type { MessageKey } from '../../lib/i18n';

export type Library = 'arduino' | 'faberlink';

/** The catalogue keys (en.ts) of the snippets' descriptions. */
export type SnippetKey = Extract<MessageKey, `arduino.assist.snippet.${string}`>;

/** One way to call a function: `void tone(uint8_t pin, unsigned int frequency)`. */
export interface Overload {
  returns: string;
  /** As written, e.g. `uint8_t pin`; the name is the last word. */
  params: string[];
  /** The whole line, for the info panel and hover. */
  signature: string;
}

export interface ApiFunction {
  name: string;
  overloads: Overload[];
  doc: string;
  library: Library;
  /** A function's own `#include`, when the sketch needs one (`Servo.h`). */
  include?: string;
}

export interface ApiConstant {
  name: string;
  doc: string;
  library: Library;
  /** `faberlink` for `faberlink::DISTANCE_INFRARED`. */
  namespace?: string;
}

export interface ApiClass {
  name: string;
  doc: string;
  library: Library;
  include?: string;
  /** Its members include those of this class. */
  extends?: string;
  constructors: Overload[];
  members: ApiFunction[];
}

/** A ready-made object: `Serial` is a `HardwareSerial`. */
export interface ApiObject {
  name: string;
  type: string;
  doc: string;
  library: Library;
  include?: string;
}

/** A type name with no members worth listing (`byte`, `uint8_t`). */
export interface ApiType {
  name: string;
  doc: string;
}

export interface Snippet {
  label: string;
  /** CodeMirror snippet syntax: `${name}` is a tab stop. */
  template: string;
  /** Its catalogue key (en.ts), for the line in the completion list. */
  key: SnippetKey;
}

/** Splits a parameter list at its top-level commas. */
function splitParams(list: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < list.length; i++) {
    const c = list[i];
    if (c === '(' || c === '<' || c === '[') depth++;
    else if (c === ')' || c === '>' || c === ']') depth--;
    else if (c === ',' && depth === 0) {
      out.push(list.slice(start, i).trim());
      start = i + 1;
    }
  }
  const last = list.slice(start).trim();
  if (last) out.push(last);
  return out;
}

const SIGNATURE = /^(.*?)\b([A-Za-z_]\w*)\s*\((.*)\)\s*(const)?$/;

/** `void pinMode(uint8_t pin, uint8_t mode)` → its return type, name and parameters. */
export function parseSignature(signature: string): { name: string; overload: Overload } {
  const m = SIGNATURE.exec(signature.trim());
  if (!m) throw new Error(`Not a signature: ${signature}`);
  return {
    name: m[2]!,
    overload: { returns: m[1]!.trim(), params: splitParams(m[3]!), signature: signature.trim() },
  };
}

/** The name a parameter is written with: `uint8_t pin` → `pin`, `const char* text` → `text`. */
export function paramName(param: string): string {
  const withoutDefault = param.split('=')[0]!.trim();
  // A function pointer: `void (*isr)()` → `isr`.
  const pointer = /\(\s*\*\s*([A-Za-z_]\w*)\s*\)/.exec(withoutDefault);
  if (pointer) return pointer[1]!;
  return /([A-Za-z_]\w*)(\s*\[\])?$/.exec(withoutDefault)?.[1] ?? withoutDefault;
}

/** A function from one or more signatures of the same name. */
export function fn(
  signatures: string | string[],
  doc: string,
  library: Library = 'arduino',
  include?: string,
): ApiFunction {
  const list = (Array.isArray(signatures) ? signatures : [signatures]).map(parseSignature);
  const name = list[0]!.name;
  for (const s of list) {
    if (s.name !== name) throw new Error(`Overloads of ${name} named ${s.name}`);
  }
  return { name, overloads: list.map((s) => s.overload), doc, library, include };
}
