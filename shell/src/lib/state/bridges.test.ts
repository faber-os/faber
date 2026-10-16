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
  pinsText,
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

  it("says which of the board's pins a part uses, when its board says", () => {
    const sonar = boardMotor('mega', 'sonar', {
      pins: [
        { role: 'trig', pin: 7, label: 'D7' },
        { role: 'echo', pin: 6, label: 'D6' },
      ],
    });
    expect(pinsText(sonar)).toBe('Trig D7 · Echo D6');
    const motor = boardMotor('mega', 'left', {
      pins: [
        { role: 'en', pin: 10, label: 'D10' },
        { role: 'in1', pin: 8, label: 'D8' },
        { role: 'in2', pin: 7, label: 'D7' },
        { role: 'stby', pin: 4, label: 'D4' },
      ],
    });
    expect(pinsText(motor)).toBe('EN D10 · IN1 D8 · IN2 D7 · STBY D4');
    const ir = boardMotor('mega', 'ir', { pins: [{ role: 'signal', pin: 54, label: 'A0' }] });
    expect(pinsText(ir)).toBe('Signal A0');
    // A role from a newer library still shows its pin.
    expect(
      pinsText(boardMotor('mega', 'x', { pins: [{ role: 'other', pin: 3, label: 'D3' }] })),
    ).toBe('Pin D3');
    // An older library says nothing.
    expect(pinsText(boardMotor('mega', 'left'))).toBe('');
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
