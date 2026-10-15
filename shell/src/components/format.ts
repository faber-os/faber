/** 14:05:09.123 in local time, for log lines. */
export function formatLogTime(epochMs: number): string {
  const d = new Date(epochMs);
  const p = (n: number, w = 2) => String(n).padStart(w, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
}

/** Core-clock seconds, used before the core's clock is known: "t+12.345 s". */
export function formatCoreTime(tUs: number): string {
  return `t+${(tUs / 1e6).toFixed(3)} s`;
}

export function formatRate(hz: number | null): string {
  return hz === null ? '–' : `${hz.toFixed(1)} Hz`;
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / 1024 / 1024).toFixed(1)} MiB`;
}
