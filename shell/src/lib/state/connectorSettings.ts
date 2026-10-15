import { t } from '../i18n';

/**
 * The settings of the generic connectors (docs/simulation.md, section 3), for the Add part form.
 * The API lists connectors but not their settings, so the shell knows these three itself; the
 * core checks every value again (`400 invalid-settings`). Values are SI, as in parts.toml.
 */

export type SettingField = (
  | {
      key: string;
      kind: 'number';
      /** The unit shown next to the field: data, not translated (D43). */
      unit: string;
      default: number;
      /** Must be above 0 (else at least 0). */
      positive: boolean;
    }
  | { key: string; kind: 'boolean'; default: boolean }
  | { key: string; kind: 'choice'; default: string; choices: readonly Choice[] }
) & {
  /** Its label for people (a function: texts follow the language). */
  label: () => string;
};

export interface Choice {
  value: string;
  label: () => string;
}

const CHOICE_LABELS: Record<string, () => string> = {
  dc: () => t('addPart.settings.choice.dc'),
  stepper: () => t('addPart.settings.choice.stepper'),
  bldc: () => t('addPart.settings.choice.bldc'),
  'continuous-servo': () => t('addPart.settings.choice.continuousServo'),
  ultrasonic: () => t('addPart.settings.choice.ultrasonic'),
  'laser-tof': () => t('addPart.settings.choice.laserTof'),
  infrared: () => t('addPart.settings.choice.infrared'),
  radar: () => t('addPart.settings.choice.radar'),
};

function choices(values: string[]): Choice[] {
  return values.map((value) => ({ value, label: CHOICE_LABELS[value] ?? (() => value) }));
}

/**
 * Defaults that make a sensible first machine: SENNEP's footprint (0.8 × 0.65 m) and limits
 * (0.6 m/s, 1.5 rad/s), a DC motor at about 100 rpm, and an ultrasonic sensor's range.
 */
export const GENERIC_SETTINGS: Readonly<Record<string, readonly SettingField[]>> = {
  'generic-planar-base': [
    {
      key: 'length',
      label: () => t('addPart.settings.base.length'),
      kind: 'number',
      unit: 'm',
      default: 0.8,
      positive: true,
    },
    {
      key: 'width',
      label: () => t('addPart.settings.base.width'),
      kind: 'number',
      unit: 'm',
      default: 0.65,
      positive: true,
    },
    {
      key: 'holonomic',
      label: () => t('addPart.settings.base.holonomic'),
      kind: 'boolean',
      default: true,
    },
    {
      key: 'max-vx',
      label: () => t('addPart.settings.base.maxVx'),
      kind: 'number',
      unit: 'm/s',
      default: 0.6,
      positive: true,
    },
    {
      key: 'max-vy',
      label: () => t('addPart.settings.base.maxVy'),
      kind: 'number',
      unit: 'm/s',
      default: 0.6,
      positive: true,
    },
    {
      key: 'max-omega',
      label: () => t('addPart.settings.base.maxOmega'),
      kind: 'number',
      unit: 'rad/s',
      default: 1.5,
      positive: true,
    },
    {
      key: 'max-accel',
      label: () => t('addPart.settings.base.maxAccel'),
      kind: 'number',
      unit: 'm/s²',
      default: 1.0,
      positive: true,
    },
  ],
  'generic-motor': [
    {
      key: 'max-velocity',
      label: () => t('addPart.settings.motor.maxVelocity'),
      kind: 'number',
      unit: 'rad/s',
      default: 10,
      positive: true,
    },
    {
      key: 'kind',
      label: () => t('addPart.settings.motor.kind'),
      kind: 'choice',
      default: 'dc',
      choices: choices(['dc', 'stepper', 'bldc', 'continuous-servo']),
    },
  ],
  'generic-distance': [
    {
      key: 'min-m',
      label: () => t('addPart.settings.distance.minM'),
      kind: 'number',
      unit: 'm',
      default: 0.02,
      positive: false,
    },
    {
      key: 'max-m',
      label: () => t('addPart.settings.distance.maxM'),
      kind: 'number',
      unit: 'm',
      default: 4,
      positive: true,
    },
    {
      key: 'fov-rad',
      label: () => t('addPart.settings.distance.fov'),
      kind: 'number',
      unit: 'rad',
      default: 0.26,
      positive: true,
    },
    {
      key: 'kind',
      label: () => t('addPart.settings.distance.kind'),
      kind: 'choice',
      default: 'ultrasonic',
      choices: choices(['ultrasonic', 'laser-tof', 'infrared', 'radar']),
    },
  ],
};

export function settingFields(connector: string | null): readonly SettingField[] {
  return (connector && GENERIC_SETTINGS[connector]) || [];
}

/** What the form holds for each field: text for numbers (as typed), else the value. */
export type SettingValues = Record<string, string | boolean>;

export function defaultValues(fields: readonly SettingField[]): SettingValues {
  return Object.fromEntries(
    fields.map((f) => [f.key, f.kind === 'number' ? String(f.default) : f.default]),
  );
}

/**
 * The settings to send, or the first field that is wrong with what to do about it. Only numbers
 * are checked here (a typo shouldn't need a round trip); the core checks the rest.
 */
export function readSettings(
  fields: readonly SettingField[],
  values: SettingValues,
): { ok: true; settings: Record<string, unknown> } | { ok: false; key: string; message: string } {
  const settings: Record<string, unknown> = {};
  for (const f of fields) {
    const v = values[f.key] ?? (f.kind === 'number' ? String(f.default) : f.default);
    if (f.kind !== 'number') {
      settings[f.key] = v;
      continue;
    }
    const n = typeof v === 'string' && v.trim() !== '' ? Number(v) : NaN;
    const label = f.label();
    if (!Number.isFinite(n))
      return { ok: false, key: f.key, message: t('addPart.settings.notANumber', { label }) };
    if (f.positive ? n <= 0 : n < 0)
      return {
        ok: false,
        key: f.key,
        message: f.positive
          ? t('addPart.settings.mustBePositive', { label })
          : t('addPart.settings.mustNotBeNegative', { label }),
      };
    settings[f.key] = n;
  }
  return { ok: true, settings };
}
