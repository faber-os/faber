import { describe, expect, it } from 'vitest';
import { richParts } from './rich';

describe('richParts', () => {
  it('splits bold and code out of a sentence', () => {
    expect(richParts('There is <b>no Trash</b> here. Type <code>machine</code>.')).toEqual([
      { text: 'There is ', tag: null },
      { text: 'no Trash', tag: 'b' },
      { text: ' here. Type ', tag: null },
      { text: 'machine', tag: 'code' },
      { text: '.', tag: null },
    ]);
  });

  it('keeps anything else as plain text', () => {
    expect(richParts('<img src=x onerror=alert(1)> <b>unclosed')).toEqual([
      { text: '<img src=x onerror=alert(1)> <b>unclosed', tag: null },
    ]);
    expect(richParts('')).toEqual([]);
  });
});
