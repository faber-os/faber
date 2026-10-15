import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { builtModule } from './transpile.ts';

/** `npm run build`: dist/faber-shell.js (the module apps include) and its type declarations. */
const path = (rel: string) => fileURLToPath(new URL(rel, import.meta.url));
mkdirSync(path('../dist/'), { recursive: true });
writeFileSync(path('../dist/faber-shell.js'), builtModule());

const program = ts.createProgram([path('../src/faber-shell.ts')], {
  target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.Bundler,
  strict: true,
  declaration: true,
  emitDeclarationOnly: true,
  outDir: path('../dist/'),
  rootDir: path('../src/'),
  skipLibCheck: true,
  lib: ['lib.es2022.d.ts', 'lib.dom.d.ts'],
});
const result = program.emit();
const problems = ts.getPreEmitDiagnostics(program).concat(result.diagnostics);
if (problems.length > 0) {
  console.error(ts.formatDiagnostics(problems, ts.createCompilerHost({})));
  process.exit(1);
}
console.log('Built dist/faber-shell.js and its type declarations.');
