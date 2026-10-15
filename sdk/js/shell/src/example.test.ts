import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { builtModule } from '../scripts/transpile.ts';

/**
 * The example apps carry the built SDK in their `ui/` (an app's UI can only load its own files).
 * When the SDK changes, this rewrites their copy and fails once, so the change gets committed:
 * like the generated TypeScript types, commit what the tests write.
 */
const COPIES = ['../../../../examples/apps/wall-follower/ui/faber-shell.js'];

describe('the example apps', () => {
  for (const rel of COPIES) {
    it(`carry exactly the built SDK: ${rel.replace(/^(\.\.\/)+/, '')}`, () => {
      const path = fileURLToPath(new URL(rel, import.meta.url));
      const fresh = builtModule();
      let committed: string | null = null;
      try {
        committed = readFileSync(path, 'utf8');
      } catch {
        committed = null;
      }
      if (committed !== fresh) {
        writeFileSync(path, fresh);
        expect.fail(`${path} was out of date and has been rewritten; commit it`);
      }
    });
  }
});
