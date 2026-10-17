/**
 * The shell's folder layering (docs/development.md, "In the shell"). Breaking a rule here means
 * changing that document first, as a deliberate decision.
 *
 *   src/lib/i18n/       texts and translations      may import: nothing (the bottom layer)
 *   src/lib/api/        talks to the core           may import: i18n, nothing from the UI
 *   src/lib/state/      app state (Svelte runes)    may import: api, i18n
 *   src/lib/desktop/    window manager logic        may import: i18n (and no Svelte)
 *   src/lib/viewers/    canvas drawing maths        may import: i18n (and no Svelte)
 *   src/components/, src/windows/   Svelte UI       may import: state, desktop, viewers, i18n,
 *                                                   never api
 *
 *   src/lib/circuit/    Circuits' logic             may import: nothing (pure TypeScript and avr8js)
 *
 * `src/test/` holds shared test fakes and is only imported by *.test.ts files.
 *
 *   ../sdk/js/shell/src/protocol.ts   the app protocol (docs/apps.md §9), shared with the
 *                                     `@faber-os/shell` SDK: types only, imported only by the app
 *                                     window's host in src/lib/state, and importing nothing.
 */
/** @type {import('dependency-cruiser').IConfiguration} */
module.exports = {
  forbidden: [
    {
      name: 'i18n-imports-nothing',
      comment:
        'src/lib/i18n is the bottom layer: every other folder may use t(), so it may use none of them.',
      severity: 'error',
      from: { path: '^src/lib/i18n/', pathNot: '\.test\.ts$' },
      to: { path: '^src/', pathNot: '^src/lib/i18n/' },
    },
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
      comment:
        'src/lib/desktop is plain TypeScript: no other shell folder (except i18n for its texts) and no Svelte.',
      severity: 'error',
      from: { path: '^src/lib/desktop/', pathNot: '\.test\.ts$' },
      to: { pathNot: '^src/lib/(desktop|i18n)/' },
    },
    {
      name: 'viewers-import-nothing',
      comment:
        'src/lib/viewers are pure functions: no other shell folder (except i18n for its texts) and no Svelte.',
      severity: 'error',
      from: { path: '^src/lib/viewers/', pathNot: '\.test\.ts$' },
      to: { pathNot: '^src/lib/(viewers|i18n)/' },
    },
    {
      name: 'circuit-imports-nothing',
      comment:
        'src/lib/circuit is the logic of Circuits (docs/circuits.md): no other shell folder, no Svelte.',
      severity: 'error',
      from: { path: '^src/lib/circuit/' },
      to: { path: '^src/', pathNot: '^src/lib/circuit/' },
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
      name: 'sdk-only-through-the-app-host',
      comment:
        'The SDK is the app side. The shell shares only its protocol types, through the app window host (src/lib/state/appHost.ts).',
      severity: 'error',
      from: { pathNot: '^(src/lib/state/appHost(\\.test)?\\.ts|\\.\\./sdk/)' },
      to: { path: '^\\.\\./sdk/' },
    },
    {
      name: 'sdk-host-uses-only-the-protocol',
      comment: 'The host may use the protocol, never the SDK itself (that runs in the app).',
      severity: 'error',
      from: { path: '^src/' },
      to: { path: '^\\.\\./sdk/', pathNot: '^\\.\\./sdk/js/shell/src/protocol\\.ts$' },
    },
    {
      name: 'sdk-protocol-imports-nothing',
      comment: 'The protocol is plain types, shared by both sides: it may import nothing.',
      severity: 'error',
      from: { path: '^\\.\\./sdk/js/shell/src/protocol\\.ts$' },
      to: {},
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
