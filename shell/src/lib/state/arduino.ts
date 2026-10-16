import { t } from '../i18n';
import type { ToolsProgress } from './core.svelte';
import type { BoardCodeInfo, BoardLink, FlashRecord } from './types';

/**
 * What the Arduino editor says about a board's code (docs/arduino-editor.md, "The same-code
 * check"). Never more than the core proved: `same` only comes with a proof.
 */
export type CodeView =
  | { kind: 'same'; text: string; proof: string; record: FlashRecord }
  | { kind: 'different'; text: string; proof: string; record: FlashRecord }
  | { kind: 'needs-check'; text: string; record: FlashRecord }
  | { kind: 'never'; text: string };

/** Seconds, rounded to what a person reads: "about 1 s", "about 12 s". */
export function seconds(ms: number): number {
  return Math.max(1, Math.round(ms / 1000));
}

/** The code on a board, in words. `date` formats an RFC 3339 time for the reader. */
export function codeView(info: BoardCodeInfo, date: (iso: string) => string): CodeView {
  const c = info.code;
  switch (c.result) {
    case 'same':
      return {
        kind: 'same',
        record: c.record,
        text: t('arduino.code.same', {
          sketch: c.record.sketchName,
          date: date(c.record.flashedAt),
        }),
        proof:
          c.proof.method === 'build-id'
            ? t('arduino.code.proof.buildId')
            : t('arduino.code.proof.readBack', { seconds: seconds(c.proof.elapsedMs) }),
      };
    case 'different':
      return {
        kind: 'different',
        record: c.record,
        text: t('arduino.code.different', {
          sketch: c.record.sketchName,
          date: date(c.record.flashedAt),
        }),
        proof:
          c.proof.method === 'build-id'
            ? t('arduino.code.proof.otherBuildId')
            : t('arduino.code.proof.readBackDiffers', { seconds: seconds(c.proof.elapsedMs) }),
      };
    case 'needs-check':
      return {
        kind: 'needs-check',
        record: c.record,
        text: t('arduino.code.needsCheck', {
          sketch: c.record.sketchName,
          date: date(c.record.flashedAt),
          seconds: seconds(c.record.checkMs),
        }),
      };
    case 'never':
      return { kind: 'never', text: t('arduino.code.never') };
  }
}

/** For a board without a serial number: why Faber can only go by its port. */
export function identityCaveat(info: BoardCodeInfo): string | null {
  return info.identity.unique ? null : t('arduino.code.noSerial', { port: info.identity.port });
}

/** A FaberLink board's code, for its line in Parts: `null` until the board has answered. */
export function partCodeText(link: BoardLink | undefined): string | null {
  const code = link?.code;
  if (!code) return null;
  return code.record
    ? t('arduino.part.fromFaber', { sketch: code.record.sketchName })
    : t('arduino.part.unknown');
}

/** Where "Install Arduino tools" is, in words. */
export function toolsStepText(p: ToolsProgress): string {
  switch (p.step) {
    case 'downloading':
      return p.total
        ? t('arduino.tools.step.downloadingOf', {
            done: Math.round((p.done ?? 0) / 1e6),
            total: Math.round(p.total / 1e6),
          })
        : t('arduino.tools.step.downloading');
    case 'verifying':
      return t('arduino.tools.step.verifying');
    case 'unpacking':
      return t('arduino.tools.step.unpacking');
    case 'installing-core':
      return t('arduino.tools.step.core');
    case 'done':
      return t('arduino.tools.step.done');
  }
}
