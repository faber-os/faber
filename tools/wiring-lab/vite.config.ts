/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// The wiring lab runs on its own (npm run dev → http://localhost:5180); it moves into the shell
// once it is right (docs/wiring-diagram.md).
export default defineConfig({
  plugins: [svelte()],
  server: { host: '127.0.0.1' },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
});
