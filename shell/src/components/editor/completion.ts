/**
 * Code completion, all offline: the sketch's own names (locals first, then this tab's and the
 * other tabs' functions, globals, #defines, classes), the Arduino API and FaberLink from the
 * catalogue, C++ keywords and snippets. After `object.` it offers that object's methods
 * (`Serial.`, `Wire.`, a FaberLink part declared in the sketch, a sketch's own struct), after
 * `faberlink::` the library's constants, after `#` the directives and after `#include <` the
 * libraries.
 *
 * It opens by itself after two letters of a name, or after `.`, `->` and `::`; Ctrl+Space opens
 * it anywhere. Accepting a function writes its call with a tab stop for each parameter.
 */
import {
  snippet,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete';
import { syntaxTree } from '@codemirror/language';
import type { EditorView } from '@codemirror/view';
import type { SyntaxNode } from '@lezer/common';
import { t } from '../../lib/i18n';
import {
  ARDUINO_CLASSES,
  ARDUINO_CONSTANTS,
  ARDUINO_FUNCTIONS,
  ARDUINO_OBJECTS,
  ARDUINO_TYPES,
  CPP_KEYWORDS,
  DIRECTIVES,
  HEADERS,
  SNIPPETS,
} from './arduinoApi';
import type { ApiFunction, Overload } from './catalogue';
import { FABERLINK_CLASSES } from './faberlinkApi';
import { docOfApi, docOfMember, docOfSymbol, renderDoc, type Doc } from './docs';
import {
  localNames,
  membersAfter,
  overloadOf,
  paramNames,
  sketchNames,
  type Member,
} from './sketchIndex';
import type { SketchSymbol } from './symbols';

/** Ranks: a local beats a sketch name beats the catalogue beats a keyword. */
const BOOST = { local: 3, sketch: 2, api: 1, snippet: 1, keyword: 0 };
/** Letters typed before the list opens by itself. */
const MIN_TYPED = 2;
const NOT_CODE = new Set(['LineComment', 'BlockComment', 'String', 'CharLiteral', 'RawString']);

const info = (doc: Doc) => () => renderDoc(doc);

/** `(uint8_t pin, uint8_t mode)`, and how many more ways there are to call it. */
function detailOf(overloads: Overload[]): string {
  if (overloads.length === 0) return '';
  const first = `(${overloads[0]!.params.join(', ')})`;
  return overloads.length > 1 ? `${first} +${overloads.length - 1}` : first;
}

/**
 * Writes `name(…)` with a tab stop per parameter (one empty stop when there are several ways to
 * call it, signature help then shows them), or only the name when a `(` follows already.
 */
function callApply(name: string, overloads: Overload[]) {
  const params = overloads.length === 1 ? paramNames(overloads[0]!) : null;
  const template =
    params === null ? `${name}(\${})` : `${name}(${params.map((p) => `\${${p}}`).join(', ')})`;
  const insert = snippet(template);
  return (view: EditorView, completion: Completion, from: number, to: number) => {
    if (view.state.sliceDoc(to, to + 1) === '(') {
      view.dispatch({
        changes: { from, to, insert: name },
        selection: { anchor: from + name.length + 1 },
      });
    } else insert(view, completion, from, to);
  };
}

function functionOption(f: ApiFunction): Completion {
  return {
    label: f.name,
    type: 'function',
    detail: detailOf(f.overloads),
    info: info(docOfApi({ kind: 'function', entry: f })),
    apply: callApply(f.name, f.overloads),
    boost: BOOST.api,
  };
}

/** The catalogue's names (made each time the list opens, so they follow a language change). */
function catalogueOptions(): Completion[] {
  return [
    ...ARDUINO_FUNCTIONS.map(functionOption),
    ...ARDUINO_CONSTANTS.map((c): Completion => ({
      label: c.name,
      type: 'constant',
      info: info(docOfApi({ kind: 'constant', entry: c })),
      boost: BOOST.api,
    })),
    ...ARDUINO_OBJECTS.map((o): Completion => ({
      label: o.name,
      type: 'variable',
      detail: o.type,
      info: info(docOfApi({ kind: 'object', entry: o })),
      boost: BOOST.api,
    })),
    ...[...ARDUINO_CLASSES, ...FABERLINK_CLASSES].map((c): Completion => ({
      label: c.name,
      type: 'class',
      detail: c.library === 'faberlink' ? t('arduino.assist.source.faberlink') : undefined,
      info: info(docOfApi({ kind: 'class', entry: c })),
      boost: BOOST.api,
    })),
    ...ARDUINO_TYPES.map((ty): Completion => ({
      label: ty.name,
      type: 'type',
      info: info(docOfApi({ kind: 'type', entry: ty })),
      boost: BOOST.api,
    })),
    ...CPP_KEYWORDS.map((k): Completion => ({
      label: k,
      type: 'keyword',
      detail: t('arduino.assist.kind.keyword'),
      boost: BOOST.keyword,
    })),
    ...SNIPPETS.map((s): Completion => ({
      label: s.label,
      type: 'snippet',
      detail: t(s.key),
      apply: snippet(s.template),
      boost: BOOST.snippet,
    })),
  ];
}

const SYMBOL_TYPE: Record<SketchSymbol['kind'], string> = {
  function: 'function',
  method: 'method',
  variable: 'variable',
  parameter: 'variable',
  field: 'property',
  define: 'constant',
  constant: 'constant',
  class: 'class',
  type: 'type',
};

function symbolOption(s: SketchSymbol, boost: number): Completion {
  const isFunction = s.kind === 'function' || s.kind === 'method';
  return {
    label: s.name,
    type: SYMBOL_TYPE[s.kind],
    detail: isFunction
      ? `(${(s.params ?? []).join(', ')})`
      : s.kind === 'define'
        ? s.signature.replace(/^#define\s+\S+\s*/, '')
        : s.type,
    info: info(docOfSymbol(s)),
    ...(isFunction ? { apply: callApply(s.name, [overloadOf(s)]) } : {}),
    boost,
  };
}

function memberOption(m: Member): Completion {
  const method = m.kind === 'method';
  return {
    label: m.name,
    type: m.kind === 'constant' ? 'constant' : method ? 'method' : 'property',
    detail: method ? detailOf(m.overloads) : m.symbol?.type,
    info: info(docOfMember(m)),
    ...(method ? { apply: callApply(m.name, m.overloads) } : {}),
    boost: BOOST.api,
  };
}

/** The sketch's names at `pos`, without repeats (a prototype and its definition are one). */
function sketchOptions(context: CompletionContext): Completion[] {
  const seen = new Set<string>();
  const out: Completion[] = [];
  for (const s of localNames(context.state, context.pos)) {
    if (seen.has(s.name)) continue;
    seen.add(s.name);
    out.push(symbolOption(s, BOOST.local));
  }
  for (const s of sketchNames(context.state)) {
    if (seen.has(s.name) || s.kind === 'method') continue;
    seen.add(s.name);
    out.push(symbolOption(s, BOOST.sketch));
  }
  return out;
}

function inPlainCode(context: CompletionContext): boolean {
  for (
    let n: SyntaxNode | null = syntaxTree(context.state).resolveInner(context.pos, -1);
    n;
    n = n.parent
  ) {
    if (NOT_CODE.has(n.name)) return false;
  }
  return true;
}

export function arduinoCompletion(context: CompletionContext): CompletionResult | null {
  const line = context.state.doc.lineAt(context.pos);
  const before = line.text.slice(0, context.pos - line.from);

  // `#include <Wi|`
  const header = /^\s*#\s*include\s*([<"])([\w./]*)$/.exec(before);
  if (header) {
    const close = header[1] === '<' ? '>' : '"';
    const closed = context.state.sliceDoc(context.pos, context.pos + 1) === close;
    return {
      from: context.pos - header[2]!.length,
      options: HEADERS.map((h) => ({
        label: h,
        type: 'namespace',
        detail: t('arduino.assist.kind.header'),
        apply: closed ? h : `${h}${close}`,
      })),
      validFor: /^[\w./]*$/,
    };
  }

  if (!inPlainCode(context)) return null;
  // The text of `#error …`, `#warning …` and `#pragma …` is words, not code.
  if (/^\s*#\s*(error|warning|pragma)\s/.test(before)) return null;

  // `#incl|`
  const directive = /^\s*#\s*(\w*)$/.exec(before);
  if (directive) {
    return {
      from: context.pos - directive[1]!.length,
      options: DIRECTIVES.map((d) => ({
        label: d,
        type: 'keyword',
        detail: t('arduino.assist.kind.directive'),
        apply: d === 'include' ? 'include <' : d,
      })),
      validFor: /^\w*$/,
    };
  }

  // `sonar.|`, `Serial.pr|`, `faberlink::|`
  const member = context.matchBefore(/([A-Za-z_]\w*)\s*(\.|->|::)\s*\w*$/);
  if (member) {
    const m = /([A-Za-z_]\w*)\s*(\.|->|::)\s*(\w*)$/.exec(member.text)!;
    const members = membersAfter(context.state, member.from, m[1]!, m[2]!);
    if (members.length === 0) return null;
    return {
      from: context.pos - m[3]!.length,
      options: members.map(memberOption),
      validFor: /^\w*$/,
    };
  }

  const word = context.matchBefore(/[A-Za-z_]\w*/);
  if (!word && !context.explicit) return null;
  if (word && word.to - word.from < MIN_TYPED && !context.explicit) return null;
  // Right after a digit: a number (`12`, `0x1F`), not a name.
  const from = word ? word.from : context.pos;
  if (/\w/.test(context.state.sliceDoc(from - 1, from))) return null;
  return {
    from,
    options: [...sketchOptions(context), ...catalogueOptions()],
    validFor: /^\w*$/,
  };
}
