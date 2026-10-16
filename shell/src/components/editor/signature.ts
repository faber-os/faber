/**
 * Signature help: inside a call (`digitalWrite(13, |`), a small tooltip above the cursor shows
 * the function's parameters with the one being typed in bold. It also shows the constructor of
 * an object being declared (`FaberUltrasonic front(|`). Escape hides it until the next edit.
 */
import { syntaxTree } from '@codemirror/language';
import { StateEffect, StateField, type EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap, showTooltip, type Tooltip } from '@codemirror/view';
import type { SyntaxNode } from '@lezer/common';
import type { Overload } from './catalogue';
import { CLASSES } from './knowledge';
import { overloadsOf, sketchNames } from './sketchIndex';

export interface CallSite {
  name: string;
  /** `Serial` in `Serial.print(`. */
  owner?: string;
  op?: '.' | '->' | '::';
  /** `FaberUltrasonic` in `FaberUltrasonic front(`: a declaration, not a call. */
  declaredType?: string;
  /** Which argument the cursor is in, from 0. */
  argIndex: number;
  /** Where the `(` is. */
  open: number;
}

/** Text the scan skips: brackets and commas in strings and comments don't count. */
const QUOTED = new Set([
  'String',
  'CharLiteral',
  'RawString',
  'SystemLibString',
  'LineComment',
  'BlockComment',
  'PreprocArg',
]);
const NOT_CALLS = new Set([
  'if',
  'for',
  'while',
  'switch',
  'return',
  'sizeof',
  'catch',
  'defined',
  'else',
  'do',
  'case',
  'new',
  'delete',
]);
/** How far back the scan looks for the call's `(`. */
const REACH = 2000;

function quoted(state: EditorState, pos: number): boolean {
  for (let n: SyntaxNode | null = syntaxTree(state).resolveInner(pos, 1); n; n = n.parent) {
    if (QUOTED.has(n.name)) return true;
  }
  return false;
}

/** The call whose arguments `pos` is in, if any. */
export function callAt(state: EditorState, pos: number): CallSite | null {
  const start = Math.max(0, pos - REACH);
  const text = state.doc.sliceString(start, pos);
  let depth = 0;
  let commas = 0;
  for (let i = text.length - 1; i >= 0; i--) {
    const ch = text[i]!;
    if (!'()[]{},;'.includes(ch) || quoted(state, start + i)) continue;
    if (ch === ')' || ch === ']' || ch === '}') depth++;
    else if (ch === '(' || ch === '[' || ch === '{') {
      if (depth > 0) {
        depth--;
        continue;
      }
      // Directly inside [] or {}: not an argument list.
      if (ch !== '(') return null;
      return site(text.slice(0, i), start + i, commas);
    } else if (depth === 0 && ch === ',') commas++;
    else if (depth === 0 && ch === ';') return null;
  }
  return null;
}

const NAME_BEFORE = /(?:([A-Za-z_]\w*)\s*(\.|->|::)\s*|([A-Za-z_][\w:]*)\s+)?([A-Za-z_]\w*)\s*$/;

function site(before: string, open: number, argIndex: number): CallSite | null {
  const m = NAME_BEFORE.exec(before);
  if (!m) return null;
  const name = m[4]!;
  if (NOT_CALLS.has(name)) return null;
  const call: CallSite = { name, argIndex, open };
  if (m[1]) {
    call.owner = m[1];
    call.op = m[2] as CallSite['op'];
  } else if (m[3] && !NOT_CALLS.has(m[3])) {
    // `Type name(`: an object with constructor arguments, or a function being declared.
    call.declaredType = m[3];
  }
  return call;
}

/** The overloads that can take the argument being typed (all of them if none can). */
export function fitting(overloads: Overload[], argIndex: number): Overload[] {
  const fit = overloads.filter(
    (o) => o.params.length > argIndex || o.params.some((p) => p.includes('...')),
  );
  return fit.length > 0 ? fit : overloads;
}

function render(name: string, overloads: Overload[], argIndex: number): HTMLElement {
  const dom = document.createElement('div');
  dom.className = 'cm-signature';
  for (const o of overloads.slice(0, 4)) {
    const row = dom.appendChild(document.createElement('div'));
    row.className = 'cm-signature-row';
    if (o.returns) row.append(`${o.returns} `);
    const fname = row.appendChild(document.createElement('span'));
    fname.className = 'cm-signature-name';
    fname.textContent = name;
    row.append('(');
    o.params.forEach((p, i) => {
      if (i > 0) row.append(', ');
      const span = row.appendChild(document.createElement('span'));
      span.textContent = p;
      if (i === argIndex) span.className = 'cm-signature-active';
    });
    row.append(')');
  }
  return dom;
}

/** The tooltip for the cursor's position, if it is in a call the editor knows. */
export function signatureTooltip(state: EditorState): Tooltip | null {
  // Also while a parameter's placeholder is selected (after completing a function).
  const sel = state.selection.main;
  const call = callAt(state, sel.head);
  if (!call) return null;
  // `void drive(`: a function's declaration, not a call.
  if (
    call.declaredType &&
    !CLASSES.has(call.declaredType) &&
    !isSketchClass(state, call.declaredType)
  )
    return null;
  const overloads = overloadsOf(state, sel.head, call);
  if (overloads.length === 0) return null;
  const shown = fitting(overloads, call.argIndex);
  const name = call.declaredType ?? call.name;
  return {
    pos: call.open,
    above: true,
    strictSide: false,
    arrow: false,
    create: () => ({ dom: render(name, shown, call.argIndex) }),
  };
}

function isSketchClass(state: EditorState, name: string): boolean {
  return sketchNames(state).some((s) => s.kind === 'class' && s.name === name);
}

const dismiss = StateEffect.define<null>();

/** Hidden by Escape until the document changes. */
const dismissed = StateField.define<boolean>({
  create: () => false,
  update(value, tr) {
    if (tr.effects.some((e) => e.is(dismiss))) return true;
    return tr.docChanged ? false : value;
  },
});

export function signatureHelp(): Extension {
  return [
    dismissed,
    showTooltip.compute(['doc', 'selection', dismissed], (state) =>
      state.field(dismissed) ? null : signatureTooltip(state),
    ),
    keymap.of([
      {
        key: 'Escape',
        run(view: EditorView) {
          if (view.state.field(dismissed) || !signatureTooltip(view.state)) return false;
          view.dispatch({ effects: dismiss.of(null) });
          return true;
        },
      },
    ]),
  ];
}
