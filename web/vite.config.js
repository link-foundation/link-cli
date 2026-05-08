import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';

export default defineConfig({
  root: new URL('.', import.meta.url).pathname,
  base: process.env.DEPLOY_TARGET === 'github-pages' ? '/link-cli/' : '/',
  plugins: [wasm(), react()],
  build: {
    outDir: '../dist',
    emptyOutDir: true,
  },
  server: {
    port: 5173,
    strictPort: false,
  },
});
