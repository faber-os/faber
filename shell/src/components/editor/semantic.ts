/**
 * Colours the C++ grammar can't know: which names are the Arduino API (`pinMode`, `Serial`),
 * Arduino constants (`HIGH`, `A0`, and any ALL_CAPS name, the C++ habit for constants), FaberLink's
 * classes, and brackets by how deeply they nest (three colours in turn). Only the visible part of
 * the document is looked at, so a long sketch costs no more than a short one.
 *
 * The marks carry classes (`cm-code-api`, `cm-bracket-2` …); codemirror.ts gives them their
 * colours, which win over the grammar's.
 */
import { syntaxTree } from '@codemirror/language';
import type { EditorState, Range } from '@codemirror/state';
import {
  Decoration,
  ViewPlugin,
  type DecorationSet,
  type EditorView,
  type ViewUpdate,
} from '@codemirror/view';
import type { SyntaxNode } from '@lezer/common';
import {
  CLASSES,
  CONSTANTS,
  FABERLINK_CLASS_NAMES,
  FUNCTIONS,
  NAMESPACES,
  OBJECTS,
} from './knowledge';

export interface CodeMark {
  from: number;
  to: number;
  className: string;
}

const OPEN = new Set(['(', '[', '{']);
const CLOSE = new Set([')', ']', '}']);
/** Constants by convention: two or more capitals, digits and underscores (`LED_PIN`, `A0`). */
const ALL_CAPS = /^[A-Z][A-Z0-9_]+$/;

/** The opening and closing bracket a node has as direct children, if any. */
interface Brackets {
  open: SyntaxNode;
  close: SyntaxNode | null;
}

/**
 * How many brackets enclose a bracket token, from the syntax tree: each ancestor whose own
 * opening bracket comes before it and whose closing bracket (if it has one yet) comes after.
 * `cache` holds each ancestor's brackets for one pass.
 */
function depthOf(token: SyntaxNode, cache: Map<string, Brackets | null>): number {
  let depth = 0;
  let child = token;
  for (let a = token.parent; a && !a.type.isTop; child = a, a = a.parent) {
    const key = `${a.from}:${a.to}:${a.type.id}`;
    let b = cache.get(key);
    if (b === undefined) {
      b = null;
      for (let c = a.firstChild; c; c = c.nextSibling) {
        if (!b && OPEN.has(c.name)) b = { open: c, close: null };
        else if (b && CLOSE.has(c.name)) b.close = c;
      }
      cache.set(key, b);
    }
    if (b && b.open.to <= child.from && (!b.close || b.close.from >= child.to)) depth++;
  }
  return depth;
}

/**
 * A declaration such as `FaberLink link(Serial);`: a catalogue class's object made with
 * arguments that are names, which the grammar can only read as a function's declaration.
 */
function declaresObject(fd: SyntaxNode, text: (n: SyntaxNode) => string): boolean {
  const decl = fd.parent;
  if (decl?.name !== 'Declaration') return false;
  const type = decl.getChild('TypeIdentifier');
  if (!type || !CLASSES.has(text(type))) return false;
  const params = fd.getChild('ParameterList');
  // A parameter with a type and a name (`int speed`) makes it a function after all.
  for (let p = params?.firstChild ?? null; p; p = p.nextSibling) {
    if (p.name === 'ParameterDeclaration' && p.firstChild?.nextSibling) return false;
  }
  return true;
}

/** The marks for `from`…`to` of the document. */
export function codeMarks(state: EditorState, from: number, to: number): CodeMark[] {
  const out: CodeMark[] = [];
  const cache = new Map<string, Brackets | null>();
  const text = (n: { from: number; to: number }) => state.doc.sliceString(n.from, n.to);
  syntaxTree(state).iterate({
    from,
    to,
    enter(ref) {
      const name = ref.name;
      if (OPEN.has(name) || CLOSE.has(name)) {
        const depth = depthOf(ref.node, cache);
        out.push({ from: ref.from, to: ref.to, className: `cm-bracket-${(depth % 3) + 1}` });
        return;
      }
      if (name === 'Identifier' || name === 'TypeIdentifier' || name === 'MacroName') {
        const word = text(ref);
        const parent = ref.node.parent;
        // `FaberLink link(Serial);` reads to the grammar as a function `link`: it is an object.
        if (parent?.name === 'FunctionDeclarator' && declaresObject(parent, text)) {
          out.push({ from: ref.from, to: ref.to, className: 'cm-code-object' });
          return;
        }
        // `faberlink::DISTANCE_INFRARED`
        if (parent?.name === 'ScopedIdentifier') {
          const ns = parent.getChild('NamespaceIdentifier');
          const list = ns ? NAMESPACES.get(text(ns)) : undefined;
          if (list?.some((k) => k.name === word)) {
            out.push({ from: ref.from, to: ref.to, className: 'cm-code-constant' });
            return;
          }
        }
        let className: string | null = null;
        if (FABERLINK_CLASS_NAMES.has(word)) className = 'cm-code-faberlink';
        else if (FUNCTIONS.has(word) || OBJECTS.has(word)) className = 'cm-code-api';
        else if (CONSTANTS.has(word) || (name !== 'TypeIdentifier' && ALL_CAPS.test(word)))
          className = 'cm-code-constant';
        if (className) out.push({ from: ref.from, to: ref.to, className });
        return;
      }
      if (name === 'NamespaceIdentifier' && NAMESPACES.has(text(ref))) {
        out.push({ from: ref.from, to: ref.to, className: 'cm-code-faberlink' });
        return;
      }
      // `Serial.begin`: a member of an Arduino object is the API too.
      if (name === 'FieldIdentifier') {
        const owner = ref.node.parent?.firstChild;
        if (owner && owner.name === 'Identifier' && OBJECTS.has(text(owner)))
          out.push({ from: ref.from, to: ref.to, className: 'cm-code-api' });
      }
    },
  });
  return out;
}

const markCache = new Map<string, Decoration>();
const mark = (className: string) => {
  let m = markCache.get(className);
  if (!m) markCache.set(className, (m = Decoration.mark({ class: className })));
  return m;
};

function build(view: EditorView): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  for (const { from, to } of view.visibleRanges) {
    for (const m of codeMarks(view.state, from, to))
      ranges.push(mark(m.className).range(m.from, m.to));
  }
  return Decoration.set(ranges, true);
}

export const semanticHighlighting = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = build(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || syntaxTree(u.startState) !== syntaxTree(u.state))
        this.decorations = build(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);
