import { describe, expect, it } from 'vitest';
import { EditorState, Text } from '@codemirror/state';
import { forEachDiagnostic, lintGutter, setDiagnostics } from '@codemirror/lint';
import { clearEditedDiagnostics, toLintDiagnostics } from './diagnostics';

const DOC = '#include <FaberLink.h>\n\nvoid setup() {\n  pinMod(13, OUTPUT);\n  int x = ;\n}';

describe('compiler messages in the code', () => {
  const doc = Text.of(DOC.split('\n'));
  const at = (d: { from: number; to: number }) => DOC.slice(d.from, d.to);

  it('underline the word at the line and column', () => {
    const [d] = toLintDiagnostics(doc, [
      { severity: 'error', line: 4, column: 3, message: "'pinMod' was not declared" },
    ]);
    expect(at(d!)).toBe('pinMod');
    expect(d).toMatchObject({ severity: 'error', message: "'pinMod' was not declared" });
  });

  it('underline the rest of the line when the column is on punctuation', () => {
    const [d] = toLintDiagnostics(doc, [
      { severity: 'error', line: 5, column: 11, message: 'expected primary-expression' },
    ]);
    expect(at(d!)).toBe(';');
  });

  it('underline the code of the line without a column, and keep warnings amber', () => {
    const [d] = toLintDiagnostics(doc, [{ severity: 'warning', line: 3, message: 'unused' }]);
    expect(at(d!)).toBe('void setup() {');
    expect(d!.severity).toBe('warning');
  });

  it('keep a line or column past the end inside the document, and skip a message without a line', () => {
    const list = toLintDiagnostics(doc, [
      { severity: 'error', line: 99, column: 99, message: 'end' },
      { severity: 'error', message: 'no line' },
    ]);
    expect(list).toHaveLength(1);
    expect(list[0]!.from).toBeLessThanOrEqual(DOC.length);
    expect(list[0]!.to).toBeLessThanOrEqual(DOC.length);
  });
});

describe('editing a line with a message', () => {
  function withMessages() {
    let state = EditorState.create({
      doc: DOC,
      extensions: [lintGutter(), clearEditedDiagnostics],
    });
    state = state.update(
      setDiagnostics(
        state,
        toLintDiagnostics(state.doc, [
          { severity: 'error', line: 4, column: 3, message: 'a' },
          { severity: 'error', line: 5, column: 11, message: 'b' },
        ]),
      ),
    ).state;
    return state;
  }
  const messages = (state: EditorState) => {
    const out: string[] = [];
    forEachDiagnostic(state, (d, from, to) =>
      out.push(`${d.message}:${state.doc.sliceString(from, to)}`),
    );
    return out;
  };

  it('takes its underline away, and keeps the others where they are', () => {
    const state = withMessages();
    expect(messages(state)).toEqual(['a:pinMod', 'b:;']);
    const line4 = state.doc.line(4);
    const edited = state.update({ changes: { from: line4.from + 8, insert: 'e' } }).state;
    expect(messages(edited)).toEqual(['b:;']);
  });

  it('moves the others along with an edit elsewhere', () => {
    const state = withMessages();
    const edited = state.update({ changes: { from: 0, insert: '// top\n' } }).state;
    expect(messages(edited)).toEqual(['a:pinMod', 'b:;']);
  });
});
