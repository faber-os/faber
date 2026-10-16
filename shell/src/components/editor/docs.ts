/**
 * The short documentation the completion list's info panel and hover show for a name: its
 * signature(s) as code, what it does, and where it comes from (Arduino, the FaberLink library,
 * or a file and line of the sketch).
 */
import { t } from '../../lib/i18n';
import type { Library } from './catalogue';
import type { Member } from './sketchIndex';
import type { ApiThing } from './knowledge';
import type { SketchSymbol } from './symbols';

export interface Doc {
  signatures: string[];
  text?: string;
  library?: Library;
  include?: string;
  /** For the sketch's own names: where it is declared. */
  symbol?: SketchSymbol;
}

export function renderDoc(doc: Doc): HTMLElement {
  const dom = document.createElement('div');
  dom.className = 'cm-doc';
  if (doc.signatures.length > 0) {
    const code = dom.appendChild(document.createElement('div'));
    code.className = 'cm-doc-signature';
    for (const s of doc.signatures) code.appendChild(document.createElement('div')).textContent = s;
  }
  if (doc.text) {
    const p = dom.appendChild(document.createElement('p'));
    p.className = 'cm-doc-text';
    p.textContent = doc.text;
  }
  const source = sourceText(doc);
  if (source) {
    const p = dom.appendChild(document.createElement('p'));
    p.className = 'cm-doc-source';
    p.textContent = source;
  }
  return dom;
}

function sourceText(doc: Doc): string {
  if (doc.symbol) {
    return t('arduino.assist.source.sketch', { place: `${doc.symbol.file}:${doc.symbol.line}` });
  }
  if (!doc.library) return '';
  const from = t(`arduino.assist.source.${doc.library}`);
  return doc.include ? `${from} · ${t('arduino.assist.include', { header: doc.include })}` : from;
}

export function docOfApi(thing: ApiThing): Doc {
  switch (thing.kind) {
    case 'function':
      return {
        signatures: thing.entry.overloads.map((o) => o.signature),
        text: thing.entry.doc,
        library: thing.entry.library,
        include: thing.entry.include,
      };
    case 'class':
      return {
        signatures: [
          `class ${thing.entry.name}`,
          ...thing.entry.constructors.map((c) => c.signature),
        ],
        text: thing.entry.doc,
        library: thing.entry.library,
        include: thing.entry.include,
      };
    case 'object':
      return {
        signatures: [`${thing.entry.type} ${thing.entry.name}`],
        text: thing.entry.doc,
        library: thing.entry.library,
        include: thing.entry.include,
      };
    case 'constant':
      return {
        signatures: [
          thing.entry.namespace
            ? `${thing.entry.namespace}::${thing.entry.name}`
            : thing.entry.name,
        ],
        text: thing.entry.doc,
        library: thing.entry.library,
      };
    case 'type':
      return { signatures: [thing.entry.name], text: thing.entry.doc, library: 'arduino' };
  }
}

export function docOfSymbol(s: SketchSymbol): Doc {
  return { signatures: [s.signature], symbol: s };
}

export function docOfMember(m: Member): Doc {
  if (m.symbol) return docOfSymbol(m.symbol);
  return { signatures: m.signatures, text: m.doc, library: m.library };
}
