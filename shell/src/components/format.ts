// Wall-clock times (log times, file dates, the clock) live in lib/desktop/wallClock.ts, so they
// can follow the machine's time zone.

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
  if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MiB`;
  if (n < 1024 ** 4) return `${(n / 1024 ** 3).toFixed(1)} GiB`;
  return `${(n / 1024 ** 4).toFixed(1)} TiB`;
}
