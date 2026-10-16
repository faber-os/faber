import { describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { ensureSyntaxTree } from '@codemirror/language';
import { cpp } from '@codemirror/lang-cpp';
import { callAt, fitting, signatureTooltip } from './signature';
import { sketchContext } from './sketchIndex';

/** A state with the cursor at `|`. */
function at(code: string, others: { name: string; text: string }[] = []) {
  const pos = code.indexOf('|');
  const doc = code.replace('|', '');
  const state = EditorState.create({
    doc,
    selection: { anchor: pos },
    extensions: [cpp(), sketchContext.of({ file: 'Rover.ino', others: () => others })],
  });
  ensureSyntaxTree(state, doc.length, 5000);
  return state;
}
const call = (code: string) => {
  const s = at(code);
  return callAt(s, s.selection.main.head);
};
const shown = (code: string, others?: { name: string; text: string }[]) => {
  const s = at(code, others);
  const tip = signatureTooltip(s);
  if (!tip) return null;
  const dom = tip.create(null as never).dom;
  return {
    rows: [...dom.querySelectorAll('.cm-signature-row')].map((r) => r.textContent),
    active: dom.querySelector('.cm-signature-active')?.textContent ?? null,
  };
};

describe('which argument is being typed', () => {
  it('counts the commas of the call the cursor is in', () => {
    expect(call('void f() { digitalWrite(|')).toMatchObject({ name: 'digitalWrite', argIndex: 0 });
    expect(call('void f() { digitalWrite(13, |')).toMatchObject({ argIndex: 1 });
    expect(call('void f() { digitalWrite(13, HI|GH); }')).toMatchObject({ argIndex: 1 });
  });

  it('skips nested calls, brackets, strings and comments', () => {
    expect(call('void f() { map(analogRead(A0), 0, x[1, 2], |')).toMatchObject({
      name: 'map',
      argIndex: 3,
    });
    expect(call('void f() { g("a, (b", \'(\', /* , ( */ |')).toMatchObject({
      name: 'g',
      argIndex: 2,
    });
    expect(call('void f() { delay(millis(|')).toMatchObject({ name: 'millis', argIndex: 0 });
  });

  it('knows the object of a method and the type of a declaration', () => {
    expect(call('void f() { Serial.print(x, |')).toMatchObject({
      name: 'print',
      owner: 'Serial',
      op: '.',
      argIndex: 1,
    });
    expect(call('FaberUltrasonic front("front", |')).toMatchObject({
      name: 'front',
      declaredType: 'FaberUltrasonic',
      argIndex: 1,
    });
  });

  it('is not in a call outside parentheses, after a ; or in if (…)', () => {
    expect(call('void f() { x = 1; |')).toBeNull();
    expect(call('void f() { if (a == |')).toBeNull();
    expect(call('int a[] = {1, |')).toBeNull();
  });
});

describe('the signature tooltip', () => {
  it('shows an Arduino function with the current parameter in bold', () => {
    expect(shown('void f() { digitalWrite(13, |')).toEqual({
      rows: ['void digitalWrite(uint8_t pin, uint8_t value)'],
      active: 'uint8_t value',
    });
  });

  it('shows the overloads that can take the argument', () => {
    expect(shown('void f() { tone(8, 440, |')!.rows).toEqual([
      'void tone(uint8_t pin, unsigned int frequency, unsigned long duration)',
    ]);
    expect(fitting([], 3)).toEqual([]);
  });

  it('shows the methods of an object, a FaberLink constructor and the sketch’s own functions', () => {
    expect(shown('void f() { Serial.begin(|')!.rows[0]).toBe('void begin(unsigned long baud)');
    expect(shown('FaberUltrasonic front("front", |')).toMatchObject({
      active: 'uint8_t triggerPin',
    });
    expect(shown('FaberUltrasonic sonar("s", 7, 6);\nvoid f() { sonar.begin(|')!.rows).toEqual([
      'void begin()',
    ]);
    expect(
      shown('void f() { drive(10, |', [
        { name: 'motor.h', text: 'void drive(int speed, bool forward);' },
      ]),
    ).toEqual({
      rows: ['void drive(int speed, bool forward)'],
      active: 'bool forward',
    });
  });

  it('shows nothing for a function it doesn’t know or one being declared', () => {
    expect(shown('void f() { mystery(|')).toBeNull();
    expect(shown('void drive(int speed, |')).toBeNull();
  });
});
