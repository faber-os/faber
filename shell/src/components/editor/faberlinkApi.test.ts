/// <reference types="node" />
// Node's types: this test reads the FaberLink library's header from disk.
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { FABERLINK_CLASSES } from './faberlinkApi';

// Tests run in shell/.
const HEADER = readFileSync('../firmware/faberlink/src/FaberLink.h', 'utf8');

/**
 * The public classes of a header and the public functions each declares (constructors under the
 * class's own name): top-level classes only, so a class nested in a private section doesn't count.
 */
export function publicApi(header: string): Map<string, Set<string>> {
  const code = header.replace(/\/\/[^\n]*/g, '').replace(/\/\*[\s\S]*?\*\//g, '');
  const out = new Map<string, Set<string>>();
  for (const m of code.matchAll(/^class\s+(\w+)[^{;]*\{/gm)) {
    const name = m[1]!;
    const members = new Set<string>();
    let depth = 1;
    let open = false;
    let statement = '';
    for (let i = m.index + m[0].length; i < code.length && depth > 0; i++) {
      const ch = code[i]!;
      if (ch === '{') {
        // A body: the statement ends here (an inline method); skip to its end.
        if (depth === 1) declare(statement);
        statement = '';
        depth++;
        continue;
      }
      if (ch === '}') {
        depth--;
        continue;
      }
      if (depth !== 1) continue;
      if (ch === ';') {
        declare(statement);
        statement = '';
        continue;
      }
      statement += ch;
      const access = /^\s*(public|protected|private)\s*:$/.exec(statement);
      if (access) {
        open = access[1] === 'public';
        statement = '';
      }
    }
    out.set(name, members);

    function declare(s: string) {
      const text = s.trim();
      if (!open || !text || /^(typedef|using|friend|class|struct|enum)\b/.test(text)) return;
      const fn = /([A-Za-z_]\w*)\s*\(/.exec(text);
      if (fn) members.add(fn[1]!);
    }
  }
  return out;
}

describe('the FaberLink catalogue', () => {
  const header = publicApi(HEADER);

  it('reads the header it guards', () => {
    expect([...header.keys()]).toContain('FaberLink');
    expect(header.get('FaberLink')).toContain('poll');
    expect(header.get('FaberUltrasonic')).toContain('FaberUltrasonic');
    // `measure()` is protected, StreamOutput is private.
    expect(header.get('FaberUltrasonic')).not.toContain('measure');
    expect(header.has('StreamOutput')).toBe(false);
  });

  it('knows exactly the public classes and methods FaberLink.h declares', () => {
    const catalogue = new Map(
      FABERLINK_CLASSES.map((c) => [
        c.name,
        new Set([
          ...(c.constructors.length > 0 ? [c.name] : []),
          ...c.members.filter((m) => !m.inherited).map((m) => m.name),
        ]),
      ]),
    );
    const sorted = (m: Map<string, Set<string>>) =>
      Object.fromEntries([...m].map(([k, v]) => [k, [...v].sort()]).sort());
    // If this fails, FaberLink.h changed: add, change or remove the entry in faberlinkApi.ts.
    expect(sorted(catalogue)).toEqual(sorted(header));
  });

  it('has a constructor signature for each class, with its parameters', () => {
    for (const c of FABERLINK_CLASSES) {
      expect(c.constructors.length, c.name).toBeGreaterThan(0);
      for (const ctor of c.constructors)
        expect(ctor.signature).toMatch(new RegExp(`^${c.name}\\(`));
    }
  });
});
