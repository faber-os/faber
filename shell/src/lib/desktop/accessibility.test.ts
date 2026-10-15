import { describe, expect, it } from 'vitest';
import { TEXT_SIZES, prefers, textScale } from './accessibility';

describe('accessibility', () => {
  it('follows the screen only when set to System', () => {
    expect(prefers('system', true)).toBe(true);
    expect(prefers('system', false)).toBe(false);
    expect(prefers('on', false)).toBe(true);
    expect(prefers('off', true)).toBe(false);
  });

  it('scales text by the chosen percentage', () => {
    expect(TEXT_SIZES).toEqual(['100', '115', '130', '150']);
    expect(textScale('100')).toBe(1);
    expect(textScale('130')).toBe(1.3);
    expect(textScale('999' as never)).toBe(1);
  });
});
