import { t } from '../i18n';
import type { FlashBoard, FlashStage, PackageInfo, PortInfo } from './types';

/** The ports a board can be on: real serial ports, not recordings. */
export function flashablePorts(ports: PortInfo[]): PortInfo[] {
  return ports.filter((p) => p.kind !== 'replay');
}

/** The board a port's USB ids name, if they name one the flasher knows. */
export function guessedBoard(port: PortInfo | undefined, boards: FlashBoard[]): FlashBoard | null {
  const id = port?.board?.board;
  return (id && boards.find((b) => b.id === id)) || null;
}

/** What a port's USB ids say about the board on it, for the port list. Empty: nothing. */
export function boardGuessText(port: PortInfo, boards: FlashBoard[]): string {
  const known = guessedBoard(port, boards);
  if (known) return known.name;
  const chip = port.board?.usbChip;
  return chip ? t('flasher.port.clone', { chip }) : '';
}

/** Installed packages that carry firmware for `board`. */
export function packagesFor(packages: PackageInfo[], board: string): PackageInfo[] {
  return packages.filter((p) => p.firmware.includes(board));
}

/** The last part of a Files path: `blink.hex` for `home:/Documents/blink.hex`. */
export function fileName(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}

/** How far it is, 0–100, as a whole number. */
export function percent(done: number, total: number): number {
  return total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
}

/** Where the flasher is, in words. */
export function stageText(stage: FlashStage, done: number, total: number): string {
  const p = percent(done, total);
  switch (stage) {
    case 'connecting':
      return t('flasher.stage.connecting');
    case 'writing':
      return t('flasher.stage.writing', { percent: p });
    case 'verifying':
      return t('flasher.stage.verifying', { percent: p });
    case 'done':
      return t('flasher.stage.done');
  }
}

/**
 * One bar for the whole flash: writing is the first half, reading back the second, so it never
 * jumps back to 0 between them.
 */
export function overallPercent(stage: FlashStage, done: number, total: number): number {
  const p = percent(done, total);
  switch (stage) {
    case 'connecting':
      return 0;
    case 'writing':
      return Math.round(p / 2);
    case 'verifying':
      return 50 + Math.round(p / 2);
    case 'done':
      return 100;
  }
}
