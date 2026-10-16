/**
 * The sketch's own names, read from the C++ syntax tree (Lezer): its functions, global variables,
 * `#define`s, classes and structs (with their members), enums, typedefs, and the local variables
 * and parameters in scope at a position. Completion, hover and signature help use them next to
 * the catalogue, for the open tab and for the sketch's other tabs.
 *
 * It reads what the parser understood, so it is a good guess, not a compiler: code that doesn't
 * parse yet gives what can be read around it.
 */
import type { SyntaxNode, Tree } from '@lezer/common';

export type SymbolKind =
  | 'function'
  | 'variable'
  | 'define'
  | 'class'
  | 'constant'
  | 'type'
  | 'method'
  | 'field'
  | 'parameter';

export interface SketchSymbol {
  name: string;
  kind: SymbolKind;
  /** The declared type of a variable, field or parameter; the return type of a function. */
  type?: string;
  /** Functions, methods and constructors: the parameters as written (`int speed`). */
  params?: string[];
  /** The line people read: `void drive(int speed)`, `#define LED 13`, `FaberMotor left`. */
  signature: string;
  file: string;
  /** 1-based. */
  line: number;
  /** Classes and structs: their members (public ones only for a class). */
  members?: SketchSymbol[];
  /** Classes and structs: their constructors. */
  constructors?: SketchSymbol[];
  /** A method defined outside its class: `void Motor::go()` belongs to `Motor`. */
  owner?: string;
}

/** Where the text comes from: an editor's document or another tab's string. */
export interface Source {
  slice(from: number, to: number): string;
  /** The 1-based line of a position. */
  lineAt(pos: number): number;
}

/** A Source over a plain string (another tab). */
export function stringSource(text: string): Source {
  let starts: number[] | null = null;
  return {
    slice: (from, to) => text.slice(from, to),
    lineAt(pos) {
      if (!starts) {
        starts = [0];
        for (let i = 0; i < text.length; i++) if (text.charCodeAt(i) === 10) starts.push(i + 1);
      }
      let lo = 0;
      let hi = starts.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (starts[mid]! <= pos) lo = mid;
        else hi = mid - 1;
      }
      return lo + 1;
    },
  };
}

/** Nodes that declare a name, possibly wrapping another declarator (`*p`, `a[4]`, `x = 1`). */
const DECLARATORS = new Set([
  'Identifier',
  'FieldIdentifier',
  'PointerDeclarator',
  'ReferenceDeclarator',
  'ArrayDeclarator',
  'InitDeclarator',
  'FunctionDeclarator',
  'ScopedIdentifier',
  'ParenthesizedDeclarator',
  'DestructorName',
  'OperatorName',
]);

const CLASS_SPECIFIERS = new Set(['ClassSpecifier', 'StructSpecifier', 'UnionSpecifier']);
/** Nodes whose children are declarations of the same scope: `#if` blocks, namespaces. */
const TRANSPARENT =
  /^(Preproc(If|Ifdef|Else|Elif|Elifdef)|NamespaceDefinition|DeclarationList|LinkageSpecification)$/;

const squash = (s: string) => s.replace(/\s+/g, ' ').trim();

class Reader {
  constructor(
    readonly src: Source,
    readonly file: string,
  ) {}

  text(n: SyntaxNode | null | undefined): string {
    return n ? this.src.slice(n.from, n.to) : '';
  }

  line(n: SyntaxNode): number {
    return this.src.lineAt(n.from);
  }

  /** The first declarator child of a declaration. */
  firstDeclarator(n: SyntaxNode): SyntaxNode | null {
    for (let c = n.firstChild; c; c = c.nextSibling) if (DECLARATORS.has(c.name)) return c;
    return null;
  }

  /** The type part of a declaration: everything before its first declarator. */
  typeOf(n: SyntaxNode): string {
    const d = this.firstDeclarator(n);
    return squash(this.src.slice(n.from, d ? d.from : n.to));
  }

  /** The name node a declarator declares, through `*`, `&`, `[]` and `= value`. */
  nameNode(d: SyntaxNode): SyntaxNode | null {
    if (d.name === 'Identifier' || d.name === 'FieldIdentifier' || d.name === 'DestructorName')
      return d;
    if (d.name === 'ScopedIdentifier') {
      // `Motor::go`: the last name.
      let last: SyntaxNode | null = null;
      for (let c = d.firstChild; c; c = c.nextSibling)
        if (c.name === 'Identifier' || c.name === 'DestructorName') last = c;
      return last;
    }
    for (let c = d.firstChild; c; c = c.nextSibling) {
      if (DECLARATORS.has(c.name)) return this.nameNode(c);
    }
    return null;
  }

  /** The FunctionDeclarator inside a declarator, if it declares a function. */
  functionDeclarator(d: SyntaxNode): SyntaxNode | null {
    if (d.name === 'FunctionDeclarator') return d;
    if (
      d.name === 'InitDeclarator' ||
      d.name === 'PointerDeclarator' ||
      d.name === 'ReferenceDeclarator'
    ) {
      for (let c = d.firstChild; c; c = c.nextSibling) {
        if (DECLARATORS.has(c.name)) return this.functionDeclarator(c);
      }
    }
    return null;
  }

  params(fd: SyntaxNode): string[] {
    const list = fd.getChild('ParameterList');
    const out: string[] = [];
    for (let c = list?.firstChild ?? null; c; c = c.nextSibling) {
      if (c.name === '(' || c.name === ')' || c.name === ',' || c.type.isError) continue;
      const p = squash(this.text(c));
      if (p && p !== 'void') out.push(p);
    }
    return out;
  }

  /** `FaberLink link(Serial);` parses as a function `link` taking a `Serial`: is it an object? */
  looksLikeObject(type: string, fd: SyntaxNode, isClass: (name: string) => boolean): boolean {
    if (!isClass(baseType(type))) return false;
    const list = fd.getChild('ParameterList');
    for (let c = list?.firstChild ?? null; c; c = c.nextSibling) {
      // A parameter with a name (`int speed`) makes it a function's declaration.
      if (c.name === 'ParameterDeclaration' && this.firstDeclarator(c)) return false;
    }
    return true;
  }

  functionSymbol(n: SyntaxNode, fd: SyntaxNode, kind: 'function' | 'method'): SketchSymbol | null {
    const head = fd.firstChild;
    const nameNode = head ? this.nameNode(head) : null;
    if (!nameNode) return null;
    let owner: string | undefined;
    if (head?.name === 'ScopedIdentifier') owner = this.text(head.getChild('NamespaceIdentifier'));
    const returns = squash(this.src.slice(n.from, fd.from));
    const declared = squash(this.text(fd));
    return {
      name: this.text(nameNode),
      kind,
      type: returns,
      params: this.params(fd),
      signature: returns ? `${returns} ${declared}` : declared,
      file: this.file,
      line: this.line(n),
      ...(owner ? { owner } : {}),
    };
  }

  /** The names one declaration (`int a = 1, *b;`, `FaberMotor left(…);`) introduces. */
  declaration(
    n: SyntaxNode,
    isClass: (name: string) => boolean,
    variable: SymbolKind = 'variable',
  ): SketchSymbol[] {
    const out: SketchSymbol[] = [];
    const type = this.typeOf(n);
    for (let d = n.firstChild; d; d = d.nextSibling) {
      if (!DECLARATORS.has(d.name)) continue;
      const fd = this.functionDeclarator(d);
      if (fd && !this.looksLikeObject(type, fd, isClass)) {
        const f = this.functionSymbol(n, fd, variable === 'field' ? 'method' : 'function');
        if (f) out.push(f);
        continue;
      }
      const nameNode = this.nameNode(d);
      if (!nameNode) continue;
      const pointer = d.name === 'PointerDeclarator' || d.firstChild?.name === 'PointerDeclarator';
      // The declarator without its `= value`, so a long initialiser isn't the signature.
      const shown = squash(this.text(d)).split(' = ')[0]!;
      out.push({
        name: this.text(nameNode),
        kind: variable,
        type: pointer ? `${type}*` : type,
        signature: `${type} ${shown}`.slice(0, 120),
        file: this.file,
        line: this.line(d),
      });
    }
    return out;
  }

  classSymbol(spec: SyntaxNode, isClass: (name: string) => boolean): SketchSymbol | null {
    const nameNode = spec.getChild('TypeIdentifier');
    if (!nameNode) return null;
    const name = this.text(nameNode);
    const members: SketchSymbol[] = [];
    const constructors: SketchSymbol[] = [];
    let open = spec.name !== 'ClassSpecifier';
    const body = spec.getChild('FieldDeclarationList');
    for (let m = body?.firstChild ?? null; m; m = m.nextSibling) {
      if (m.name === 'AccessSpecifier') {
        open = this.text(m.getChild('Access')) === 'public';
        continue;
      }
      if (!open) continue;
      let found: SketchSymbol[] = [];
      if (m.name === 'FieldDeclaration' || m.name === 'Declaration') {
        found = this.declaration(m, isClass, 'field');
      } else if (m.name === 'FunctionDefinition') {
        const d = this.firstDeclarator(m);
        const fd = d ? this.functionDeclarator(d) : null;
        const f = fd ? this.functionSymbol(m, fd, 'method') : null;
        if (f) found = [f];
      }
      for (const s of found) (s.name === name ? constructors : members).push(s);
    }
    const keyword =
      spec.name === 'ClassSpecifier'
        ? 'class'
        : spec.name === 'UnionSpecifier'
          ? 'union'
          : 'struct';
    return {
      name,
      kind: 'class',
      signature: `${keyword} ${name}`,
      file: this.file,
      line: this.line(spec),
      members,
      constructors,
    };
  }

  enumSymbols(spec: SyntaxNode): SketchSymbol[] {
    const out: SketchSymbol[] = [];
    const nameNode = spec.getChild('TypeIdentifier');
    if (nameNode) {
      const name = this.text(nameNode);
      out.push({
        name,
        kind: 'type',
        signature: `enum ${name}`,
        file: this.file,
        line: this.line(spec),
      });
    }
    const list = spec.getChild('EnumeratorList');
    for (let e = list?.firstChild ?? null; e; e = e.nextSibling) {
      if (e.name !== 'Enumerator') continue;
      const id = e.getChild('Identifier');
      if (!id) continue;
      out.push({
        name: this.text(id),
        kind: 'constant',
        type: nameNode ? this.text(nameNode) : undefined,
        signature: squash(this.text(e)),
        file: this.file,
        line: this.line(e),
      });
    }
    return out;
  }

  define(n: SyntaxNode): SketchSymbol | null {
    if (!n.getChild('#define')) return null;
    const id = n.getChild('Identifier');
    if (!id) return null;
    const firstLine = this.src.slice(n.from, n.to).split('\n')[0]!;
    return {
      name: this.text(id),
      kind: 'define',
      signature: squash(firstLine).slice(0, 120),
      file: this.file,
      line: this.line(n),
    };
  }
}

/** The class a type names: `const FaberMotor&` → `FaberMotor`, `Servo*` → `Servo`. */
export function baseType(type: string): string {
  const words = type
    .replace(/[*&]/g, ' ')
    .split(/\s+/)
    .filter(
      (w) =>
        w &&
        ![
          'const',
          'static',
          'volatile',
          'extern',
          'inline',
          'unsigned',
          'signed',
          'struct',
          'class',
          'mutable',
          'constexpr',
        ].includes(w),
    );
  return words[words.length - 1] ?? '';
}

function classSpecifierIn(n: SyntaxNode): SyntaxNode | null {
  if (CLASS_SPECIFIERS.has(n.name)) return n;
  for (let c = n.firstChild; c; c = c.nextSibling) if (CLASS_SPECIFIERS.has(c.name)) return c;
  return null;
}

function enumSpecifierIn(n: SyntaxNode): SyntaxNode | null {
  if (n.name === 'EnumSpecifier') return n;
  return n.getChild('EnumSpecifier');
}

/** Calls `f` for every top-level item, inside `#if` blocks and namespaces too. */
function eachTopLevel(node: SyntaxNode, f: (n: SyntaxNode) => void): void {
  for (let n = node.firstChild; n; n = n.nextSibling) {
    if (TRANSPARENT.test(n.name)) eachTopLevel(n, f);
    else f(n);
  }
}

/**
 * Everything a file declares at its top level. `knownClass` names the catalogue's classes, so
 * `FaberLink link(Serial);` is read as an object, not as a function.
 */
export function sketchSymbols(
  tree: Tree,
  src: Source,
  file: string,
  knownClass: (name: string) => boolean = () => false,
): SketchSymbol[] {
  const r = new Reader(src, file);
  const classes = new Set<string>();
  eachTopLevel(tree.topNode, (n) => {
    const spec = classSpecifierIn(n);
    const id = spec?.getChild('TypeIdentifier');
    if (id) classes.add(r.text(id));
  });
  const isClass = (name: string) => classes.has(name) || knownClass(name);

  const out: SketchSymbol[] = [];
  eachTopLevel(tree.topNode, (n) => {
    switch (n.name) {
      case 'PreprocDirective': {
        const d = r.define(n);
        if (d) out.push(d);
        return;
      }
      case 'FunctionDefinition': {
        const d = r.firstDeclarator(n);
        const fd = d ? r.functionDeclarator(d) : null;
        const f = fd ? r.functionSymbol(n, fd, 'function') : null;
        if (f) out.push(f.owner ? { ...f, kind: 'method' } : f);
        return;
      }
      case 'TypeDefinition': {
        const ids = n.getChildren('TypeIdentifier');
        const id = ids[ids.length - 1];
        if (id)
          out.push({
            name: r.text(id),
            kind: 'type',
            signature: squash(r.text(n)).replace(/;$/, ''),
            file,
            line: r.line(n),
          });
        return;
      }
    }
    const spec = classSpecifierIn(n);
    if (spec) {
      const c = r.classSymbol(spec, isClass);
      if (c) out.push(c);
    }
    const en = enumSpecifierIn(n);
    if (en) out.push(...r.enumSymbols(en));
    if (n.name === 'Declaration') out.push(...r.declaration(n, isClass));
  });
  return out;
}

/** The local variables and parameters visible at `pos`, nearest first. */
export function localsAt(
  tree: Tree,
  src: Source,
  pos: number,
  file: string,
  knownClass: (name: string) => boolean = () => false,
): SketchSymbol[] {
  const r = new Reader(src, file);
  const out: SketchSymbol[] = [];
  for (let n: SyntaxNode | null = tree.resolveInner(pos, -1); n; n = n.parent) {
    if (n.name === 'CompoundStatement' || n.name === 'ForStatement' || n.name === 'ForRangeLoop') {
      const here: SketchSymbol[] = [];
      for (let c = n.firstChild; c; c = c.nextSibling) {
        if (c.from >= pos) break;
        if (c.name === 'Declaration' && c.to <= pos) here.push(...r.declaration(c, knownClass));
        // `for (int i = 0; …)` has its declaration before the body, ended by its own `;`.
        else if (c.name === 'Declaration' && n.name !== 'CompoundStatement')
          here.push(...r.declaration(c, knownClass));
      }
      out.push(...here.reverse());
    } else if (n.name === 'FunctionDefinition') {
      const d = r.firstDeclarator(n);
      const fd = d ? r.functionDeclarator(d) : null;
      const list = fd?.getChild('ParameterList');
      for (let p = list?.firstChild ?? null; p; p = p.nextSibling) {
        if (!p.name.endsWith('ParameterDeclaration')) continue;
        const pd = r.firstDeclarator(p);
        const id = pd ? r.nameNode(pd) : null;
        if (!id) continue;
        out.push({
          name: r.text(id),
          kind: 'parameter',
          type: r.typeOf(p),
          signature: squash(r.text(p)),
          file,
          line: r.line(p),
        });
      }
    }
  }
  return out.filter((s) => s.kind !== 'function');
}
