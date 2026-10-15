import { describe, expect, it } from 'vitest';
import { formatBytes, formatCoreTime, formatLogTime, formatModified, formatRate } from './format';

describe('format', () => {
  it('formats log times with milliseconds', () => {
    const t = new Date(2026, 9, 15, 9, 5, 7, 42).getTime();
    expect(formatLogTime(t)).toBe('09:05:07.042');
  });

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

  it('formats a file date, or nothing when unknown', () => {
    const ms = new Date(2026, 9, 15, 10, 30).getTime();
    expect(formatModified(ms, 'en-GB')).toBe('15 Oct 2026, 10:30');
    expect(formatModified(undefined)).toBe('');
  });
});
