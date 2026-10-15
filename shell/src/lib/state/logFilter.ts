import type { LogEntry, LogLevel } from './types';

/** Most severe first. */
export const LOG_LEVELS: readonly LogLevel[] = ['error', 'warn', 'info', 'debug'];

/** Entries at `minLevel` or more severe whose source or message contains `text`. */
export function filterLogs(
  entries: readonly LogEntry[],
  minLevel: LogLevel,
  text: string,
): LogEntry[] {
  const max = LOG_LEVELS.indexOf(minLevel);
  const needle = text.trim().toLowerCase();
  return entries.filter(
    (e) =>
      LOG_LEVELS.indexOf(e.level) <= max &&
      (needle === '' ||
        e.source.toLowerCase().includes(needle) ||
        e.message.toLowerCase().includes(needle)),
  );
}
