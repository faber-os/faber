import { t } from '../i18n';

/**
 * Words for a `battery@1` reading: the Battery viewer's pure half. Volts, amps and °C as the
 * capability gives them; electrical units have no display setting.
 */

export type Level = 'ok' | 'low' | 'critical';

const LEVEL: Record<Level, () => string> = {
  ok: () => t('battery.level.ok'),
  low: () => t('battery.level.low'),
  critical: () => t('battery.level.critical'),
};

export function levelWords(level: Level): string {
  return LEVEL[level]();
}

const CHEMISTRY: Record<string, () => string> = {
  'li-ion': () => t('battery.chemistry.li-ion'),
  lipo: () => t('battery.chemistry.lipo'),
  lifepo4: () => t('battery.chemistry.lifepo4'),
  nimh: () => t('battery.chemistry.nimh'),
  'lead-acid': () => t('battery.chemistry.lead-acid'),
};

/** "Li-ion"; an unknown chemistry (from a newer core) as the core spells it. */
export function chemistryWords(chemistry: string): string {
  return CHEMISTRY[chemistry]?.() ?? chemistry;
}

/** 0–1 as a whole percentage, kept inside 0–100. */
export function percent(charge: number): number {
  return Math.round(Math.min(1, Math.max(0, charge)) * 100);
}

/**
 * Where the voltage sits between empty (0) and full (1), for the gauge when the battery reports
 * no charge of its own. Only a rough guide: a battery under load reads lower.
 */
export function voltageShare(volts: number, empty: number, full: number): number {
  if (!(full > empty)) return 0;
  return Math.min(1, Math.max(0, (volts - empty) / (full - empty)));
}
