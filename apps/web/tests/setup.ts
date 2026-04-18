// Vitest setup: polyfill browser APIs that jsdom does not implement natively.
import { vi } from 'vitest'

// `useTheme` reads `window.matchMedia('(prefers-color-scheme: dark)')`; jsdom
// omits it by default.
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
})
