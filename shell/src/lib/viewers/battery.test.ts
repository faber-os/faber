import { describe, expect, it } from 'vitest';
import { chemistryWords, levelWords, percent, voltageShare } from './battery';

describe('battery words', () => {
  it('say what each level means for the user', () => {
    expect(levelWords('ok')).toBe('OK');
    expect(levelWords('low')).toBe('Low: charge it soon');
    expect(levelWords('critical')).toBe('Critical: stop and charge it now');
  });

  it('name the chemistries, and pass an unknown one through', () => {
    expect(chemistryWords('li-ion')).toBe('Li-ion');
    expect(chemistryWords('lead-acid')).toBe('Lead-acid');
    expect(chemistryWords('sodium-ion')).toBe('sodium-ion');
  });

  it('keep percentages and the gauge inside their range', () => {
    expect(percent(0.456)).toBe(46);
    expect(percent(1.2)).toBe(100);
    expect(percent(-1)).toBe(0);
    expect(voltageShare(10.8, 9, 12.6)).toBeCloseTo(0.5);
    expect(voltageShare(13, 9, 12.6)).toBe(1);
    expect(voltageShare(10, 9, 9)).toBe(0);
  });
});
