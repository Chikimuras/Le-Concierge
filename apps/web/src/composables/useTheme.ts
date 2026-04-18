import { onMounted, onUnmounted, ref, watch } from 'vue'

/**
 * Theme management.
 *
 * Three modes:
 * - `system`: follow `prefers-color-scheme` and react to OS changes live.
 * - `light` / `dark`: explicit override, persisted in localStorage.
 *
 * The initial class is applied inline in `index.html` to avoid a flash of
 * the wrong theme before Vue mounts. This composable only handles runtime
 * changes after mount.
 */

const STORAGE_KEY = 'le-concierge.theme'
export type ThemeMode = 'system' | 'light' | 'dark'

function isThemeMode(value: unknown): value is ThemeMode {
  return value === 'system' || value === 'light' || value === 'dark'
}

function readStoredMode(): ThemeMode {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (isThemeMode(raw)) return raw
  } catch {
    // localStorage unavailable (SSR, privacy modes) — fall through.
  }
  return 'system'
}

function resolveEffective(mode: ThemeMode): 'light' | 'dark' {
  if (mode === 'system') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  }
  return mode
}

function applyEffective(effective: 'light' | 'dark'): void {
  document.documentElement.classList.toggle('dark', effective === 'dark')
}

export function useTheme(): {
  mode: ReturnType<typeof ref<ThemeMode>>
  effective: ReturnType<typeof ref<'light' | 'dark'>>
  setMode: (next: ThemeMode) => void
} {
  const mode = ref<ThemeMode>(readStoredMode())
  const effective = ref<'light' | 'dark'>(resolveEffective(mode.value))

  const mql = window.matchMedia('(prefers-color-scheme: dark)')
  const onSystemChange = (): void => {
    if (mode.value === 'system') {
      effective.value = resolveEffective('system')
      applyEffective(effective.value)
    }
  }

  watch(
    mode,
    (next) => {
      try {
        if (next === 'system') localStorage.removeItem(STORAGE_KEY)
        else localStorage.setItem(STORAGE_KEY, next)
      } catch {
        /* ignore */
      }
      effective.value = resolveEffective(next)
      applyEffective(effective.value)
    },
    { immediate: false },
  )

  onMounted(() => {
    mql.addEventListener('change', onSystemChange)
    // Ensure class is in sync in case index.html inline script and stored
    // mode disagree (e.g. user cleared storage mid-session).
    applyEffective(effective.value)
  })

  onUnmounted(() => {
    mql.removeEventListener('change', onSystemChange)
  })

  const setMode = (next: ThemeMode): void => {
    mode.value = next
  }

  return { mode, effective, setMode }
}
