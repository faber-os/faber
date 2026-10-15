/**
 * Safe minimal Markdown, for a package's README in the install dialog. The text becomes plain
 * blocks and spans that a component draws as elements; nothing is ever read as HTML, so a README
 * can't run script or load anything. Supported: headings, paragraphs, lists, code blocks, simple
 * tables, quotes, `code`, **bold**, *emphasis* and [links](https://…) (http and https only).
 */

export type Inline =
  | { kind: 'text' | 'code' | 'strong' | 'em'; text: string }
  | { kind: 'link'; text: string; href: string };

export type Block =
  | { kind: 'heading'; level: number; inline: Inline[] }
  | { kind: 'paragraph'; inline: Inline[] }
  | { kind: 'quote'; inline: Inline[] }
  | { kind: 'list'; ordered: boolean; items: Inline[][] }
  | { kind: 'code'; text: string }
  | { kind: 'table'; rows: Inline[][][] };

const HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/;
const BULLET = /^\s*[-*+]\s+(.*)$/;
const NUMBERED = /^\s*\d+[.)]\s+(.*)$/;
const FENCE = /^\s*(```|~~~)/;
const TABLE_SEPARATOR = /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/;

export function parseMarkdown(text: string): Block[] {
  const lines = text.replace(/\r\n?/g, '\n').split('\n');
  const blocks: Block[] = [];
  let i = 0;
  const startsBlock = (line: string) =>
    line.trim() === '' ||
    HEADING.test(line) ||
    BULLET.test(line) ||
    NUMBERED.test(line) ||
    FENCE.test(line) ||
    line.trimStart().startsWith('|') ||
    line.trimStart().startsWith('>');

  while (i < lines.length) {
    const line = lines[i]!;
    if (line.trim() === '') {
      i++;
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      blocks.push({ kind: 'heading', level: heading[1]!.length, inline: parseInline(heading[2]!) });
      i++;
      continue;
    }
    if (FENCE.test(line)) {
      const code: string[] = [];
      i++;
      while (i < lines.length && !FENCE.test(lines[i]!)) code.push(lines[i++]!);
      i++; // the closing fence, if there is one
      blocks.push({ kind: 'code', text: code.join('\n') });
      continue;
    }
    if (BULLET.test(line) || NUMBERED.test(line)) {
      const ordered = NUMBERED.test(line);
      const marker = ordered ? NUMBERED : BULLET;
      const items: string[] = [];
      while (i < lines.length && marker.test(lines[i]!)) {
        let item = marker.exec(lines[i]!)![1]!;
        i++;
        // Indented lines continue the item.
        while (
          i < lines.length &&
          /^\s{2,}\S/.test(lines[i]!) &&
          !BULLET.test(lines[i]!) &&
          !NUMBERED.test(lines[i]!)
        ) {
          item += ' ' + lines[i]!.trim();
          i++;
        }
        items.push(item);
      }
      blocks.push({ kind: 'list', ordered, items: items.map(parseInline) });
      continue;
    }
    if (line.trimStart().startsWith('|')) {
      const rows: Inline[][][] = [];
      while (i < lines.length && lines[i]!.trimStart().startsWith('|')) {
        const row = lines[i]!;
        i++;
        if (TABLE_SEPARATOR.test(row)) continue;
        const cells = row.trim().replace(/^\|/, '').replace(/\|$/, '').split('|');
        rows.push(cells.map((c) => parseInline(c.trim())));
      }
      blocks.push({ kind: 'table', rows });
      continue;
    }
    if (line.trimStart().startsWith('>')) {
      const quoted: string[] = [];
      while (i < lines.length && lines[i]!.trimStart().startsWith('>')) {
        quoted.push(lines[i]!.trimStart().replace(/^>\s?/, ''));
        i++;
      }
      blocks.push({ kind: 'quote', inline: parseInline(quoted.join(' ')) });
      continue;
    }
    const paragraph = [line.trim()];
    i++;
    while (i < lines.length && !startsBlock(lines[i]!)) paragraph.push(lines[i++]!.trim());
    blocks.push({ kind: 'paragraph', inline: parseInline(paragraph.join(' ')) });
  }
  return blocks;
}

/** `[text](url)`, `` `code` ``, `**strong**`, `*em*` or `_em_`, in the order they appear. */
const SPAN =
  /`([^`]+)`|\*\*([^*]+)\*\*|\[([^\]]+)\]\(((?:[^()\s]|\([^()\s]*\))+)\)|(?<![\w*])\*([^*\s][^*]*?)\*(?![\w*])|(?<!\w)_([^_\s][^_]*?)_(?!\w)/g;

export function parseInline(text: string): Inline[] {
  const out: Inline[] = [];
  const pushText = (t: string) => {
    if (!t) return;
    const last = out.at(-1);
    if (last?.kind === 'text') last.text += t;
    else out.push({ kind: 'text', text: t });
  };
  let at = 0;
  for (const m of text.matchAll(SPAN)) {
    pushText(text.slice(at, m.index));
    at = m.index + m[0].length;
    if (m[1] !== undefined) out.push({ kind: 'code', text: m[1] });
    else if (m[2] !== undefined) out.push({ kind: 'strong', text: m[2] });
    else if (m[3] !== undefined) {
      const href = safeLink(m[4]!);
      if (href) out.push({ kind: 'link', text: m[3], href });
      else pushText(m[3]);
    } else out.push({ kind: 'em', text: (m[5] ?? m[6])! });
  }
  pushText(text.slice(at));
  return out;
}

/** The address if it is an ordinary web link (http or https), else null. */
export function safeLink(href: string): string | null {
  return /^https?:\/\//i.test(href) ? href : null;
}
