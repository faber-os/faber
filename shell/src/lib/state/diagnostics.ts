import type { AboutInfo, LogEntry, PartInfo, PortInfo } from './types';

/**
 * Settings → About and diagnostics: the text "Copy diagnostics" puts on the clipboard, for a bug
 * report. It must hold no personal data (the project is public): the data folder and any home
 * folder are replaced, and Bluetooth devices are not named (their names are often a person's).
 */

const LOG_LINES = 200;

export interface DiagnosticsInput {
  about: AboutInfo | null;
  ports: PortInfo[];
  parts: PartInfo[];
  logs: LogEntry[];
  /** How a log entry's time is written (the machine's wall clock). */
  logTime: (e: LogEntry) => string;
}

/** Replaces the data folder and any user's home folder in `text`. */
export function redact(text: string, dataFolder: string | null): string {
  let out = dataFolder ? text.split(dataFolder).join('<data folder>') : text;
  out = out.replace(/[A-Za-z]:\\Users\\[^\\\s"']+/g, '<home>');
  out = out.replace(/\/(?:home|Users)\/[^/\s"']+/g, '<home>');
  return out;
}

/** "1 h 2 min" or "4 min 5 s". */
export function formatUptime(s: number): string {
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h} h ${m} min` : `${m} min ${s % 60} s`;
}

/** One line per entry: "14:05:09.123 INFO core::ports message". */
export function logsText(
  logs: LogEntry[],
  logTime: (e: LogEntry) => string,
  dataFolder: string | null,
): string {
  return logs
    .map((e) =>
      redact(`${logTime(e)} ${e.level.toUpperCase()} ${e.source} ${e.message}`, dataFolder),
    )
    .map((l) => `${l}\n`)
    .join('');
}

export function diagnosticsText(d: DiagnosticsInput): string {
  const a = d.about;
  const lines = [
    a ? `Faber ${a.version} (API ${a.api})` : 'Faber (version unknown)',
    `OS: ${a?.os ?? 'unknown'}`,
    `Time zone: ${a?.timeZone ?? 'unknown'}`,
    `Uptime: ${a ? formatUptime(a.uptimeS) : 'unknown'}`,
    '',
    `Ports (${d.ports.length}):`,
    ...d.ports.map((p) => {
      const label = p.kind === 'bluetooth' ? 'Bluetooth device' : p.label;
      const used = p.claimedBy ? `, used by ${p.claimedBy}` : '';
      return `  ${p.id} (${p.kind}) ${label}${used}`;
    }),
    '',
    `Parts (${d.parts.length}):`,
    ...d.parts.map((p) => {
      const why = p.state.message ? `: ${p.state.message}` : '';
      return `  ${p.name}: ${p.connector} on ${p.port}, ${p.state.kind}${why}`;
    }),
    '',
    `Last ${Math.min(LOG_LINES, d.logs.length)} log lines:`,
  ];
  const text = lines.map((l) => redact(l, a?.dataFolder ?? null)).join('\n');
  return `${text}\n${logsText(d.logs.slice(-LOG_LINES), d.logTime, a?.dataFolder ?? null)}`;
}
