/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import pkg from './package.json' with { type: 'json' };

// The core listens on 127.0.0.1:7878 (docs/api.md). In development Vite forwards /api to it,
// including the WebSocket, so the shell can always use relative URLs.
const CORE = 'http://127.0.0.1:7878';

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  define: {
    SHELL_VERSION: JSON.stringify(pkg.version),
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
    include: ['src/**/*.test.ts'],
    setupFiles: ['src/test/setup.ts'],
    restoreMocks: true,
  },
});
