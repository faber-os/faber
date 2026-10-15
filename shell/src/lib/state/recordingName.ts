import { validatePartName } from './partName';

/** `recording_2026_10_15_1030`: today's date and time, in the part-name rule the core uses. */
export function defaultRecordingName(now: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `recording_${now.getFullYear()}_${p(now.getMonth() + 1)}_${p(now.getDate())}_${p(now.getHours())}${p(now.getMinutes())}`;
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
