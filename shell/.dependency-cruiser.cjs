/**
 * The shell's folder layering (docs/development.md, "In the shell"). Breaking a rule here means
 * changing that document first, as a deliberate decision.
 *
 *   src/lib/api/        talks to the core           may import: nothing from the UI
 *   src/lib/state/      app state (Svelte runes)    may import: api
 *   src/lib/desktop/    window manager logic        may import: nothing (and no Svelte)
 *   src/lib/viewers/    canvas drawing maths        may import: nothing (and no Svelte)
 *   src/components/, src/windows/   Svelte UI       may import: state, desktop, viewers, never api
 *
 * `src/test/` holds shared test fakes and is only imported by *.test.ts files.
 */
/** @type {import('dependency-cruiser').IConfiguration} */
module.exports = {
  forbidden: [
    {
      name: 'api-imports-no-ui',
      comment:
        'src/lib/api is the bottom layer: it must not know about state, desktop, viewers or UI.',
      severity: 'error',
      from: { path: '^src/lib/api/' },
      to: { path: '^src/(lib/(state|desktop|viewers)|components|windows)/' },
    },
    {
      name: 'state-imports-only-api',
      comment: 'src/lib/state may only use src/lib/api (and itself).',
      severity: 'error',
      from: { path: '^src/lib/state/' },
      to: { path: '^src/(lib/(desktop|viewers)|components|windows)/' },
    },
    {
      name: 'desktop-imports-nothing',
      comment: 'src/lib/desktop is plain TypeScript: no other shell folder and no Svelte.',
      severity: 'error',
      from: { path: '^src/lib/desktop/', pathNot: '\.test\.ts$' },
      to: { pathNot: '^src/lib/desktop/' },
    },
    {
      name: 'viewers-import-nothing',
      comment: 'src/lib/viewers are pure functions: no other shell folder and no Svelte.',
      severity: 'error',
      from: { path: '^src/lib/viewers/', pathNot: '\.test\.ts$' },
      to: { pathNot: '^src/lib/viewers/' },
    },
    {
      name: 'ui-never-imports-api',
      comment: 'Svelte UI goes through src/lib/state; it never talks to the core directly.',
      severity: 'error',
      from: { path: '^src/((components|windows)/|App\\.svelte$|main\\.ts$)' },
      to: { path: '^src/lib/api/' },
    },
    {
      name: 'test-helpers-only-in-tests',
      comment: 'src/test holds fakes for tests; production code must not depend on them.',
      severity: 'error',
      from: { path: '^src/', pathNot: '(\.test\.ts$|^src/test/)' },
      to: { path: '^src/test/' },
    },
    {
      name: 'no-circular',
      severity: 'error',
      from: {},
      to: { circular: true },
    },
    {
      name: 'not-to-unresolvable',
      severity: 'error',
      from: {},
      to: { couldNotResolve: true },
    },
  ],
  options: {
    doNotFollow: { path: 'node_modules' },
    tsPreCompilationDeps: true,
    tsConfig: { fileName: 'tsconfig.json' },
    enhancedResolveOptions: {
      exportsFields: ['exports'],
      conditionNames: ['import', 'svelte', 'browser', 'default', 'types'],
      extensions: ['.ts', '.js', '.svelte', '.svelte.ts'],
      mainFields: ['svelte', 'module', 'main', 'types'],
    },
    reporterOptions: { text: { highlightFocused: true } },
  },
};
