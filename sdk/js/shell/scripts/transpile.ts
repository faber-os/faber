import { readFileSync } from 'node:fs';
import ts from 'typescript';

/**
 * The SDK as the one ES module apps include: `src/faber-shell.ts` with its types taken out. The
 * build writes it to `dist/`, and a test checks that the example apps carry exactly this.
 */
export function builtModule(): string {
  const url = new URL('../src/faber-shell.ts', import.meta.url);
  const source = readFileSync(url, 'utf8');
  const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')) as {
    version: string;
  };
  const out = ts.transpileModule(source, {
    fileName: 'faber-shell.ts',
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
      verbatimModuleSyntax: true,
      removeComments: false,
      newLine: ts.NewLineKind.LineFeed,
    },
  });
  const banner =
    `// @faber-os/shell ${pkg.version}: the SDK for a Faber app's UI (docs/apps.md).\n` +
    '// Built from sdk/js/shell/src/faber-shell.ts by `npm run build`; edit that file, not this one.\n';
  return banner + out.outputText;
}
