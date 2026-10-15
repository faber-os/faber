/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import pkg from './package.json' with { type: 'json' };
import { fileURLToPath } from 'node:url';
import { apiFingerprint } from './apiFingerprint.ts';

// The core listens on 127.0.0.1:7878 (docs/api.md). In development Vite forwards /api to it,
// including the WebSocket, so the shell can always use relative URLs.
const CORE = 'http://127.0.0.1:7878';

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  define: {
    SHELL_VERSION: JSON.stringify(pkg.version),
    // Compared with the core's `api` (GET /system); see core/build.rs.
    FABER_API: JSON.stringify(
      apiFingerprint(fileURLToPath(new URL('./src/lib/api/generated', import.meta.url))),
    ),
  },
  server: {
    proxy: {
      '/api': { target: CORE, ws: true, changeOrigin: false },
    },
  },
  preview: {
    proxy: {
      '/api': { target: CORE, ws: true, changeOrigin: false },
    },
  },
  build: {
    target: 'es2022',
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts', 'apiFingerprint.test.ts'],
    setupFiles: ['src/test/setup.ts'],
    restoreMocks: true,
  },
});
