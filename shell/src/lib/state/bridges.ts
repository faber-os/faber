import { t } from '../i18n';
import type { BoardCheck, LinkState, PartInfo } from './types';

/** The connector of a FaberLink board (docs/faberlink.md). Its parts appear by themselves. */
export const BRIDGE_CONNECTOR = 'faber-bridge';

/** A part on a bridge's board (a sub-part, `mega.left`). */
export function isSubPart(p: PartInfo): boolean {
  return p.bridge != null;
}

/** A FaberLink board's own part, which hosts sub-parts. */
export function isBridge(p: PartInfo): boolean {
  return p.connector === BRIDGE_CONNECTOR && !isSubPart(p);
}

/** A sub-part's name without its bridge: `left` for `mega.left`. */
export function localName(p: PartInfo): string {
  return p.bridge && p.name.startsWith(`${p.bridge}.`) ? p.name.slice(p.bridge.length + 1) : p.name;
}

/**
 * A sub-part can be removed only while its board doesn't have it (a stale line in parts.toml):
 * then it has no signal. The core refuses the rest and says why.
 */
export function removable(p: PartInfo): boolean {
  return !isSubPart(p) || p.signals.length === 0;
}

/**
 * A part name from a board's own name (`Sennep-Mega` → `sennep_mega`), the same rule as the
 * core's for a board's parts. Empty when nothing usable is left.
 */
export function nameFromBoard(board: string): string {
  const name = board
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 32)
    .replace(/_+$/, '');
  return /^[a-z]/.test(name) ? name : '';
}

/**
 * A board Faber can't talk to properly: often a sketch is being uploaded from the Arduino IDE,
 * which can't open the port while Faber holds it. Faber can't see the IDE, so Parts suggests
 * pausing the board whenever its link or its state says something is wrong.
 */
export function mayNeedPauseForUpload(p: PartInfo): boolean {
  if (!isBridge(p) || p.state.kind === 'paused') return false;
  const link = p.board?.link;
  return (
    link === 'no-answer' ||
    link === 'lost' ||
    link === 'incompatible' ||
    p.state.kind === 'failed' ||
    p.state.kind === 'conflict'
  );
}

/** Where the link to a board stands, in a few words. */
export function linkText(link: LinkState): string {
  switch (link) {
    case 'connecting':
      return t('parts.board.link.connecting');
    case 'describing':
      return t('parts.board.link.describing');
    case 'ready':
      return t('parts.board.link.ready');
    case 'no-answer':
      return t('parts.board.link.noAnswer');
    case 'lost':
      return t('parts.board.link.lost');
    case 'incompatible':
      return t('parts.board.link.incompatible');
  }
}

/** What checking a port found, in a sentence. */
export function boardCheckText(c: BoardCheck): string {
  switch (c.result) {
    case 'found':
      return t('addPart.port.check.found', {
        board: c.board,
        firmware: c.firmware,
        version: c.firmwareVersion,
      });
    case 'other-version':
      return t('addPart.port.check.otherVersion');
    case 'nothing':
      return t('addPart.port.check.nothing');
  }
}
