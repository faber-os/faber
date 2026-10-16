/**
 * Hover docs: resting the pointer on a name shows what it is. An Arduino or FaberLink function,
 * constant, object or class shows its signature and what it does; a method after `object.` the
 * method of that object's class; the sketch's own names their declaration and where it is.
 */
import { syntaxTree } from '@codemirror/language';
import type { EditorState } from '@codemirror/state';
import { hoverTooltip, type Tooltip } from '@codemirror/view';
import type { SyntaxNode } from '@lezer/common';
import { docOfApi, docOfMember, docOfSymbol, renderDoc, type Doc } from './docs';
import { lookup } from './knowledge';
import { localNames, membersAfter, sketchNames } from './sketchIndex';

const NOT_CODE = new Set([
  'LineComment',
  'BlockComment',
  'String',
  'CharLiteral',
  'RawString',
  'SystemLibString',
  'Number',
]);

/** The documentation for the name at `pos`, and where that name is. */
export function docAt(
  state: EditorState,
  pos: number,
): { from: number; to: number; doc: Doc } | null {
  for (let n: SyntaxNode | null = syntaxTree(state).resolveInner(pos, 1); n; n = n.parent) {
    if (NOT_CODE.has(n.name)) return null;
  }
  const line = state.doc.lineAt(pos);
  const text = line.text;
  let from = pos - line.from;
  let to = from;
  while (from > 0 && /\w/.test(text[from - 1]!)) from--;
  while (to < text.length && /\w/.test(text[to]!)) to++;
  if (from === to || /^\d/.test(text[from]!)) return null;
  const name = text.slice(from, to);
  const at = { from: line.from + from, to: line.from + to };

  const member = /([A-Za-z_]\w*)\s*(\.|->|::)\s*$/.exec(text.slice(0, from));
  if (member) {
    const m = membersAfter(state, at.from, member[1]!, member[2]!).find((x) => x.name === name);
    return m ? { ...at, doc: docOfMember(m) } : null;
  }

  const local = localNames(state, at.from).find((s) => s.name === name);
  if (local) return { ...at, doc: docOfSymbol(local) };
  const own = sketchNames(state).filter((s) => s.name === name && s.kind !== 'method');
  // A function's definition says more than its prototype (both have the same signature).
  const best = own.find((s) => s.kind !== 'function') ?? own[own.length - 1];
  if (best) return { ...at, doc: docOfSymbol(best) };
  const api = lookup(name);
  return api ? { ...at, doc: docOfApi(api) } : null;
}

export const hoverDocs = hoverTooltip(
  (view, pos): Tooltip | null => {
    const found = docAt(view.state, pos);
    if (!found) return null;
    return {
      pos: found.from,
      end: found.to,
      above: true,
      create: () => ({ dom: renderDoc(found.doc) }),
    };
  },
  { hoverTime: 350 },
);
