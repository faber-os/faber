import { describe, expect, it } from 'vitest';
import { formatLength, niceLengthStep } from './units';

describe('lengths', () => {
  it('uses cm below one metre and m above', () => {
    expect(formatLength(0.5, 'metric')).toBe('50 cm');
    expect(formatLength(0.05, 'metric')).toBe('5 cm');
    expect(formatLength(1, 'metric')).toBe('1 m');
    expect(formatLength(2.5, 'metric')).toBe('2.5 m');
    expect(formatLength(0.30000000000000004, 'metric')).toBe('30 cm');
  });

  it('uses inches below one foot and feet above', () => {
    expect(formatLength(0.3048, 'imperial')).toBe('1 ft');
    expect(formatLength(0.1524, 'imperial')).toBe('6 in');
    expect(formatLength(3.048, 'imperial')).toBe('10 ft');
    expect(formatLength(0.4572, 'imperial')).toBe('1.5 ft');
    expect(formatLength(0.0254, 'imperial')).toBe('1 in');
  });

  it('picks round steps in the chosen unit', () => {
    expect(niceLengthStep(0.6, 'metric')).toBe(1);
    expect(niceLengthStep(1.5, 'metric')).toBe(2);
    expect(niceLengthStep(0.06, 'metric')).toBe(0.1);
    // Imperial steps are whole inches or feet: 6 in, 1 ft, 2 ft, 5 ft, 10 ft …
    expect(niceLengthStep(0.1, 'imperial')).toBeCloseTo(0.1524, 6); // 6 in
    expect(niceLengthStep(0.2, 'imperial')).toBeCloseTo(0.3048, 6); // 1 ft
    expect(niceLengthStep(1, 'imperial')).toBeCloseTo(1.524, 6); // 5 ft
    expect(niceLengthStep(20, 'imperial')).toBeCloseTo(30.48, 6); // 100 ft
    expect(formatLength(niceLengthStep(1, 'imperial'), 'imperial')).toBe('5 ft');
  });
});
