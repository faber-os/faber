/**
 * The Notes accessory's document, as plain values: what was loaded or last saved, what is in
 * the editor now, and finding text in it.
 */

import { t } from '../i18n';

/** The core refuses bigger files, and a browser text box gets unusable well before that. */
export const MAX_TEXT_BYTES = 16 * 1024 * 1024;

export type Eol = '\n' | '\r\n';

export interface TextDoc {
  /** `null` until it has been saved somewhere. */
  path: string | null;
  name: string;
  /** The text as it is on disk (with `\n` line endings). */
  savedText: string;
  /** The text in the editor. */
  text: string;
  /** Line endings to write back, so a Windows file stays a Windows file. */
  eol: Eol;
}

export type Decoded =
  | { ok: true; text: string; eol: Eol }
  | { ok: false; reason: 'binary' | 'too-large'; message: string; hint: string };

export function decodeTextFile(bytes: Uint8Array): Decoded {
  if (bytes.length > MAX_TEXT_BYTES) {
    return {
      ok: false,
      reason: 'too-large',
      message: t('notes.error.tooBig.message'),
      hint: t('notes.error.tooBig.hint'),
    };
  }
  let text: string;
  try {
    if (bytes.includes(0)) throw new Error('NUL byte');
    text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    return {
      ok: false,
      reason: 'binary',
      message: t('notes.error.notText.message'),
      hint: t('notes.error.notText.hint'),
    };
  }
  const eol: Eol = text.includes('\r\n') ? '\r\n' : '\n';
  return { ok: true, text: text.replace(/\r\n/g, '\n'), eol };
}

export function encodeText(text: string, eol: Eol): Uint8Array {
  return new TextEncoder().encode(eol === '\n' ? text : text.replace(/\n/g, eol));
}

/** The name of new note number `n`: "Untitled", then "Untitled 2", "Untitled 3"… */
export function untitledName(n: number): string {
  return n > 1 ? t('notes.untitledNumbered', { n }) : t('notes.untitled');
}

export function newDoc(n: number): TextDoc {
  return {
    path: null,
    name: untitledName(n),
    savedText: '',
    text: '',
    eol: '\n',
  };
}

export function openedDoc(path: string, text: string, eol: Eol): TextDoc {
  return { path, name: nameOf(path), savedText: text, text, eol };
}

export function edit(doc: TextDoc, text: string): TextDoc {
  return { ...doc, text };
}

export function isDirty(doc: TextDoc): boolean {
  return doc.text !== doc.savedText;
}

/** The text was written; with `path` when it was saved somewhere new (Save as). */
export function markSaved(doc: TextDoc, path?: string): TextDoc {
  const next = { ...doc, savedText: doc.text };
  return path ? { ...next, path, name: nameOf(path) } : next;
}

export interface Match {
  start: number;
  end: number;
}

export function findMatches(text: string, query: string, caseSensitive = false): Match[] {
  if (query === '') return [];
  const hay = caseSensitive ? text : text.toLowerCase();
  const needle = caseSensitive ? query : query.toLowerCase();
  const out: Match[] = [];
  for (let i = hay.indexOf(needle); i !== -1; i = hay.indexOf(needle, i + needle.length)) {
    out.push({ start: i, end: i + needle.length });
  }
  return out;
}

/**
 * The match to go to from cursor position `from`: the first one starting after it ('next') or
 * the last one starting before it ('prev'), wrapping around. -1 when there are none.
 */
export function stepMatch(matches: readonly Match[], from: number, dir: 'next' | 'prev'): number {
  if (matches.length === 0) return -1;
  if (dir === 'next') {
    const i = matches.findIndex((m) => m.start >= from);
    return i === -1 ? 0 : i;
  }
  for (let i = matches.length - 1; i >= 0; i--) if (matches[i]!.start < from) return i;
  return matches.length - 1;
}

/** The index of the match exactly covering the selection, or -1. */
export function matchAt(matches: readonly Match[], start: number, end: number): number {
  return matches.findIndex((m) => m.start === start && m.end === end);
}

export function lineColumn(text: string, offset: number): { line: number; column: number } {
  const before = text.slice(0, offset);
  const line = before.split('\n').length;
  return { line, column: offset - (before.lastIndexOf('\n') + 1) + 1 };
}

function nameOf(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}
