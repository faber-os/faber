import { describe, expect, it } from 'vitest';
import {
  diagnosticPlace,
  diagnosticsByTab,
  groupDigits,
  isMainFile,
  isSketchFile,
  offsetOf,
  sizeLines,
  sketchNameProblem,
  sketchOf,
  tabOrder,
} from './arduino';

describe('sketches', () => {
  it('a code file belongs to the sketch its folder is named after', () => {
    expect(sketchOf('home:/Arduino/Blink/Blink.ino')).toEqual({
      folder: 'home:/Arduino/Blink',
      name: 'Blink',
      main: 'home:/Arduino/Blink/Blink.ino',
    });
    expect(sketchOf('home:/Arduino/Rover/motor.h')?.main).toBe('home:/Arduino/Rover/Rover.ino');
    expect(sketchOf('home:/Documents/notes.txt')).toBeNull();
    expect(isMainFile('home:/Arduino/Rover/Rover.ino')).toBe(true);
    expect(isMainFile('home:/Arduino/Rover/motor.h')).toBe(false);
  });

  it('knows which files are tabs', () => {
    for (const n of ['a.ino', 'b.h', 'c.cpp', 'd.S']) expect(isSketchFile(n)).toBe(true);
    for (const n of ['notes.txt', '.ino', 'x.ino.bak']) expect(isSketchFile(n)).toBe(false);
  });

  it('orders tabs like the Arduino IDE: the main .ino first', () => {
    expect(
      tabOrder(['motor.cpp', 'Rover.ino', 'motor.h', 'arm.ino', 'pins.h'], 'Rover.ino'),
    ).toEqual(['Rover.ino', 'arm.ino', 'motor.h', 'pins.h', 'motor.cpp']);
  });

  it('checks a new name with the same rule as the core', () => {
    expect(sketchNameProblem('Rover2')).toBeNull();
    expect(sketchNameProblem('motor_test-v1.2')).toBeNull();
    expect(sketchNameProblem('')).toBe('Give the sketch a name.');
    expect(sketchNameProblem('_x')).toBe('Start with a letter or a digit.');
    expect(sketchNameProblem('my sketch')).toBe(
      '“␣” can’t be in a sketch’s name: use letters, digits, _, - and .',
    );
    expect(sketchNameProblem('a'.repeat(64))).toBe('At most 63 characters.');
  });
});

describe('sizes and messages', () => {
  it('reads like the Arduino IDE, with grouped digits', () => {
    expect(groupDigits(6896)).toBe('6 896');
    expect(groupDigits(253952)).toBe('253 952');
    expect(groupDigits(512)).toBe('512');
    expect(sizeLines({ flash: 6896, flashMax: 32256, ram: 590, ramMax: 2048 })).toEqual([
      'Program: 6 896 bytes of 32 256 (21 %)',
      'Memory: 590 bytes of 2 048 (29 %)',
    ]);
    expect(sizeLines({ flash: 10, flashMax: 100, ram: 0, ramMax: 0 })).toHaveLength(1);
  });

  it('says where an error is', () => {
    expect(diagnosticPlace({ file: 'motor.h', line: 3, column: 2 })).toBe('motor.h:3:2');
    expect(diagnosticPlace({ file: 'Blink.ino', line: 5 })).toBe('Blink.ino:5');
    expect(diagnosticPlace({ file: 'FaberLink.h' })).toBe('FaberLink.h');
    expect(diagnosticPlace({})).toBe('');
  });

  it('sorts the compiler’s messages by the tab they point into', () => {
    const d = (file: string | undefined, inSketch: boolean, line?: number) => ({
      file,
      inSketch,
      line,
      message: `${file}:${line}`,
    });
    const list = [
      d('Rover.ino', true, 3),
      d('motor.h', true, 1),
      d('Rover.ino', true, 9),
      d('FaberLink.h', false, 9),
      d('Rover.ino', true),
      d('src/extra.cpp', true, 2),
      d(undefined, false),
    ];
    const byTab = diagnosticsByTab(list, ['Rover.ino', 'motor.h']);
    expect([...byTab.keys()]).toEqual(['Rover.ino', 'motor.h']);
    expect(byTab.get('Rover.ino')!.map((x) => x.line)).toEqual([3, 9]);
    expect(byTab.get('motor.h')!.map((x) => x.line)).toEqual([1]);
  });

  it('finds a line and column in the text', () => {
    const text = 'ab\ncdef\n\ng';
    expect(offsetOf(text, 1)).toBe(0);
    expect(offsetOf(text, 2, 3)).toBe(5);
    expect(offsetOf(text, 2, 99)).toBe(7);
    expect(offsetOf(text, 4)).toBe(9);
    expect(offsetOf(text, 99)).toBe(text.length);
  });
});
