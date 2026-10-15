import { describe, expect, it } from 'vitest';
import {
  formatClockTime,
  formatDateTime,
  formatLogTime,
  formatLongDate,
  formatShortDate,
  isKnownTimeZone,
  timeZones,
  wallParts,
  type WallClock,
} from './wallClock';

// 12:30:09.042 UTC on 15 October 2026: 14:30 in Copenhagen, 08:30 in New York.
const T = Date.UTC(2026, 9, 15, 12, 30, 9, 42);
const CPH: WallClock = { timeZone: 'Europe/Copenhagen', clock: '24h' };
const NY12: WallClock = { timeZone: 'America/New_York', clock: '12h' };

describe('wall clock', () => {
  it("shows the time in the machine's zone, not the browser's", () => {
    expect(formatClockTime(T, CPH, 'en-GB')).toBe('14:30');
    expect(formatClockTime(T, { timeZone: 'UTC', clock: '24h' }, 'en-GB')).toBe('12:30');
  });

  it('follows the 12/24-hour setting', () => {
    expect(formatClockTime(T, NY12, 'en-US')).toBe('8:30 AM');
    expect(formatClockTime(T, { ...NY12, clock: '24h' }, 'en-US')).toBe('08:30');
  });

  it('formats log times with milliseconds', () => {
    expect(formatLogTime(T, CPH)).toBe('14:30:09.042');
    expect(formatLogTime(T, NY12)).toBe('8:30:09.042 am');
  });

  it('formats file dates, or nothing when unknown', () => {
    expect(formatDateTime(T, CPH, 'en-GB')).toBe('15 Oct 2026, 14:30');
    expect(formatDateTime(undefined, CPH, 'en-GB')).toBe('');
  });

  it('formats the dates the taskbar clock shows', () => {
    // Just after midnight in Copenhagen is still the day before in UTC.
    const late = Date.UTC(2026, 9, 15, 22, 30);
    expect(formatShortDate(late, CPH, 'en-GB')).toBe('16 Oct');
    expect(formatLongDate(late, CPH, 'en-GB')).toBe('Friday 16 October 2026');
  });

  it('gives the parts of a moment in the zone', () => {
    expect(wallParts(T, CPH)).toEqual({ year: 2026, month: 10, day: 15, hour: 14, minute: 30 });
    expect(wallParts(T, NY12)).toEqual({ year: 2026, month: 10, day: 15, hour: 8, minute: 30 });
  });

  it("uses the browser's zone when the zone is missing or unknown", () => {
    const browser = formatClockTime(T, { clock: '24h' }, 'en-GB');
    expect(formatClockTime(T, { timeZone: 'Not/AZone', clock: '24h' }, 'en-GB')).toBe(browser);
    expect(isKnownTimeZone('Europe/Copenhagen')).toBe(true);
    expect(isKnownTimeZone('Not/AZone')).toBe(false);
  });

  it('lists the time zones to pick from', () => {
    const zones = timeZones();
    expect(zones).toContain('Europe/Copenhagen');
    expect(zones).toContain('America/New_York');
  });
});
