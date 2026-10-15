import { t } from '../i18n';

/**
 * Words and links for a `position@1` reading (a GPS): the Position viewer's pure half. Positions
 * are WGS 84 degrees, as the capability gives them; nothing here needs a map or the internet.
 */

export type Fix = 'none' | 'fix2d' | 'fix3d' | 'dgps' | 'rtk-float' | 'rtk-fixed';

/** Whether the coordinates mean anything: they don't while there is no fix. */
export function hasFix(fix: Fix): boolean {
  return fix !== 'none';
}

const FIX: Record<Fix, () => string> = {
  none: () => t('position.fix.none'),
  fix2d: () => t('position.fix.fix2d'),
  fix3d: () => t('position.fix.fix3d'),
  dgps: () => t('position.fix.dgps'),
  'rtk-float': () => t('position.fix.rtkFloat'),
  'rtk-fixed': () => t('position.fix.rtkFixed'),
};

export function fixWords(fix: Fix): string {
  return FIX[fix]();
}

/** 6 decimals: about 0.1 m, finer than any hobby GPS. */
const DECIMALS = 6;

/** `55.676098° N`. */
export function latitudeWords(deg: number): string {
  const v = Math.abs(deg).toFixed(DECIMALS);
  return deg < 0 ? t('position.south', { deg: v }) : t('position.north', { deg: v });
}

/** `12.568337° E`. */
export function longitudeWords(deg: number): string {
  const v = Math.abs(deg).toFixed(DECIMALS);
  return deg < 0 ? t('position.west', { deg: v }) : t('position.east', { deg: v });
}

/**
 * The place on OpenStreetMap. Only offered when the computer is online: the shell itself never
 * loads map tiles.
 */
export function mapLink(latitudeDeg: number, longitudeDeg: number): string {
  const lat = latitudeDeg.toFixed(DECIMALS);
  const lon = longitudeDeg.toFixed(DECIMALS);
  return `https://www.openstreetmap.org/?mlat=${lat}&mlon=${lon}#map=17/${lat}/${lon}`;
}
