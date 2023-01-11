/// <reference types="vitest" />

import { resolve } from 'path';
import { defineConfig } from 'vite';
import dts from 'vite-plugin-dts';

export default defineConfig({
  plugins: [dts()],
  resolve: {
    conditions: ['source'],
  },
  test: {
    minThreads: 1,
    maxThreads: 8,
    hookTimeout: 1000000,
    testTimeout: 1000000,
  },
  build: {
    lib: {
      entry: resolve(__dirname, 'src/index.ts'),
      fileName: 'index',
      formats: ['es', 'cjs'],
    },
  },
});
