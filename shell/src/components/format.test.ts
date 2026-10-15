import { describe, expect, it } from 'vitest';
import { formatBytes, formatCoreTime, formatRate } from './format';

// Wall-clock times (log times, file dates) are formatted in lib/desktop/wallClock.ts.
describe('format', () => {
  it('formats core time in seconds', () => {
    expect(formatCoreTime(12_345_678)).toBe('t+12.346 s');
  });

  it('formats a rate, or a dash when unknown', () => {
    expect(formatRate(10)).toBe('10.0 Hz');
    expect(formatRate(null)).toBe('–');
  });

  it('formats byte counts', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2.0 KiB');
    expect(formatBytes(3 * 1024 * 1024)).toBe('3.0 MiB');
  });

  it('formats bigger sizes in GiB', () => {
    expect(formatBytes(5 * 1024 ** 3)).toBe('5.0 GiB');
    expect(formatBytes(1.5 * 1024 ** 4)).toBe('1.5 TiB');
  });
});
