/**
 * CodeMirror 6, set up for the Arduino editor: C++ highlighting, line numbers, search (Ctrl+F),
 * undo, bracket matching. Loaded only when an editor opens (CodeEditor.svelte imports it
 * dynamically), so the rest of the shell doesn't carry it.
 */
import { EditorState } from '@codemirror/state';
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import {
  HighlightStyle,
  bracketMatching,
  indentOnInput,
  syntaxHighlighting,
} from '@codemirror/language';
import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search';
import { cpp } from '@codemirror/lang-cpp';
import { tags } from '@lezer/highlight';

// Colours from the shell's own tokens, so the code follows light and dark like everything else.
const highlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.controlKeyword, tags.operatorKeyword], color: 'var(--info)' },
  { tag: [tags.typeName, tags.standard(tags.typeName)], color: 'var(--accent-ink)' },
  { tag: [tags.processingInstruction, tags.macroName], color: 'var(--danger)' },
  { tag: [tags.string, tags.character], color: 'var(--ok)' },
  { tag: [tags.number, tags.bool, tags.null], color: 'var(--warn)' },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment],
    color: 'var(--text-muted)',
    fontStyle: 'italic',
  },
  { tag: tags.invalid, color: 'var(--danger)' },
]);

const theme = EditorView.theme({
  '&': {
    height: '100%',
    color: 'var(--text)',
    backgroundColor: 'var(--surface)',
    fontSize: 'var(--text-sm)',
  },
  '.cm-scroller': { fontFamily: 'var(--font-data)', lineHeight: '1.5' },
  '.cm-content': { caretColor: 'var(--text)' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--text)' },
  '.cm-gutters': {
    backgroundColor: 'var(--surface-2)',
    color: 'var(--text-faint)',
    borderRight: '1px solid var(--line)',
  },
  '.cm-activeLine': { backgroundColor: 'var(--hover)' },
  '.cm-activeLineGutter': { backgroundColor: 'var(--press)', color: 'var(--text)' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
    backgroundColor: 'var(--selected)',
  },
  '.cm-selectionMatch': { backgroundColor: 'var(--accent-soft)' },
  '&.cm-focused .cm-matchingBracket': { backgroundColor: 'var(--accent-soft)', outline: 'none' },
  '.cm-panels': { backgroundColor: 'var(--surface-2)', color: 'var(--text)' },
  '.cm-panels.cm-panels-top': { borderBottom: '1px solid var(--line)' },
  '.cm-searchMatch': { backgroundColor: 'var(--warn-soft)' },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'var(--selected)' },
  '.cm-textfield, .cm-button': {
    fontSize: 'var(--text-sm)',
    minHeight: '32px',
    color: 'var(--text)',
    backgroundColor: 'var(--surface)',
    border: '1px solid var(--line-strong)',
    borderRadius: 'var(--radius-sm)',
    backgroundImage: 'none',
  },
});

/** What the component holds on to: the view, and a way to move the cursor. */
export interface Editor {
  goto(line: number, column: number): void;
  measure(): void;
  destroy(): void;
}

export function createEditor(
  parent: HTMLElement,
  doc: string,
  label: string,
  onchange: (text: string) => void,
): Editor {
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc,
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        drawSelection(),
        indentOnInput(),
        bracketMatching(),
        highlightActiveLine(),
        highlightSelectionMatches(),
        search({ top: true }),
        cpp(),
        syntaxHighlighting(highlight),
        EditorState.tabSize.of(2),
        keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab]),
        theme,
        EditorView.contentAttributes.of({ 'aria-label': label }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) onchange(u.state.doc.toString());
        }),
      ],
    }),
  });
  return {
    goto(line, column) {
      const d = view.state.doc;
      const l = d.line(Math.min(Math.max(1, line), d.lines));
      const pos = Math.min(l.from + Math.max(0, column - 1), l.to);
      view.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
      view.focus();
    },
    measure: () => view.requestMeasure(),
    destroy: () => view.destroy(),
  };
}
