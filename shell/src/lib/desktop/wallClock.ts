/**
 * Wall-clock times, shown in the machine's time zone (D42): the taskbar clock, file dates, log
 * times and recent files read the same in every browser, so someone looking at a robot from
 * another country sees the robot's own time.
 *
 * `timeZone` is the resolved zone (the setting, else the machine's own). A zone this browser
 * doesn't know falls back to the browser's own instead of failing.
 */

export interface WallClock {
  /** IANA zone, e.g. `Europe/Copenhagen`. Missing: the browser's own. */
  timeZone?: string;
  clock: '24h' | '12h';
}

/** The shell is English for now (D44), so dates read the same in every browser. */
const LOCALE = 'en-GB';

const cache = new Map<string, Intl.DateTimeFormat>();

function formatter(
  wc: WallClock,
  options: Intl.DateTimeFormatOptions,
  locale = LOCALE,
): Intl.DateTimeFormat {
  const zone = wc.timeZone && isKnownTimeZone(wc.timeZone) ? wc.timeZone : undefined;
  const key = JSON.stringify([zone, wc.clock, locale, options]);
  let f = cache.get(key);
  if (!f) {
    const hourCycle: Intl.DateTimeFormatOptions = options.hour
      ? { hourCycle: wc.clock === '12h' ? 'h12' : 'h23' }
      : {};
    f = new Intl.DateTimeFormat(locale, { ...options, ...hourCycle, timeZone: zone });
    cache.set(key, f);
  }
  return f;
}

/** 24-hour times are always two digits (09:05); 12-hour ones aren't (9:05 AM). */
function hourStyle(wc: WallClock): '2-digit' | 'numeric' {
  return wc.clock === '12h' ? 'numeric' : '2-digit';
}

/** True if this browser can show times in `zone`. */
export function isKnownTimeZone(zone: string): boolean {
  try {
    new Intl.DateTimeFormat('en', { timeZone: zone });
    return true;
  } catch {
    return false;
  }
}

/** Every zone this browser knows, for the time-zone picker. */
export function timeZones(): string[] {
  const intl = Intl as typeof Intl & { supportedValuesOf?: (key: string) => string[] };
  try {
    return intl.supportedValuesOf?.('timeZone') ?? [];
  } catch {
    return [];
  }
}

/** "14:30" or "2:30 PM". */
export function formatClockTime(ms: number, wc: WallClock, locale?: string): string {
  return formatter(wc, { hour: hourStyle(wc), minute: '2-digit' }, locale).format(ms);
}

/** "Friday 16 October 2026". */
export function formatLongDate(ms: number, wc: WallClock, locale?: string): string {
  return formatter(wc, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }, locale)
    .format(ms)
    .replace(',', '');
}

/** "16 Oct". */
export function formatShortDate(ms: number, wc: WallClock, locale?: string): string {
  return formatter(wc, { day: 'numeric', month: 'short' }, locale).format(ms);
}

/** "15 Oct 2026, 14:30", for file dates; '' when the core didn't say. */
export function formatDateTime(ms: number | undefined, wc: WallClock, locale?: string): string {
  if (ms === undefined) return '';
  return formatter(
    wc,
    { day: 'numeric', month: 'short', year: 'numeric', hour: hourStyle(wc), minute: '2-digit' },
    locale,
  ).format(ms);
}

/** "14:30:09.042" (or "2:30:09.042 PM"), for log lines. */
export function formatLogTime(ms: number, wc: WallClock): string {
  const parts = formatter(
    wc,
    { hour: hourStyle(wc), minute: '2-digit', second: '2-digit' },
    LOCALE,
  ).formatToParts(ms);
  const get = (type: Intl.DateTimeFormatPartTypes) => parts.find((p) => p.type === type)?.value;
  const millis = String(((ms % 1000) + 1000) % 1000).padStart(3, '0');
  const time = `${get('hour')}:${get('minute')}:${get('second')}.${millis}`;
  return wc.clock === '12h' ? `${time} ${get('dayPeriod')}` : time;
}

export interface WallParts {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
}

/** The calendar date and time of `ms` in the zone (month 1–12, hour 0–23). */
export function wallParts(ms: number, wc: WallClock): WallParts {
  const parts = formatter(
    { timeZone: wc.timeZone, clock: '24h' },
    {
      year: 'numeric',
      month: 'numeric',
      day: 'numeric',
      hour: 'numeric',
      minute: 'numeric',
    },
    'en-US',
  ).formatToParts(ms);
  const n = (type: Intl.DateTimeFormatPartTypes) =>
    Number(parts.find((p) => p.type === type)?.value ?? 0);
  return {
    year: n('year'),
    month: n('month'),
    day: n('day'),
    hour: n('hour'),
    minute: n('minute'),
  };
}
