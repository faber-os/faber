/**
 * CodeMirror 6, set up for the Arduino editor (docs/arduino-editor.md): C++ highlighting with
 * the Arduino API, constants, FaberLink and bracket depth in their own colours, completion,
 * signature help and hover docs from the sketch and the catalogue (editor/), the compiler's
 * errors in the code, line numbers, search (Ctrl+F), undo and bracket matching. Loaded only when
 * an editor opens (CodeEditor.svelte imports it dynamically), so the rest of the shell carries
 * none of it, the catalogue included.
 */
import { EditorState, Prec } from '@codemirror/state';
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
import { acceptCompletion, autocompletion } from '@codemirror/autocomplete';
import { lintGutter, lintKeymap, setDiagnostics } from '@codemirror/lint';
import { cpp } from '@codemirror/lang-cpp';
import { tags } from '@lezer/highlight';
import { t } from '../lib/i18n';
import { arduinoCompletion } from './editor/completion';
import {
  clearEditedDiagnostics,
  toLintDiagnostics,
  type CompilerMessage,
} from './editor/diagnostics';
import { hoverDocs } from './editor/hover';
import { semanticHighlighting } from './editor/semantic';
import { signatureHelp } from './editor/signature';
import { sketchContext, type SketchFile } from './editor/sketchIndex';

export type { CompilerMessage, SketchFile };

// Colours from the shell's own tokens (styles.ts, CODE_COLOURS), so the code follows the style,
// light and dark like everything else. Each is checked as text on the editor's background.
const highlight = HighlightStyle.define([
  {
    tag: [
      tags.keyword,
      tags.controlKeyword,
      tags.operatorKeyword,
      tags.definitionKeyword,
      tags.modifier,
      tags.self,
    ],
    color: 'var(--code-keyword)',
  },
  {
    tag: [tags.typeName, tags.standard(tags.typeName), tags.namespace],
    color: 'var(--code-type)',
  },
  {
    tag: [
      tags.function(tags.variableName),
      tags.function(tags.propertyName),
      tags.function(tags.definition(tags.variableName)),
    ],
    color: 'var(--code-function)',
  },
  { tag: [tags.propertyName, tags.labelName], color: 'var(--code-property)' },
  {
    tag: [tags.number, tags.bool, tags.null, tags.special(tags.name), tags.escape],
    color: 'var(--code-constant)',
  },
  {
    tag: [tags.string, tags.character, tags.special(tags.string)],
    color: 'var(--code-string)',
  },
  // `#include`, `#define`: the directive; what follows it keeps its own colour.
  { tag: tags.processingInstruction, color: 'var(--code-macro)' },
  {
    tag: [
      tags.operator,
      tags.arithmeticOperator,
      tags.logicOperator,
      tags.bitwiseOperator,
      tags.compareOperator,
      tags.updateOperator,
      tags.definitionOperator,
      tags.derefOperator,
      tags.separator,
      tags.punctuation,
    ],
    color: 'var(--text-muted)',
  },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment],
    color: 'var(--text-muted)',
    fontStyle: 'italic',
  },
  { tag: tags.invalid, color: 'var(--danger-text)' },
]);

/** A class's colour, also on the grammar's span inside it (semantic.ts marks wrap them). */
const colour = (value: string, extra: Record<string, string> = {}) => ({ color: value, ...extra });

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
  '&.cm-focused .cm-nonmatchingBracket': {
    backgroundColor: 'var(--danger-soft)',
    outline: 'none',
  },
  '.cm-panels': { backgroundColor: 'var(--surface-2)', color: 'var(--text)' },
  '.cm-panels.cm-panels-top': { borderBottom: '1px solid var(--line)' },
  '.cm-panels.cm-panels-bottom': { borderTop: '1px solid var(--line)' },
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

  // What the grammar can't know (editor/semantic.ts), over the grammar's colours.
  '.cm-code-api, .cm-code-api *': colour('var(--code-api)'),
  '.cm-code-constant, .cm-code-constant *': colour('var(--code-constant)'),
  '.cm-code-faberlink, .cm-code-faberlink *': colour('var(--code-type)', { fontWeight: '600' }),
  '.cm-code-object, .cm-code-object *': colour('var(--text)'),
  '.cm-bracket-1, .cm-bracket-1 *': colour('var(--code-bracket-1)'),
  '.cm-bracket-2, .cm-bracket-2 *': colour('var(--code-bracket-2)'),
  '.cm-bracket-3, .cm-bracket-3 *': colour('var(--code-bracket-3)'),

  // Tooltips: completion, its info panel, hover docs, signature help and errors.
  '.cm-tooltip': {
    backgroundColor: 'var(--menu)',
    color: 'var(--text)',
    border: '1px solid var(--line-strong)',
    borderRadius: 'var(--radius-sm)',
    boxShadow: 'var(--shadow-pop)',
    fontFamily: 'var(--font-ui)',
    fontSize: 'var(--text-sm)',
  },
  '.cm-tooltip.cm-tooltip-autocomplete > ul': {
    fontFamily: 'var(--font-data)',
    maxHeight: '15em',
    minWidth: '16em',
  },
  '.cm-tooltip.cm-tooltip-autocomplete > ul > li': {
    padding: '2px 10px 2px 4px',
    lineHeight: '1.6',
  },
  '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    backgroundColor: 'var(--selected)',
    color: 'var(--text)',
  },
  '.cm-completionLabel': { color: 'var(--text)' },
  '.cm-completionMatchedText': {
    textDecoration: 'none',
    fontWeight: '700',
    color: 'var(--accent-ink)',
  },
  '.cm-completionDetail': {
    color: 'var(--text-muted)',
    fontStyle: 'normal',
    marginLeft: '0.75em',
  },
  '.cm-completionIcon': {
    opacity: '1',
    width: '1.4em',
    paddingRight: '0.4em',
    textAlign: 'center',
    color: 'var(--text-muted)',
  },
  '.cm-completionIcon-function, .cm-completionIcon-method': colour('var(--code-function)'),
  '.cm-completionIcon-constant': colour('var(--code-constant)'),
  '.cm-completionIcon-class, .cm-completionIcon-type, .cm-completionIcon-namespace':
    colour('var(--code-type)'),
  '.cm-completionIcon-variable, .cm-completionIcon-property': colour('var(--code-property)'),
  '.cm-completionIcon-keyword': colour('var(--code-keyword)'),
  '.cm-completionIcon-snippet:after': { content: "'▸'" },
  '.cm-tooltip.cm-completionInfo': { padding: '0', maxWidth: '30em' },
  '.cm-doc': { padding: '8px 10px', maxWidth: '34em' },
  '.cm-doc-signature': {
    fontFamily: 'var(--font-data)',
    whiteSpace: 'pre-wrap',
    color: 'var(--text)',
  },
  '.cm-doc-text': { margin: '6px 0 0', lineHeight: '1.45' },
  '.cm-doc-source': {
    margin: '6px 0 0',
    color: 'var(--text-muted)',
    fontSize: 'var(--text-xs)',
  },
  '.cm-signature': { padding: '4px 8px', fontFamily: 'var(--font-data)' },
  '.cm-signature-name': colour('var(--code-function)'),
  '.cm-signature-active': { fontWeight: '700', color: 'var(--accent-ink)' },

  // The compiler's errors and warnings (editor/diagnostics.ts).
  '.cm-lintRange-error, .cm-lintRange-warning': {
    backgroundImage: 'none',
    textDecorationLine: 'underline',
    textDecorationStyle: 'wavy',
    textDecorationSkipInk: 'none',
    textUnderlineOffset: '3px',
  },
  '.cm-lintRange-error': { textDecorationColor: 'var(--danger)' },
  '.cm-lintRange-warning': { textDecorationColor: 'var(--warn)' },
  '.cm-lintPoint:after': { borderBottomColor: 'var(--danger)' },
  '.cm-lintPoint-warning:after': { borderBottomColor: 'var(--warn)' },
  '.cm-diagnostic': {
    padding: '6px 10px',
    fontFamily: 'var(--font-data)',
    whiteSpace: 'pre-wrap',
  },
  '.cm-diagnostic-error': { borderLeft: '4px solid var(--danger)' },
  '.cm-diagnostic-warning': { borderLeft: '4px solid var(--warn)' },
  '.cm-gutter-lint': { width: '1.2em' },
  '.cm-gutter-lint .cm-gutterElement': { padding: '0.35em 0.2em' },
  '.cm-lint-marker': { width: '0.8em', height: '0.8em' },
  '.cm-lint-marker-error': {
    content: 'normal',
    backgroundColor: 'var(--danger)',
    borderRadius: '50%',
  },
  '.cm-lint-marker-warning': {
    content: 'normal',
    backgroundColor: 'var(--warn)',
    clipPath: 'polygon(50% 0, 100% 100%, 0 100%)',
  },
  '.cm-panel.cm-panel-lint ul [aria-selected], .cm-panel.cm-panel-lint ul:focus [aria-selected]': {
    backgroundColor: 'var(--selected)',
    color: 'var(--text)',
  },
});

/** What the component holds on to: the view, and ways to move the cursor and show errors. */
export interface Editor {
  goto(line: number, column: number): void;
  /** The compiler's messages for this tab, underlined (an empty list clears them). */
  setDiagnostics(messages: readonly CompilerMessage[]): void;
  measure(): void;
  destroy(): void;
}

export interface EditorOptions {
  /** For screen readers: which file this is. */
  label: string;
  /** The tab's file name (`Rover.ino`), for where its names are declared. */
  file: string;
  /** The sketch's other tabs, read when completion or hover needs their names. */
  others: () => SketchFile[];
  onchange: (text: string) => void;
}

export function createEditor(parent: HTMLElement, doc: string, options: EditorOptions): Editor {
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc,
      extensions: [
        lintGutter(),
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
        semanticHighlighting,
        sketchContext.of({ file: options.file, others: options.others }),
        autocompletion({ override: [arduinoCompletion], icons: true }),
        signatureHelp(),
        hoverDocs,
        clearEditedDiagnostics,
        EditorState.tabSize.of(2),
        // Tab accepts a completion when the list is open (also inside a snippet's parameter,
        // before Tab moves on to the next one), and indents otherwise.
        Prec.highest(keymap.of([{ key: 'Tab', run: acceptCompletion }])),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...searchKeymap,
          ...lintKeymap,
          indentWithTab,
        ]),
        EditorState.phrases.of({
          Completions: t('arduino.assist.completions'),
          Diagnostics: t('arduino.assist.diagnostics'),
          'No diagnostics': t('arduino.assist.noDiagnostics'),
          close: t('arduino.assist.close'),
        }),
        theme,
        EditorView.contentAttributes.of({ 'aria-label': options.label }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) options.onchange(u.state.doc.toString());
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
    setDiagnostics(messages) {
      view.dispatch(setDiagnostics(view.state, toLintDiagnostics(view.state.doc, messages)));
    },
    measure: () => view.requestMeasure(),
    destroy: () => view.destroy(),
  };
}
