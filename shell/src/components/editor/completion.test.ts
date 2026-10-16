import { describe, expect, it } from 'vitest';
import { CompletionContext, type Completion } from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { ensureSyntaxTree } from '@codemirror/language';
import { cpp } from '@codemirror/lang-cpp';
import { arduinoCompletion } from './completion';
import { docAt } from './hover';
import { sketchContext, type SketchFile } from './sketchIndex';

const MOTOR_H: SketchFile = {
  name: 'motor.h',
  text: '#pragma once\n#define MOTOR_PIN 9\nvoid drive(int speed, bool forward);\nstruct Point { int x; int y; float length() const; };\n',
};

/** A state with the cursor at `|`, as the tab Rover.ino next to motor.h. */
function at(code: string) {
  const pos = code.indexOf('|');
  const doc = code.replace('|', '');
  const state = EditorState.create({
    doc,
    selection: { anchor: pos },
    extensions: [cpp(), sketchContext.of({ file: 'Rover.ino', others: () => [MOTOR_H] })],
  });
  ensureSyntaxTree(state, doc.length, 5000);
  return { state, pos };
}

function complete(code: string, explicit = false) {
  const { state, pos } = at(code);
  return arduinoCompletion(new CompletionContext(state, pos, explicit));
}
const labels = (code: string, explicit = false) =>
  complete(code, explicit)?.options.map((o) => o.label) ?? null;
const option = (code: string, label: string) =>
  complete(code)?.options.find((o) => o.label === label);

describe('completion', () => {
  it('offers the Arduino API after two letters, with its parameters as detail', () => {
    const result = complete('void setup() {\n  pinM|\n}');
    expect(result!.from).toBe('void setup() {\n  '.length);
    const pinMode = result!.options.find((o) => o.label === 'pinMode')!;
    expect(pinMode).toMatchObject({ type: 'function', detail: '(uint8_t pin, uint8_t mode)' });
    expect(labels('void f() { di|')).toEqual(
      expect.arrayContaining(['digitalWrite', 'digitalRead', 'double', 'detachInterrupt']),
    );
  });

  it('waits for two letters unless asked with Ctrl+Space', () => {
    expect(complete('void f() { p|')).toBeNull();
    expect(complete('void f() { |')).toBeNull();
    expect(labels('void f() { p|', true)).toContain('pinMode');
    expect(labels('void f() { |', true)).toContain('HIGH');
  });

  it('offers constants, objects, classes, types, keywords and snippets', () => {
    const all = labels('void f() { |', true)!;
    for (const l of [
      'INPUT_PULLUP',
      'LED_BUILTIN',
      'A0',
      'Serial',
      'Wire',
      'FaberUltrasonic',
      'String',
      'uint8_t',
      'while',
      'every',
      'setup',
    ])
      expect(all, l).toContain(l);
    expect(option('void f() { ev|', 'every')).toMatchObject({
      type: 'snippet',
      detail: 'Run code every so many ms, without delay()',
    });
  });

  it('offers the sketch’s own names, from this tab and the others, locals first', () => {
    const code = 'int total = 0;\nlong scaled(int value) {\n  long result = value;\n  re|\n}';
    const result = complete(code)!;
    const local = result.options.find((o) => o.label === 'result')!;
    const global = result.options.find((o) => o.label === 'total')!;
    expect(local).toMatchObject({ type: 'variable', detail: 'long' });
    expect(local.boost).toBeGreaterThan(global.boost!);
    expect(option('void f() { dr|', 'drive')).toMatchObject({
      type: 'function',
      detail: '(int speed, bool forward)',
    });
    expect(option('void f() { MO|', 'MOTOR_PIN')).toMatchObject({ type: 'constant', detail: '9' });
    expect(option('void f() { Po|', 'Point')).toMatchObject({ type: 'class' });
  });

  it('offers an object’s members after a dot', () => {
    expect(labels('void f() { Serial.|')).toEqual(
      expect.arrayContaining(['begin', 'print', 'println', 'available', 'read', 'parseInt']),
    );
    expect(labels('void f() { Wire.beg|')).toContain('beginTransmission');
    expect(labels('FaberUltrasonic sonar("front", 7, 6);\nvoid f() { sonar.|')).toEqual([
      'begin',
      'name',
    ]);
    expect(labels('FaberLink link(Serial);\nvoid loop() { link.|')).toEqual(
      expect.arrayContaining(['begin', 'poll', 'log', 'connected', 'engageEstop']),
    );
    expect(labels('void f() { String s = "x"; s.|')).toContain('toInt');
    expect(labels('Point p;\nvoid f() { p.|')).toEqual(['x', 'y', 'length']);
    expect(labels('Servo arm;\nvoid f() { Servo* a = &arm; a->|')).toContain('write');
    expect(labels('FaberDistance d("d", A0, 0.1f, 0.8f, faberlink::|')).toContain(
      'DISTANCE_INFRARED',
    );
    expect(complete('void f() { mystery.|')).toBeNull();
  });

  it('offers directives after # and libraries after #include <', () => {
    expect(labels('#inc|')).toContain('include');
    const headers = complete('#include <Wi|')!;
    expect(headers.options.map((o) => o.label)).toContain('Wire.h');
    expect(headers.options.find((o) => o.label === 'Wire.h')!.apply).toBe('Wire.h>');
  });

  it('stays quiet in comments, strings and numbers', () => {
    expect(complete('// pinM|')).toBeNull();
    expect(complete('const char* s = "pinM|";')).toBeNull();
    expect(complete('int x = 12|', true)).toBeNull();
    expect(complete('#error the motor is not wired ye|')).toBeNull();
  });

  it('writes a function’s call with a tab stop per parameter', () => {
    const { state } = at('void setup() {\n  pinM|\n}');
    const view = new EditorView({ state, parent: document.body });
    const result = arduinoCompletion(
      new CompletionContext(view.state, view.state.selection.main.head, false),
    )!;
    const pinMode = result.options.find((o) => o.label === 'pinMode')! as Completion & {
      apply: (v: EditorView, c: Completion, from: number, to: number) => void;
    };
    pinMode.apply(view, pinMode, result.from, view.state.selection.main.head);
    expect(view.state.doc.line(2).text).toBe('  pinMode(pin, mode)');
    const sel = view.state.selection.main;
    expect(view.state.sliceDoc(sel.from, sel.to)).toBe('pin');
    view.destroy();
  });
});

describe('hover docs', () => {
  const docOf = (code: string) => {
    const { state, pos } = at(code);
    const found = docAt(state, pos);
    if (!found) return null;
    return { word: state.sliceDoc(found.from, found.to), ...found.doc };
  };

  it('shows an Arduino function’s signature and description', () => {
    expect(docOf('void f() { pin|Mode(13, OUTPUT); }')).toMatchObject({
      word: 'pinMode',
      signatures: ['void pinMode(uint8_t pin, uint8_t mode)'],
      library: 'arduino',
    });
    expect(docOf('void f() { digitalWrite(13, HI|GH); }')!.text).toMatch(/supply voltage/);
  });

  it('shows the method of an object’s class, and FaberLink’s classes', () => {
    expect(docOf('void f() { Serial.be|gin(9600); }')!.signatures).toEqual([
      'void begin(unsigned long baud)',
      'void begin(unsigned long baud, uint8_t config)',
    ]);
    expect(docOf('FaberUltra|sonic front("front", 7, 6);')).toMatchObject({
      library: 'faberlink',
      include: 'FaberLink.h',
    });
    expect(docOf('FaberUltrasonic sonar("s", 7, 6);\nvoid f() { sonar.be|gin(); }')!.text).toMatch(
      /pins/,
    );
  });

  it('shows the sketch’s own function with its signature and place', () => {
    const d = docOf('void f() { dri|ve(3, true); }')!;
    expect(d.signatures).toEqual(['void drive(int speed, bool forward)']);
    expect(d.symbol).toMatchObject({ file: 'motor.h', line: 3 });
    expect(docOf('long scaled(int value) { return val|ue; }')!.signatures).toEqual(['int value']);
  });

  it('shows nothing in comments, strings, numbers or for unknown names', () => {
    expect(docOf('// pin|Mode')).toBeNull();
    expect(docOf('int x = 1|2;')).toBeNull();
    expect(docOf('void f() { myst|ery(); }')).toBeNull();
  });
});
