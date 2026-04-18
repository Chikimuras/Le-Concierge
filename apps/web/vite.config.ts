import { fileURLToPath, URL } from 'node:url'

import vue from '@vitejs/plugin-vue'
// `vitest/config` re-exports Vite's defineConfig plus types for the `test`
// block, so a single file configures both the dev server and the test runner.
import { defineConfig } from 'vitest/config'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    // `/api/*` → the Axum API on :3000 (stripping the `/api` prefix). This
    // keeps CORS off the dev hot-path entirely. In prod the reverse proxy
    // (Caddy/Traefik) plays the same role.
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3000',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
  preview: {
    port: 5173,
    strictPort: true,
  },
  test: {
    environment: 'jsdom',
    globals: false,
    include: ['tests/**/*.{spec,test}.ts'],
    setupFiles: ['tests/setup.ts'],
    coverage: {
      reporter: ['text', 'html'],
      include: ['src/**/*.{ts,vue}'],
    },
  },
})
