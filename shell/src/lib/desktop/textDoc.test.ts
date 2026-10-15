import { describe, expect, it } from 'vitest';
import {
  MAX_TEXT_BYTES,
  decodeTextFile,
  edit,
  encodeText,
  findMatches,
  isDirty,
  lineColumn,
  markSaved,
  matchAt,
  newDoc,
  openedDoc,
  stepMatch,
} from './textDoc';

const enc = (s: string) => new TextEncoder().encode(s);

describe('decodeTextFile', () => {
  it('reads UTF-8 text and remembers CRLF line endings', () => {
    const r = decodeTextFile(enc('a\r\nb\r\nø'));
    expect(r).toEqual({ ok: true, text: 'a\nb\nø', eol: '\r\n' });
    expect(decodeTextFile(enc('a\nb'))).toEqual({ ok: true, text: 'a\nb', eol: '\n' });
  });

  it('refuses binary files, saying so', () => {
    const r = decodeTextFile(new Uint8Array([0x46, 0x41, 0x00, 0x01]));
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.reason).toBe('binary');
      expect(r.message).toContain("isn't text");
    }
    expect(decodeTextFile(new Uint8Array([0xff, 0xfe, 0xfd])).ok).toBe(false);
  });

  it('refuses files over 16 MB', () => {
    expect(MAX_TEXT_BYTES).toBe(16 * 1024 * 1024);
    const r = decodeTextFile({ length: MAX_TEXT_BYTES + 1 } as Uint8Array);
    expect(r).toMatchObject({ ok: false, reason: 'too-large' });
  });

  it('writes back with the line endings the file had', () => {
    expect([...encodeText('a\nb', '\r\n')]).toEqual([...enc('a\r\nb')]);
    expect([...encodeText('a\nb', '\n')]).toEqual([...enc('a\nb')]);
  });
});

describe('document state', () => {
  it('is dirty after an edit and clean again after saving', () => {
    let d = openedDoc('home:/Documents/a.txt', 'hello', '\n');
    expect(d.name).toBe('a.txt');
    expect(isDirty(d)).toBe(false);
    d = edit(d, 'hello!');
    expect(isDirty(d)).toBe(true);
    d = edit(d, 'hello');
    expect(isDirty(d)).toBe(false);
    d = markSaved(edit(d, 'bye'));
    expect(isDirty(d)).toBe(false);
    expect(d.savedText).toBe('bye');
  });

  it('starts untitled, and takes the path and name it was saved as', () => {
    let d = newDoc(2);
    expect(d).toMatchObject({ path: null, name: 'Untitled 2', text: '' });
    d = markSaved(edit(d, 'x'), 'home:/Documents/x.txt');
    expect(d).toMatchObject({ path: 'home:/Documents/x.txt', name: 'x.txt' });
  });
});

describe('find', () => {
  const text = 'Faber is a maker. FABER makes. faber';

  it('finds every match, ignoring case unless asked', () => {
    expect(findMatches(text, 'faber')).toEqual([
      { start: 0, end: 5 },
      { start: 18, end: 23 },
      { start: 31, end: 36 },
    ]);
    expect(findMatches(text, 'faber', true)).toEqual([{ start: 31, end: 36 }]);
    expect(findMatches(text, '')).toEqual([]);
  });

  it('steps to the next and previous match from the cursor, wrapping around', () => {
    const m = findMatches(text, 'faber');
    expect(stepMatch(m, 1, 'next')).toBe(1);
    expect(stepMatch(m, 31, 'next')).toBe(2);
    expect(stepMatch(m, 36, 'next')).toBe(0);
    expect(stepMatch(m, 18, 'prev')).toBe(0);
    expect(stepMatch(m, 0, 'prev')).toBe(2);
    expect(stepMatch([], 0, 'next')).toBe(-1);
  });

  it('knows which match the selection is on', () => {
    const m = findMatches(text, 'faber');
    expect(matchAt(m, 18, 23)).toBe(1);
    expect(matchAt(m, 18, 20)).toBe(-1);
  });
});

describe('lineColumn', () => {
  it('is 1-based, counting characters', () => {
    expect(lineColumn('ab\ncd\n', 0)).toEqual({ line: 1, column: 1 });
    expect(lineColumn('ab\ncd\n', 4)).toEqual({ line: 2, column: 2 });
    expect(lineColumn('ab\ncd\n', 6)).toEqual({ line: 3, column: 1 });
  });
});
