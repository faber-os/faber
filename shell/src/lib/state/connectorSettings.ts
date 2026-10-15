import { t } from '../i18n';
import type { ConnectorInfo, SettingInfo } from './types';

/**
 * A connector's settings, for the Add part form. The core describes them (`ConnectorInfo.settings`:
 * kind, unit, default, limits, a label and a description, in English for now), so the form is
 * built from that and any connector with settings gets one. The core checks every value again
 * (`400 invalid-settings`). Values are SI, as in parts.toml.
 */

export function settingFields(connector: ConnectorInfo | null): readonly SettingInfo[] {
  return connector?.settings ?? [];
}

/** What the form holds for each field: text for numbers (as typed), names for a part list. */
export type SettingValue = string | boolean | string[];
export type SettingValues = Record<string, SettingValue>;

/** Where a new part's form starts: the suggested value where there is one, else the default. */
export function startValue(f: SettingInfo): SettingValue {
  switch (f.kind) {
    case 'number':
      return String(f.suggested ?? f.default);
    case 'part-list':
      return [];
    default:
      return f.default;
  }
}

export function defaultValues(fields: readonly SettingInfo[]): SettingValues {
  return Object.fromEntries(fields.map((f) => [f.key, startValue(f)]));
}

export type ReadResult =
  { ok: true; settings: Record<string, unknown> } | { ok: false; key: string; message: string };

/**
 * The settings to send, or the first field that is wrong with what to do about it. Numbers are
 * checked against their limits here (a typo shouldn't need a round trip); the core checks the
 * rest. Advanced settings are only written when they differ from the default, so parts.toml
 * stays short.
 */
export function readSettings(fields: readonly SettingInfo[], values: SettingValues): ReadResult {
  const settings: Record<string, unknown> = {};
  for (const f of fields) {
    const v = values[f.key] ?? startValue(f);
    if (f.kind === 'part-list') {
      if (Array.isArray(v) && v.length > 0) settings[f.key] = v;
      continue;
    }
    if (f.kind !== 'number') {
      if (!f.advanced || v !== f.default) settings[f.key] = v;
      continue;
    }
    const n = typeof v === 'string' && v.trim() !== '' ? Number(v) : NaN;
    const problem = numberProblem(f, n);
    if (problem) return { ok: false, key: f.key, message: problem };
    if (!f.advanced || n !== f.default) settings[f.key] = n;
  }
  return { ok: true, settings };
}

function numberProblem(f: SettingInfo & { kind: 'number' }, n: number): string | null {
  const label = f.label;
  if (!Number.isFinite(n)) return t('addPart.settings.notANumber', { label });
  if (f.min !== undefined) {
    const min = String(f.min);
    if (f.minExclusive && n <= f.min) return t('addPart.settings.mustBeAbove', { label, min });
    if (!f.minExclusive && n < f.min) return t('addPart.settings.mustBeAtLeast', { label, min });
  }
  if (f.max !== undefined && n > f.max)
    return t('addPart.settings.mustBeAtMost', { label, max: String(f.max) });
  return null;
}
