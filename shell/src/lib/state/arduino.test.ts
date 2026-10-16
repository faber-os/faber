import { describe, expect, it } from 'vitest';
import { codeView, identityCaveat, partCodeText, seconds, toolsStepText } from './arduino';
import type { BoardCodeInfo, FlashRecord } from './types';

const RECORD: FlashRecord = {
  id: 'usb-2341-0043-S1',
  board: 'uno',
  boardName: 'Arduino Uno',
  sketch: 'home:/Arduino/Blink/Blink.ino',
  sketchName: 'Blink',
  buildId: '0123456789abcdef',
  sha256: 'ab'.repeat(32),
  bytes: 7000,
  flashedAt: '2026-10-16T12:00:00Z',
  identity: { serial: 'S1', port: 'usb:2341:0043', unique: true },
  checkMs: 1100,
};

const info = (code: BoardCodeInfo['code'], unique = true): BoardCodeInfo => ({
  port: 'usb:2341:0043',
  identity: { port: 'usb:2341:0043', unique, ...(unique ? { serial: 'S1' } : {}) },
  code,
});

const date = (iso: string) => `[${iso}]`;

describe('the code on a board', () => {
  it('says the board still runs the sketch only with a proof', () => {
    const v = codeView(
      info({ result: 'same', record: RECORD, proof: { method: 'build-id' } }),
      date,
    );
    expect(v).toMatchObject({
      kind: 'same',
      text: 'This board still runs Blink from [2026-10-16T12:00:00Z].',
      proof: 'The board said so itself: its build id is the one Faber compiled.',
    });
    const r = codeView(
      info({ result: 'same', record: RECORD, proof: { method: 'read-back', elapsedMs: 1240 } }),
      date,
    );
    expect(r.kind === 'same' && r.proof).toBe('Faber read the program back and compared it (1 s).');
  });

  it('says plainly when the code changed outside Faber', () => {
    const v = codeView(
      info({ result: 'different', record: RECORD, proof: { method: 'build-id' } }),
      date,
    );
    expect(v.text).toBe(
      'This board has different code now (changed outside Faber since [2026-10-16T12:00:00Z]). The last code Faber flashed is Blink.',
    );
  });

  it('asks before reading back, and says what it costs', () => {
    const v = codeView(info({ result: 'needs-check', record: RECORD }), date);
    expect(v.kind).toBe('needs-check');
    expect(v.text).toContain('resets the board and takes about 1 s');
    expect(codeView(info({ result: 'never' }), date).text).toBe(
      'Faber has never flashed this board.',
    );
  });

  it('warns that a board without a serial number is known only by its port', () => {
    expect(identityCaveat(info({ result: 'never' }))).toBeNull();
    expect(identityCaveat(info({ result: 'never' }, false))).toContain('can’t tell it apart');
  });

  it('rounds the time to whole seconds, at least one', () => {
    expect([seconds(200), seconds(1400), seconds(23_600)]).toEqual([1, 1, 24]);
  });
});

describe('in Parts', () => {
  const link = { link: 'ready', hardwareEstop: false, failsafe: false } as const;
  it('names the sketch Faber flashed, or says it is unknown', () => {
    expect(partCodeText({ ...link })).toBeNull();
    expect(partCodeText({ ...link, code: { buildId: 'ab', record: RECORD } })).toBe(
      'Code from Faber: Blink',
    );
    expect(partCodeText({ ...link, code: {} })).toBe('Code not flashed from Faber (unknown)');
  });
});

describe('installing the tools', () => {
  it('shows the download in megabytes', () => {
    expect(toolsStepText({ step: 'downloading', done: 5_200_000, total: 10_000_000 })).toBe(
      'Downloading arduino-cli… 5 of 10 MB',
    );
    expect(toolsStepText({ step: 'installing-core', line: 'x' })).toBe(
      'Installing the Arduino AVR core (about 140 MB)…',
    );
  });
});
