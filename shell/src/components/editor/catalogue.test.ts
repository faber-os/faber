import { describe, expect, it } from 'vitest';
import {
  ARDUINO_CLASSES,
  ARDUINO_CONSTANTS,
  ARDUINO_FUNCTIONS,
  ARDUINO_OBJECTS,
  ARDUINO_TYPES,
  CPP_KEYWORDS,
  SNIPPETS,
} from './arduinoApi';
import { FABERLINK_CLASSES, FABERLINK_CONSTANTS } from './faberlinkApi';
import { paramName, parseSignature } from './catalogue';
import { CLASSES, apiMembers, lookup } from './knowledge';
import { en } from '../../lib/i18n/en';

const ALL_CLASSES = [...ARDUINO_CLASSES, ...FABERLINK_CLASSES];

const duplicates = (names: string[]) => names.filter((n, i) => names.indexOf(n) !== i);

describe('signatures', () => {
  it('parse into return type, name and parameters', () => {
    expect(parseSignature('void pinMode(uint8_t pin, uint8_t mode)')).toEqual({
      name: 'pinMode',
      overload: {
        returns: 'void',
        params: ['uint8_t pin', 'uint8_t mode'],
        signature: 'void pinMode(uint8_t pin, uint8_t mode)',
      },
    });
    expect(parseSignature('unsigned long millis()').overload.params).toEqual([]);
    expect(
      parseSignature('void attachInterrupt(uint8_t interrupt, void (*isr)(), int mode)').overload
        .params,
    ).toEqual(['uint8_t interrupt', 'void (*isr)()', 'int mode']);
  });

  it('name their parameters', () => {
    expect(paramName('uint8_t pin')).toBe('pin');
    expect(paramName('const char* text')).toBe('text');
    expect(paramName('float minM = 0.1f')).toBe('minM');
    expect(paramName('void (*isr)()')).toBe('isr');
    expect(paramName('x')).toBe('x');
  });
});

describe('the Arduino catalogue', () => {
  it('has no name twice', () => {
    const topLevel = [
      ...ARDUINO_FUNCTIONS,
      ...ARDUINO_CONSTANTS,
      ...ALL_CLASSES,
      ...ARDUINO_OBJECTS,
      ...ARDUINO_TYPES,
    ].map((e) => e.name);
    expect(duplicates(topLevel)).toEqual([]);
    for (const c of ALL_CLASSES) {
      expect(duplicates(c.members.map((m) => m.name)), c.name).toEqual([]);
    }
    expect(duplicates(FABERLINK_CONSTANTS.map((k) => k.name))).toEqual([]);
    expect(duplicates(CPP_KEYWORDS)).toEqual([]);
    expect(duplicates(SNIPPETS.map((s) => s.label))).toEqual([]);
  });

  it('gives every entry a signature and a description of a sentence or more', () => {
    const functions = [...ARDUINO_FUNCTIONS, ...ALL_CLASSES.flatMap((c) => c.members)];
    for (const f of functions) {
      expect(f.overloads.length, f.name).toBeGreaterThan(0);
      for (const o of f.overloads) expect(o.signature, f.name).toContain(`${f.name}(`);
    }
    const described = [
      ...functions,
      ...ARDUINO_CONSTANTS,
      ...FABERLINK_CONSTANTS,
      ...ALL_CLASSES,
      ...ARDUINO_OBJECTS,
      ...ARDUINO_TYPES,
    ];
    for (const e of described) {
      expect(e.doc.length, e.name).toBeGreaterThan(5);
      expect(e.doc, e.name).toMatch(/[.…)]$/);
    }
  });

  it('covers the core of the Arduino language', () => {
    for (const name of [
      'pinMode',
      'digitalWrite',
      'digitalRead',
      'analogRead',
      'analogWrite',
      'millis',
      'micros',
      'delay',
      'map',
      'constrain',
      'random',
      'attachInterrupt',
      'bitRead',
      'isDigit',
      'tone',
      'pulseIn',
    ]) {
      expect(lookup(name)?.kind, name).toBe('function');
    }
    for (const name of [
      'HIGH',
      'LOW',
      'INPUT',
      'OUTPUT',
      'INPUT_PULLUP',
      'LED_BUILTIN',
      'A0',
      'A15',
    ]) {
      expect(lookup(name)?.kind, name).toBe('constant');
    }
    for (const name of ['Serial', 'Serial3', 'Wire', 'SPI', 'EEPROM']) {
      expect(lookup(name)?.kind, name).toBe('object');
    }
  });

  it('gives every object a class it knows, and every base class exists', () => {
    for (const o of ARDUINO_OBJECTS) expect(CLASSES.has(o.type), o.name).toBe(true);
    for (const c of ALL_CLASSES) if (c.extends) expect(CLASSES.has(c.extends), c.name).toBe(true);
  });

  it('includes the members of base classes: Serial reads like any Stream', () => {
    const serial = apiMembers('HardwareSerial').map((m) => m.name);
    expect(serial).toEqual(
      expect.arrayContaining(['begin', 'print', 'println', 'available', 'read']),
    );
    expect(apiMembers('TwoWire').map((m) => m.name)).toEqual(
      expect.arrayContaining(['beginTransmission', 'write', 'read']),
    );
  });

  it('has a text in the catalogue for every snippet', () => {
    for (const s of SNIPPETS) expect(en, s.key).toHaveProperty([s.key]);
  });
});
