import { describe, expect, it } from 'vitest';
import { cppLanguage } from '@codemirror/lang-cpp';
import { baseType, localsAt, sketchSymbols, stringSource } from './symbols';

const SKETCH = `#include <FaberLink.h>
#define LED 13
#define SQ(x) ((x) * (x))

FaberLink link(Serial);
FaberUltrasonic front("front", 7, 6);
int count = 0, *where;
const unsigned long PERIOD = 500;
void drive(int speed, bool forward);
typedef unsigned char u8;

enum Mode { IDLE, RUN };

struct Point {
  int x;
  int y;
  float length() const;
};

class Arm {
 public:
  Arm(int pin);
  void raise(int degrees) { angle = degrees; }
 private:
  int angle;
};

#ifdef DEBUG
int debugLevel = 1;
#endif

void setup() {
  pinMode(LED, OUTPUT);
}

long scaled(int value, float factor) {
  long result = value * factor;
  for (int i = 0; i < 3; i++) {
    int inner = i;
    HERE
  }
  return result;
}

void Arm::lower() {}
`;

/** The 1-based line `text` first appears on in the sketch. */
const lineOf = (text: string) => SKETCH.slice(0, SKETCH.indexOf(text)).split('\n').length;
const parse = (text: string) => cppLanguage.parser.parse(text);
const KNOWN = new Set(['FaberLink', 'FaberUltrasonic']);
const symbols = (text = SKETCH) =>
  sketchSymbols(parse(text), stringSource(text), 'Rover.ino', (n) => KNOWN.has(n));
const named = (name: string, text = SKETCH) => symbols(text).find((s) => s.name === name);

describe('the sketch’s own names', () => {
  it('finds functions with their signature, return type, parameters and line', () => {
    expect(named('scaled')).toMatchObject({
      kind: 'function',
      type: 'long',
      params: ['int value', 'float factor'],
      signature: 'long scaled(int value, float factor)',
      line: lineOf('long scaled'),
    });
    expect(named('setup')).toMatchObject({
      kind: 'function',
      params: [],
      line: lineOf('void setup'),
    });
    // A declaration without a body is a function too.
    expect(named('drive')).toMatchObject({
      kind: 'function',
      signature: 'void drive(int speed, bool forward)',
    });
  });

  it('finds global variables, one per declarator, with their type', () => {
    expect(named('count')).toMatchObject({
      kind: 'variable',
      type: 'int',
      line: lineOf('int count'),
    });
    expect(named('where')).toMatchObject({ kind: 'variable', type: 'int*' });
    expect(named('PERIOD')).toMatchObject({ kind: 'variable', type: 'const unsigned long' });
  });

  it('reads `FaberLink link(Serial);` as an object, not a function', () => {
    expect(named('link')).toMatchObject({ kind: 'variable', type: 'FaberLink' });
    expect(named('front')).toMatchObject({
      kind: 'variable',
      type: 'FaberUltrasonic',
      signature: 'FaberUltrasonic front("front", 7, 6)',
    });
  });

  it('finds #defines, function-like ones too', () => {
    expect(named('LED')).toMatchObject({ kind: 'define', signature: '#define LED 13', line: 2 });
    expect(named('SQ')).toMatchObject({ kind: 'define', signature: '#define SQ(x) ((x) * (x))' });
  });

  it('finds structs and classes with their public members and constructors', () => {
    const point = named('Point')!;
    expect(point).toMatchObject({ kind: 'class', signature: 'struct Point' });
    expect(point.members!.map((m) => [m.name, m.kind])).toEqual([
      ['x', 'field'],
      ['y', 'field'],
      ['length', 'method'],
    ]);
    const arm = named('Arm')!;
    // `angle` is private.
    expect(arm.members!.map((m) => m.name)).toEqual(['raise']);
    expect(arm.members![0]).toMatchObject({ signature: 'void raise(int degrees)' });
    expect(arm.constructors!.map((c) => c.signature)).toEqual(['Arm(int pin)']);
  });

  it('finds enums and their values, typedefs, and names inside #if blocks', () => {
    expect(named('Mode')).toMatchObject({ kind: 'type' });
    expect(named('RUN')).toMatchObject({ kind: 'constant', type: 'Mode' });
    expect(named('u8')).toMatchObject({ kind: 'type', signature: 'typedef unsigned char u8' });
    expect(named('debugLevel')).toMatchObject({ kind: 'variable', type: 'int' });
  });

  it('knows which class a method defined outside it belongs to', () => {
    expect(named('lower')).toMatchObject({ kind: 'method', owner: 'Arm' });
  });

  it('finds the locals and parameters in scope, nearest first', () => {
    const pos = SKETCH.indexOf('HERE');
    const locals = localsAt(parse(SKETCH), stringSource(SKETCH), pos, 'Rover.ino');
    expect(locals.map((l) => [l.name, l.kind, l.type])).toEqual([
      ['inner', 'variable', 'int'],
      ['i', 'variable', 'int'],
      ['result', 'variable', 'long'],
      ['value', 'parameter', 'int'],
      ['factor', 'parameter', 'float'],
    ]);
    // Not in scope in setup().
    const inSetup = localsAt(parse(SKETCH), stringSource(SKETCH), SKETCH.indexOf('pinMode'), 'x');
    expect(inSetup).toEqual([]);
  });

  it('keeps going in code that doesn’t parse yet', () => {
    const text = 'int speed = 3;\nvoid loop() {\n  digitalWrite(13, ';
    expect(named('speed', text)).toMatchObject({ kind: 'variable' });
    expect(named('loop', text)).toMatchObject({ kind: 'function' });
  });
});

describe('baseType', () => {
  it('names the class behind qualifiers, pointers and references', () => {
    expect(baseType('const FaberMotor&')).toBe('FaberMotor');
    expect(baseType('Servo*')).toBe('Servo');
    expect(baseType('static String')).toBe('String');
    expect(baseType('unsigned long')).toBe('long');
  });
});
