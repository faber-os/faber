/**
 * What a name means at a place in the sketch, for completion, hover docs and signature help:
 * the sketch's own names (this tab and its other tabs) first, then the catalogue. Also the type
 * of an object (`sonar` is a `FaberUltrasonic`) and what can follow it after a `.`.
 */
import { cppLanguage } from '@codemirror/lang-cpp';
import { ensureSyntaxTree, syntaxTree } from '@codemirror/language';
import { Facet, type EditorState, type Text } from '@codemirror/state';
import type { Tree } from '@lezer/common';
import type { ApiFunction, Library, Overload } from './catalogue';
import { paramName } from './catalogue';
import { CLASSES, FUNCTIONS, NAMESPACES, OBJECTS, apiMembers } from './knowledge';
import {
  baseType,
  localsAt,
  sketchSymbols,
  stringSource,
  type SketchSymbol,
  type Source,
} from './symbols';

/** Another tab of the sketch. */
export interface SketchFile {
  name: string;
  text: string;
}

/** Which file an editor shows, and how to read the sketch's other tabs. */
export interface SketchContext {
  file: string;
  others: () => SketchFile[];
}

export const sketchContext = Facet.define<SketchContext, SketchContext>({
  combine: (values) => values[0] ?? { file: '', others: () => [] },
});

const knownClass = (name: string) => CLASSES.has(name);

export function docSource(doc: Text): Source {
  return {
    slice: (from, to) => doc.sliceString(from, to),
    lineAt: (pos) => doc.lineAt(Math.min(pos, doc.length)).number,
  };
}

/** The tree of the whole document, parsed now if it isn't yet (at most 100 ms). */
export function fullTree(state: EditorState): Tree {
  return ensureSyntaxTree(state, state.doc.length, 100) ?? syntaxTree(state);
}

const ownSymbols = new WeakMap<Text, SketchSymbol[]>();
const otherSymbols = new Map<string, { text: string; symbols: SketchSymbol[] }>();

function symbolsOfOther(file: SketchFile): SketchSymbol[] {
  const cached = otherSymbols.get(file.name);
  if (cached && cached.text === file.text) return cached.symbols;
  const symbols = sketchSymbols(
    cppLanguage.parser.parse(file.text),
    stringSource(file.text),
    file.name,
    knownClass,
  );
  otherSymbols.set(file.name, { text: file.text, symbols });
  return symbols;
}

/** Every top-level name of the sketch: this tab's first, then the other tabs'. */
export function sketchNames(state: EditorState): SketchSymbol[] {
  const ctx = state.facet(sketchContext);
  let own = ownSymbols.get(state.doc);
  if (!own) {
    own = sketchSymbols(fullTree(state), docSource(state.doc), ctx.file, knownClass);
    ownSymbols.set(state.doc, own);
  }
  return [...own, ...ctx.others().flatMap(symbolsOfOther)];
}

/** The local variables and parameters at `pos`. */
export function localNames(state: EditorState, pos: number): SketchSymbol[] {
  const ctx = state.facet(sketchContext);
  return localsAt(fullTree(state), docSource(state.doc), pos, ctx.file, knownClass);
}

/** A sketch class by name (with its members). */
function sketchClass(state: EditorState, name: string): SketchSymbol | undefined {
  return sketchNames(state).find((s) => s.kind === 'class' && s.name === name);
}

/** The type of a name at `pos`: a local, a global of the sketch, or an Arduino object. */
export function typeOfName(state: EditorState, pos: number, name: string): string | undefined {
  const local = localNames(state, pos).find((s) => s.name === name);
  if (local?.type) return baseType(local.type);
  const global = sketchNames(state).find((s) => s.name === name && s.kind === 'variable');
  if (global?.type) return baseType(global.type);
  return OBJECTS.get(name)?.type;
}

/** Something that can follow `object.`, from the catalogue or from a sketch class. */
export interface Member {
  name: string;
  kind: 'method' | 'field' | 'constant';
  /** One per overload (methods), or the field's declaration. */
  signatures: string[];
  /** Each overload's parameters, for signature help. */
  overloads: Overload[];
  doc?: string;
  library?: Library;
  /** Where a sketch's member is declared. */
  symbol?: SketchSymbol;
}

function fromApi(m: ApiFunction): Member {
  return {
    name: m.name,
    kind: 'method',
    signatures: m.overloads.map((o) => o.signature),
    overloads: m.overloads,
    doc: m.doc,
    library: m.library,
  };
}

/** A sketch function's one overload, from its parameters. */
export function overloadOf(s: SketchSymbol): Overload {
  return { returns: s.type ?? '', params: s.params ?? [], signature: s.signature };
}

function fromSketch(s: SketchSymbol): Member {
  return {
    name: s.name,
    kind: s.kind === 'method' ? 'method' : 'field',
    signatures: [s.signature],
    overloads: s.kind === 'method' ? [overloadOf(s)] : [],
    symbol: s,
  };
}

/** What can follow an object of `type` after `.` or `->`. */
export function membersOfType(state: EditorState, type: string): Member[] {
  if (CLASSES.has(type)) return apiMembers(type).map(fromApi);
  const cls = sketchClass(state, type);
  if (!cls) return [];
  // Methods defined outside the class (`void Arm::lower() {}`) belong to it too.
  const outside = sketchNames(state).filter((s) => s.owner === type);
  const members = [...(cls.members ?? [])];
  for (const s of outside) if (!members.some((m) => m.name === s.name)) members.push(s);
  return members.map(fromSketch);
}

/** What can follow `owner.`, `owner->` or `owner::` at `pos`. */
export function membersAfter(state: EditorState, pos: number, owner: string, op: string): Member[] {
  if (op === '::') {
    const ns = NAMESPACES.get(owner);
    if (ns)
      return ns.map((k) => ({
        name: k.name,
        kind: 'constant',
        signatures: [`${owner}::${k.name}`],
        overloads: [],
        doc: k.doc,
        library: k.library,
      }));
    return membersOfType(state, owner);
  }
  const type = typeOfName(state, pos, owner);
  return type ? membersOfType(state, type) : [];
}

/** The ways to call a function or construct an object, as signature help shows them. */
export function overloadsOf(
  state: EditorState,
  pos: number,
  call: { name: string; owner?: string; op?: string; declaredType?: string },
): Overload[] {
  if (call.declaredType) {
    // `FaberUltrasonic front(` declares an object: its constructors.
    const cls = CLASSES.get(call.declaredType);
    if (cls) return cls.constructors;
    const own = sketchClass(state, call.declaredType);
    if (own) return (own.constructors ?? []).map(overloadOf);
  }
  if (call.owner !== undefined && call.op) {
    return (
      membersAfter(state, pos, call.owner, call.op).find((m) => m.name === call.name)?.overloads ??
      []
    );
  }
  const local = sketchNames(state).filter((s) => s.name === call.name && s.kind === 'function');
  if (local.length > 0) return dedupe(local.map(overloadOf));
  const api = CLASSES.get(call.name);
  if (api) return api.constructors;
  return FUNCTIONS.get(call.name)?.overloads ?? [];
}

/** A prototype and its definition are one function: keep one of each signature. */
function dedupe(list: Overload[]): Overload[] {
  const seen = new Set<string>();
  return list.filter((o) => {
    const key = o.params.join(',');
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** The parameter names of an overload, for a snippet's tab stops. */
export function paramNames(o: Overload): string[] {
  return o.params.filter((p) => p !== '...').map(paramName);
}
