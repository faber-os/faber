/** A piece of a text with emphasis: plain, `<b>bold</b>` or `<code>code</code>`. */
export interface RichPart {
  text: string;
  tag: 'b' | 'code' | null;
}

/**
 * Splits a catalogue text with `<b>…</b>` and `<code>…</code>` into parts, so a sentence with a
 * bold word stays one text for translators. Only these two tags are understood; anything else is
 * plain text, and nothing is ever parsed as HTML (a translation file can't inject markup).
 */
export function richParts(text: string): RichPart[] {
  const parts: RichPart[] = [];
  const re = /<(b|code)>(.*?)<\/\1>/gs;
  let at = 0;
  for (const m of text.matchAll(re)) {
    if (m.index > at) parts.push({ text: text.slice(at, m.index), tag: null });
    parts.push({ text: m[2]!, tag: m[1] as 'b' | 'code' });
    at = m.index + m[0].length;
  }
  if (at < text.length) parts.push({ text: text.slice(at), tag: null });
  return parts;
}
