import { t } from '../i18n';
import { baseName, childPath, extension, parentPath } from './filePaths';

/**
 * The Arduino editor's plain logic (docs/arduino-editor.md): what a sketch is, the order of its
 * tabs, and how sizes and compiler messages read. A sketch is a folder `<name>/` holding
 * `<name>.ino`, as the Arduino IDE keeps it; its other code files are more tabs.
 */

/** The extensions of a sketch's tabs, as the core counts them (core/src/arduino/sketch.rs). */
export const SKETCH_EXTENSIONS = ['ino', 'pde', 'h', 'hh', 'hpp', 'c', 'cc', 'cpp', 'cxx', 'S'];

/** Where new sketches go. */
export const SKETCHBOOK = 'home:/Arduino';

export function isSketchFile(name: string): boolean {
  const dot = name.lastIndexOf('.');
  return dot > 0 && SKETCH_EXTENSIONS.includes(name.slice(dot + 1));
}

export interface SketchPaths {
  /** `home:/Arduino/Blink` */
  folder: string;
  /** `Blink` */
  name: string;
  /** `home:/Arduino/Blink/Blink.ino` */
  main: string;
}

/** The sketch a file belongs to: its folder, named like the folder. `null` for a non-code file. */
export function sketchOf(path: string): SketchPaths | null {
  if (!isSketchFile(baseName(path))) return null;
  const folder = parentPath(path);
  if (!folder) return null;
  const name = baseName(folder);
  if (!name) return null;
  return { folder, name, main: childPath(folder, `${name}.ino`) };
}

/** True when `path` is the `.ino` the sketch is named after. */
export function isMainFile(path: string): boolean {
  const s = sketchOf(path);
  return !!s && s.main === path;
}

const KIND_ORDER: Record<string, number> = { ino: 0, pde: 0, h: 1, hh: 1, hpp: 1 };

/** The tabs as the Arduino IDE shows them: the main `.ino` first, other `.ino`s, headers, then the rest, each by name. */
export function tabOrder(names: readonly string[], main: string): string[] {
  const rank = (n: string) => (n === main ? -1 : (KIND_ORDER[extension(n)] ?? 2));
  return [...names].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
}

/** Arduino's rule for a sketch's name, as the core checks it: the reason it is wrong, or `null`. */
export function sketchNameProblem(name: string): string | null {
  if (name === '') return t('arduino.name.empty');
  if (name.length > 63) return t('arduino.name.long');
  if (!/^[0-9A-Za-z]/.test(name)) return t('arduino.name.start');
  const bad = [...name].find((c) => !/[0-9A-Za-z_.-]/.test(c));
  if (bad !== undefined) return t('arduino.name.character', { char: bad === ' ' ? '␣' : bad });
  return null;
}

/** 6896 → "6 896", with a space that never breaks a line. */
export function groupDigits(n: number): string {
  return Math.round(n)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
}

export interface SizeLike {
  flash: number;
  flashMax: number;
  ram: number;
  ramMax: number;
}

function percentOf(used: number, max: number): number {
  return max > 0 ? Math.round((used / max) * 100) : 0;
}

/** "Program: 6 896 bytes of 32 256 (21 %)", and the same for RAM when it is known. */
export function sizeLines(s: SizeLike): string[] {
  const out = [
    t('arduino.size.flash', {
      used: groupDigits(s.flash),
      max: groupDigits(s.flashMax),
      percent: percentOf(s.flash, s.flashMax),
    }),
  ];
  if (s.ramMax > 0) {
    out.push(
      t('arduino.size.ram', {
        used: groupDigits(s.ram),
        max: groupDigits(s.ramMax),
        percent: percentOf(s.ram, s.ramMax),
      }),
    );
  }
  return out;
}

export interface DiagnosticLike {
  file?: string;
  line?: number;
  column?: number;
}

/** Where a compiler message points: `motor.h:3:2`, `Blink.ino:5`, or nothing. */
export function diagnosticPlace(d: DiagnosticLike): string {
  if (!d.file) return '';
  if (d.line === undefined) return d.file;
  return d.column === undefined ? `${d.file}:${d.line}` : `${d.file}:${d.line}:${d.column}`;
}

/** The character offset of a 1-based line and column in `text`, for jumping to an error. */
export function offsetOf(text: string, line: number, column = 1): number {
  let at = 0;
  for (let l = 1; l < line; l++) {
    const next = text.indexOf('\n', at);
    if (next < 0) return text.length;
    at = next + 1;
  }
  const end = text.indexOf('\n', at);
  const lineEnd = end < 0 ? text.length : end;
  return Math.min(at + Math.max(0, column - 1), lineEnd);
}
