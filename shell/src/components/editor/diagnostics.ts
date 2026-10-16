/**
 * The compiler's errors and warnings in the code: after Verify, each message that points into a
 * tab is underlined there (red for an error, amber for a warning), with the message on hover and
 * a marker in the gutter (@codemirror/lint, no live linting). Editing the line of a message takes
 * its underline away, since the message may no longer be true; the next Verify replaces them all.
 */
import type { Diagnostic } from '@codemirror/lint';
import { forEachDiagnostic, setDiagnosticsEffect } from '@codemirror/lint';
import { EditorState, type Text } from '@codemirror/state';

/** A compiler message for this file (SketchDiagnostic, without the file). */
export interface CompilerMessage {
  severity: 'error' | 'warning';
  line?: number;
  column?: number;
  message: string;
}

const WORD = /[\w$]/;

/**
 * Where a message goes in the document: gcc's 1-based line and column (a column counts bytes,
 * the same as characters in the ASCII code sketches are written in). The underline covers the
 * word at the column, or the rest of the line from there; a message without a column underlines
 * the line's code. Lines past the end go to the last line.
 */
export function toLintDiagnostics(doc: Text, messages: readonly CompilerMessage[]): Diagnostic[] {
  const out: Diagnostic[] = [];
  for (const m of messages) {
    if (m.line === undefined) continue;
    const line = doc.line(Math.min(Math.max(1, m.line), doc.lines));
    const text = line.text;
    const indent = text.length - text.trimStart().length;
    let from: number;
    let to: number;
    if (m.column === undefined || m.column < 1) {
      from = indent;
      to = text.trimEnd().length;
    } else {
      from = Math.min(m.column - 1, text.length);
      to = from;
      while (to < text.length && WORD.test(text[to]!)) to++;
      // Not on a word (an operator, a `#`, a bracket): from there to the end of the line.
      if (to === from) to = Math.max(from, text.trimEnd().length);
    }
    out.push({
      from: line.from + from,
      to: line.from + to,
      severity: m.severity,
      message: m.message,
    });
  }
  return out;
}

/**
 * Takes away a message's underline once its line is edited. A transaction extender, so the
 * change and the removal are one step (undo brings neither back).
 */
export const clearEditedDiagnostics = EditorState.transactionExtender.of((tr) => {
  if (!tr.docChanged) return null;
  const doc = tr.startState.doc;
  const kept: Diagnostic[] = [];
  let dropped = false;
  forEachDiagnostic(tr.startState, (d, from, to) => {
    const start = doc.lineAt(from).from;
    const end = doc.lineAt(Math.min(to, doc.length)).to;
    if (tr.changes.touchesRange(start, end)) dropped = true;
    else kept.push({ ...d, from: tr.changes.mapPos(from, 1), to: tr.changes.mapPos(to, -1) });
  });
  return dropped ? { effects: setDiagnosticsEffect.of(kept) } : null;
});
