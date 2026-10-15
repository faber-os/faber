import { describe, expect, it } from 'vitest';
import { afterConflict, uploadSummary } from './uploads';

describe('upload conflicts', () => {
  it('replaces under the same name, keeps both under a free name, or skips', () => {
    const names = ['a.txt', 'a 2.txt'];
    expect(afterConflict('replace', 'a.txt', names)).toEqual({ name: 'a.txt', mode: 'replace' });
    expect(afterConflict('keep-both', 'a.txt', names)).toEqual({ name: 'a 3.txt', mode: 'create' });
    expect(afterConflict('skip', 'a.txt', names)).toBeNull();
  });

  it('says what happened, and nothing when nothing special did', () => {
    expect(uploadSummary({ renamed: [], replaced: [], skipped: [] })).toBeNull();
    expect(
      uploadSummary({
        renamed: [{ from: 'a.txt', to: 'a 2.txt' }],
        replaced: ['b.txt'],
        skipped: ['c.txt', 'd.txt'],
      }),
    ).toBe('Kept both: a.txt → a 2.txt. Replaced b.txt. Skipped c.txt, d.txt.');
  });
});
