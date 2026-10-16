import { describe, expect, it } from 'vitest';
import { boardMotor, bridgePart, part } from '../../test/fakeShell';
import {
  mayNeedPauseForUpload,
  boardCheckText,
  isBridge,
  isSubPart,
  linkText,
  localName,
  nameFromBoard,
  removable,
} from './bridges';

describe('bridges', () => {
  it('tells a board, its parts and other parts apart', () => {
    const mega = bridgePart('mega');
    const left = boardMotor('mega', 'left');
    const lidar = part('lidar');
    expect([isBridge(mega), isSubPart(mega)]).toEqual([true, false]);
    expect([isBridge(left), isSubPart(left)]).toEqual([false, true]);
    expect([isBridge(lidar), isSubPart(lidar)]).toEqual([false, false]);
    expect(localName(left)).toBe('left');
    expect(localName(lidar)).toBe('lidar');
  });

  it('only a part the board lacks can be removed on its own', () => {
    expect(removable(bridgePart('mega'))).toBe(true);
    expect(removable(boardMotor('mega', 'left'))).toBe(false);
    expect(removable(boardMotor('mega', 'arm', { signals: [] }))).toBe(true);
  });

  it('names a board as the core names its parts', () => {
    expect(nameFromBoard('sim-mega')).toBe('sim_mega');
    expect(nameFromBoard('SENNEP Mega 2560!')).toBe('sennep_mega_2560');
    expect(nameFromBoard('2nd board')).toBe('');
    expect(nameFromBoard('ø')).toBe('');
    expect(nameFromBoard('a'.repeat(40))).toHaveLength(32);
  });

  it('says where the link stands and what a check found', () => {
    expect(linkText('ready')).toBe('Connected');
    expect(linkText('no-answer')).toBe('No answer from the board');
    expect(boardCheckText({ result: 'nothing' })).toBe('No FaberLink board answered on this port.');
    expect(
      boardCheckText({
        result: 'found',
        board: 'b',
        firmware: 'f',
        firmwareVersion: '1',
        parts: 2,
      }),
    ).toBe('FaberLink board: b (f 1)');
  });

  it('suggests pausing a board for an upload only when its link or state shows trouble', () => {
    expect(mayNeedPauseForUpload(bridgePart('mega'))).toBe(false);
    for (const link of ['no-answer', 'lost', 'incompatible'] as const) {
      expect(mayNeedPauseForUpload(bridgePart('mega', { link })), link).toBe(true);
    }
    const failed = { ...bridgePart('mega'), state: { kind: 'failed' as const } };
    expect(mayNeedPauseForUpload(failed)).toBe(true);
    const paused = { ...bridgePart('mega', { link: 'lost' }), state: { kind: 'paused' as const } };
    expect(mayNeedPauseForUpload(paused)).toBe(false);
    expect(mayNeedPauseForUpload(boardMotor('mega', 'left', { state: { kind: 'failed' } }))).toBe(
      false,
    );
    expect(mayNeedPauseForUpload(part('lidar', { state: { kind: 'failed' } }))).toBe(false);
  });
});
