import { describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { ensureSyntaxTree } from '@codemirror/language';
import { cpp } from '@codemirror/lang-cpp';
import { codeMarks } from './semantic';

function marks(doc: string, from = 0, to = doc.length) {
  const state = EditorState.create({ doc, extensions: cpp() });
  ensureSyntaxTree(state, doc.length, 5000);
  return codeMarks(state, from, to).map((m) => [doc.slice(m.from, m.to), m.className]);
}

const of = (list: string[][], cls: RegExp) => list.filter(([, c]) => cls.test(c!));

describe('colours the grammar can’t know', () => {
  it('marks the Arduino API, its objects and their members', () => {
    const m = marks('void setup() { pinMode(13, OUTPUT); Serial.begin(9600); millis(); }');
    expect(of(m, /api/)).toEqual([
      ['pinMode', 'cm-code-api'],
      ['Serial', 'cm-code-api'],
      ['begin', 'cm-code-api'],
      ['millis', 'cm-code-api'],
    ]);
  });

  it('marks Arduino constants and ALL_CAPS names, not ALL_CAPS types', () => {
    const m = marks(
      '#define LED_PIN 13\nvoid f() { digitalWrite(LED_PIN, HIGH); analogRead(A0); int x = MAX; }',
    );
    expect(of(m, /constant/).map(([w]) => w)).toEqual(['LED_PIN', 'LED_PIN', 'HIGH', 'A0', 'MAX']);
  });

  it('marks FaberLink classes, the faberlink namespace and its constants', () => {
    const m = marks(
      'FaberLink link(Serial);\nFaberDistance d("d", A0, 0.1f, 0.8f, faberlink::DISTANCE_INFRARED);',
    );
    expect(of(m, /faberlink|constant/)).toEqual([
      ['FaberLink', 'cm-code-faberlink'],
      ['FaberDistance', 'cm-code-faberlink'],
      ['A0', 'cm-code-constant'],
      ['faberlink', 'cm-code-faberlink'],
      ['DISTANCE_INFRARED', 'cm-code-constant'],
    ]);
  });

  it('shows `FaberLink link(Serial);` as an object, not a function', () => {
    expect(of(marks('FaberLink link(Serial);'), /object/)).toEqual([['link', 'cm-code-object']]);
    expect(of(marks('FaberLink make(int speed);'), /object/)).toEqual([]);
    expect(of(marks('void go(Serial);'), /object/)).toEqual([]);
  });

  it('leaves the sketch’s own names to the grammar', () => {
    const m = marks('int speed = 3; void drive() { speed = speed + 1; }');
    expect(of(m, /code-/)).toEqual([]);
  });
});

describe('bracket colours', () => {
  it('colours brackets by depth, three colours in turn, matching pairs alike', () => {
    const doc = 'void f(int a) { if (a[g(1)]) { x = {{1}}; } }';
    const brackets = of(marks(doc), /bracket/);
    expect(brackets.map(([b, c]) => `${b}${c!.slice(-1)}`).join(' ')).toBe(
      '(1 )1 {1 (2 [3 (1 )1 ]3 )2 {2 {3 {1 }1 }3 }2 }1',
    );
  });

  it('is the same when only part of the document is looked at', () => {
    const doc = 'void f() {\n  if (x) {\n    g(h(1));\n  }\n}\n';
    const line = doc.indexOf('g(');
    const part = marks(doc, line, doc.indexOf('\n', line));
    expect(of(part, /bracket/)).toEqual([
      ['(', 'cm-bracket-3'],
      ['(', 'cm-bracket-1'],
      [')', 'cm-bracket-1'],
      [')', 'cm-bracket-3'],
    ]);
    expect(of(marks(doc), /bracket/)).toEqual(expect.arrayContaining(of(part, /bracket/)));
  });

  it('copes with brackets that aren’t closed yet', () => {
    const doc = 'void loop() { digitalWrite(13, ';
    const brackets = of(marks(doc), /bracket/);
    expect(brackets.map(([b, c]) => `${b}${c!.slice(-1)}`)).toEqual(['(1', ')1', '{1', '(2']);
  });
});
