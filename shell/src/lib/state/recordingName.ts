import { validatePartName } from './partName';

/**
 * `recording_2026_10_15_1030`: the date and time on the machine's clock (month 1–12), in the
 * part-name rule the core uses.
 */
export function defaultRecordingName(now: {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
}): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `recording_${now.year}_${p(now.month)}_${p(now.day)}_${p(now.hour)}${p(now.minute)}`;
}

/** Recording names follow the part-name rule (docs/api.md, "Recordings"). */
export function validateRecordingName(name: string): { message: string; hint: string } | null {
  const e = validatePartName(name);
  if (!e) return null;
  return {
    message: e.message
      .replace('The part needs a name.', 'The recording needs a name.')
      .replace('valid part name', 'valid recording name'),
    hint: e.hint.replace('front_lidar', 'bench_test'),
  };
}
