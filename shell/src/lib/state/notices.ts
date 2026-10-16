import { t } from '../i18n';
import type { Notice } from './types';

/**
 * Words for a notice from the core (docs/api.md, "Notices"), built here from its data so they
 * can be translated; the core's log entry for it stays in English.
 */
export type NoticeView = {
  message: string;
  hint: string;
  /** How it looks: critical is danger, low a warning, ok again is good news. */
  tone: 'danger' | 'warn' | 'ok';
  /** Good news goes away by itself; a warning stays until it is dismissed. */
  sticky: boolean;
};

/** "sennep.battery" from `sennep.battery/battery`, and "app:monitor" from an app's signal. */
function owner(address: string): string {
  return address.split('/')[0] ?? address;
}

function volts(v: number): string {
  return `${String(Number(v.toFixed(2)))} V`;
}

export function noticeView(n: Notice): NoticeView {
  const part = owner(n.address);
  const v = volts(n.volts);
  switch (n.level) {
    case 'critical':
      return {
        message: t('notice.battery.critical', { part, volts: v }),
        hint: t('notice.battery.critical.hint'),
        tone: 'danger',
        sticky: true,
      };
    case 'low':
      return {
        message: t('notice.battery.low', { part, volts: v }),
        hint: t('notice.battery.low.hint'),
        tone: 'warn',
        sticky: true,
      };
    case 'ok':
      return {
        message: t('notice.battery.ok', { part, volts: v }),
        hint: '',
        tone: 'ok',
        sticky: false,
      };
  }
}
