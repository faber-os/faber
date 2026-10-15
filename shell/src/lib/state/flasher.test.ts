import { describe, expect, it } from 'vitest';
import {
  boardGuessText,
  fileName,
  flashablePorts,
  guessedBoard,
  overallPercent,
  packagesFor,
  stageText,
} from './flasher';
import type { FlashBoard, PackageInfo, PortInfo } from './types';

const BOARDS: FlashBoard[] = [
  { id: 'uno', name: 'Arduino Uno', chip: 'ATmega328P', room: 32256 },
  { id: 'mega2560', name: 'Arduino Mega 2560', chip: 'ATmega2560', room: 253952 },
];

function port(id: string, extra: Partial<PortInfo> = {}): PortInfo {
  return { id, kind: 'usb-serial', label: id, suggestions: [], ...extra };
}

describe('the flasher helpers', () => {
  it('guesses the board from what the core says about the USB ids', () => {
    expect(guessedBoard(port('a', { board: { board: 'uno' } }), BOARDS)?.name).toBe('Arduino Uno');
    expect(guessedBoard(port('b', { board: { usbChip: 'CH340' } }), BOARDS)).toBeNull();
    expect(guessedBoard(port('c'), BOARDS)).toBeNull();
    expect(guessedBoard(undefined, BOARDS)).toBeNull();
    expect(boardGuessText(port('a', { board: { board: 'mega2560' } }), BOARDS)).toBe(
      'Arduino Mega 2560',
    );
    expect(boardGuessText(port('b', { board: { usbChip: 'CH340' } }), BOARDS)).toBe(
      'CH340 USB chip: pick the board',
    );
    expect(boardGuessText(port('c'), BOARDS)).toBe('');
  });

  it('leaves recordings out: no board is on them', () => {
    const ports = [port('usb:1'), port('replay:x', { kind: 'replay' })];
    expect(flashablePorts(ports).map((p) => p.id)).toEqual(['usb:1']);
  });

  it('offers only packages with firmware for the board', () => {
    const pkg = (id: string, firmware: string[]) => ({ id, firmware }) as PackageInfo;
    const list = [pkg('a', ['uno']), pkg('b', ['mega2560']), pkg('c', [])];
    expect(packagesFor(list, 'uno').map((p) => p.id)).toEqual(['a']);
  });

  it('says where it is, with one bar that only moves forward', () => {
    expect(stageText('connecting', 0, 512)).toBe('Waiting for the bootloader…');
    expect(stageText('writing', 256, 512)).toBe('Writing… 50 %');
    expect(stageText('verifying', 512, 512)).toBe('Checking… 100 %');
    expect(stageText('done', 512, 512)).toBe('Done');
    expect(overallPercent('writing', 512, 512)).toBe(50);
    expect(overallPercent('verifying', 0, 512)).toBe(50);
    expect(overallPercent('verifying', 256, 512)).toBe(75);
    expect(overallPercent('done', 0, 0)).toBe(100);
    expect(fileName('home:/Documents/blink.hex')).toBe('blink.hex');
  });
});
