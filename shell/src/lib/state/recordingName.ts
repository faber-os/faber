import { t } from '../i18n';
import { MAX_PART_NAME, validatePartName } from './partName';

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

/** Recording names follow the part-name rule (docs/api.md, "Recordings"), in recording words. */
export function validateRecordingName(name: string): { message: string; hint: string } | null {
  if (!validatePartName(name)) return null;
  const hint = t('serial.recordingName.hint');
  if (name === '') return { message: t('serial.recordingName.required'), hint };
  if (name.length > MAX_PART_NAME) {
    return { message: t('serial.recordingName.tooLong', { name, max: MAX_PART_NAME }), hint };
  }
  return { message: t('serial.recordingName.invalid', { name }), hint };
}
