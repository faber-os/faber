export type LineEnding = 'none' | 'lf' | 'cr' | 'crlf';

const ENDINGS: Record<LineEnding, string> = { none: '', lf: '\n', cr: '\r', crlf: '\r\n' };

export function encodeLine(text: string, ending: LineEnding): Uint8Array {
  return new TextEncoder().encode(text + ENDINGS[ending]);
}

export function bytesToText(bytes: Uint8Array): string {
  return new TextDecoder('utf-8', { fatal: false }).decode(bytes).replace(/\r\n?/g, '\n');
}

/** Classic hex dump lines: `offset  hex bytes  ascii`. */
export function hexLines(bytes: Uint8Array, perLine = 16, startOffset = 0): string[] {
  const lines: string[] = [];
  for (let i = 0; i < bytes.length; i += perLine) {
    const row = bytes.subarray(i, i + perLine);
    const hex = Array.from(row, (b) => b.toString(16).padStart(2, '0'))
      .join(' ')
      .padEnd(perLine * 3 - 1, ' ');
    const ascii = Array.from(row, (b) =>
      b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : '.',
    ).join('');
    lines.push(`${(startOffset + i).toString(16).padStart(8, '0')}  ${hex}  ${ascii}`);
  }
  return lines;
}

/** The newest `limit` bytes received, so a chatty port can't grow the page without bound. */
export class ByteLog {
  private buf: Uint8Array;
  private length = 0;
  /** Bytes received since the last clear, including the ones already dropped. */
  total = 0;

  constructor(readonly limit: number) {
    this.buf = new Uint8Array(limit);
  }

  /** Offset (since the last clear) of the first byte still kept. */
  get firstOffset(): number {
    return this.total - this.length;
  }

  append(chunk: Uint8Array): void {
    this.total += chunk.length;
    if (chunk.length >= this.limit) {
      this.buf.set(chunk.subarray(chunk.length - this.limit));
      this.length = this.limit;
      return;
    }
    const overflow = this.length + chunk.length - this.limit;
    if (overflow > 0) {
      this.buf.copyWithin(0, overflow, this.length);
      this.length -= overflow;
    }
    this.buf.set(chunk, this.length);
    this.length += chunk.length;
  }

  bytes(): Uint8Array {
    return this.buf.slice(0, this.length);
  }

  clear(): void {
    this.length = 0;
    this.total = 0;
  }
}
