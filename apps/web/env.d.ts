/// <reference types="vite/client" />

// Shim for `.vue` single-file components. Needed so TypeScript (and ESLint
// in type-checked mode) don't see SFC imports as `any`. `vue-tsc` resolves
// the real shape at build time; this declaration just keeps the plain `tsc`
// and ESLint passes happy.
declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
  export default component
}

interface ImportMetaEnv {
  /**
   * Absolute base URL for the HTTP API. Empty/undefined in dev means the
   * client must use the relative `/api` path which Vite proxies on :5173.
   */
  readonly VITE_API_BASE_URL?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
