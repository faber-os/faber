import { describe, expect, it } from 'vitest';
import { emulate, loadHex } from './avr';
import { lookup } from './catalog';
import { runCircuit } from './circuit';
import type { Diagram } from './types';

/** Intel HEX for program words at address 0. */
function hexOf(words: number[]): string {
  const bytes = words.flatMap((w) => [w & 0xff, w >> 8]);
  const rec = [bytes.length, 0, 0, 0, ...bytes];
  const sum = (0x100 - (rec.reduce((a, b) => a + b, 0) & 0xff)) & 0xff;
  const h = (n: number) => n.toString(16).padStart(2, '0').toUpperCase();
  return `:${rec.map(h).join('')}${h(sum)}\n:00000001FF\n`;
}

/** ldi r16,mask · out DDRB,r16 · out PORTB,r16 · rjmp . : PORTB bits on. */
const setPortB = (mask: number) =>
  hexOf([0xe000 | ((mask & 0xf0) << 4) | (mask & 0x0f), 0xb904, 0xb905, 0xcfff]);

describe('the emulator', () => {
  it('reads Intel HEX into program words', () => {
    expect(Array.from(loadHex(setPortB(0x20), 16).slice(0, 4))).toEqual([
      0xe200, 0xb904, 0xb905, 0xcfff,
    ]);
  });

  it('runs an Uno program that drives D13 high', () => {
    const emu = emulate('uno', setPortB(0x20));
    emu.run(2);
    const pins = emu.pinStates();
    expect(pins.get('D13')).toEqual({ mode: 'out', level: 1 });
    expect(pins.get('D12')).toBeUndefined();
  });

  it('on a Mega, D13 is PB7', () => {
    const emu = emulate('mega', setPortB(0x80));
    emu.run(2);
    expect(emu.pinStates().get('D13')).toMatchObject({ mode: 'out' });
  });

  it('the pin it drives lights the LED on the drawing', () => {
    const defs = lookup();
    const doc: Diagram = {
      name: 't',
      parts: [
        { id: 'u', def: 'uno', name: 'u', x: 0, y: 0, rot: 0 },
        { id: 'a', def: 'adapter12', name: 'a', x: -60, y: 0, rot: 0 },
        { id: 'r', def: 'res?ohms=330&w=0.25', name: 'r', x: 20, y: -20, rot: 0 },
        { id: 'l', def: 'led-green', name: 'l', x: 40, y: -20, rot: 0 },
      ],
      wires: [
        ['a.+', 'u.VIN'],
        ['a.-', 'u.GND2'],
        ['u.D13', 'r.1'],
        ['r.2', 'l.A'],
        ['l.K', 'u.GND1'],
      ].map(([a, b], i) => ({ id: `w${i}`, a: a!, b: b! })),
      done: {},
      seq: 9,
    };
    const emu = emulate('uno', setPortB(0x20));
    emu.run(2);
    const pins = new Map([...emu.pinStates()].map(([k, v]) => [`u.${k}`, v]));
    const c = runCircuit(doc, defs, { pins });
    expect(c.parts.get('l')).toBe('on');
  });
});

describe('a real sketch (fixtures/CircuitBlink.ino, compiled by arduino-cli 1.5.1)', () => {
  const hexes = import.meta.glob('./fixtures/*.hex', {
    query: '?raw',
    import: 'default',
    eager: true,
  }) as Record<string, string>;

  for (const [board, file] of [
    ['uno', 'uno'],
    ['mega', 'mega2560'],
  ] as const) {
    it(`runs on the ${board}: Serial, millis, digitalWrite, analogWrite, inputs`, () => {
      const emu = emulate(board, hexes[`./fixtures/CircuitBlink.${file}.hex`]!);
      let out = '';
      emu.onSerial = (c) => (out += c);
      emu.setInputs(
        new Map([
          ['A0', 2.5],
          ['D2', 0],
        ]),
      );
      emu.run(300, 60_000);
      expect(out).toContain('Hello from Circuits');
      expect(out).toMatch(/A0=5[01]\d button=pressed/);
      const early = emu.pinStates();
      expect(early.get('D13')?.mode).toBe('out');
      expect(early.get('D2')).toEqual({ mode: 'pullup' });
      expect(early.get('D9')?.mode).toBe('out');
      emu.run(500, 60_000);
      const later = emu.pinStates();
      // D13 blinks at 1 Hz: high for one half of the second, low for the other.
      expect((early.get('D13') as { level: number }).level).not.toBe(
        (later.get('D13') as { level: number }).level,
      );
    });
  }
});
