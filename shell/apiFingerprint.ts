// The API fingerprint: a hash of the generated API types. Must match core/build/api_fingerprint.rs
// exactly, so the shell can tell when the core it talks to was built from a different API.
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const OFFSET = 0xcbf29ce484222325n;
const PRIME = 0x100000001b3n;
const MASK = (1n << 64n) - 1n;

/** FNV-1a 64 over every `.ts` file, sorted by name: name, 0, contents without `\r`, 0. */
export function apiFingerprint(dir: string): string {
  const names = readdirSync(dir)
    .filter((n) => n.endsWith('.ts'))
    .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
  let h = OFFSET;
  const feed = (bytes: Iterable<number>) => {
    for (const b of bytes) h = ((h ^ BigInt(b)) * PRIME) & MASK;
  };
  for (const name of names) {
    feed(Buffer.from(name));
    feed([0]);
    feed(readFileSync(join(dir, name)).filter((b) => b !== 13));
    feed([0]);
  }
  return h.toString(16).padStart(16, '0');
}
