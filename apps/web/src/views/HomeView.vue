<script setup lang="ts">
import { CheckCircle2, Loader2, Moon, Sun, SunMoon, XCircle } from 'lucide-vue-next'
import { computed, type Component } from 'vue'
import { useI18n } from 'vue-i18n'

import { useHealth } from '@/composables/useHealth'
import { useTheme, type ThemeMode } from '@/composables/useTheme'
import { cn } from '@/lib/utils'

const { t } = useI18n()
const { mode, setMode } = useTheme()

// Destructure refs so the template can consume them as top-level values
// (Vue auto-unwraps refs bound directly to the setup scope). Keeping the
// whole `UseQueryReturnType` and accessing `.value` from the template is
// brittle across minor versions of @tanstack/vue-query.
const { isPending, isError, data } = useHealth()

const statusLabel = computed(() => {
  if (isPending.value) return t('pages.home.api_status_loading')
  if (isError.value) return t('pages.home.api_status_error')
  return t('pages.home.api_status_ok')
})

const statusTone = computed(() => {
  if (isPending.value) return 'text-muted-foreground'
  if (isError.value) return 'text-destructive'
  return 'text-emerald-600 dark:text-emerald-400'
})

interface ThemeOption {
  value: ThemeMode
  icon: Component
  key: string
}

const themeOptions: readonly ThemeOption[] = [
  { value: 'system', icon: SunMoon, key: 'theme.system' },
  { value: 'light', icon: Sun, key: 'theme.light' },
  { value: 'dark', icon: Moon, key: 'theme.dark' },
]
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-2xl flex-col items-start justify-center gap-8 px-6 py-12">
    <header class="space-y-2">
      <p class="text-sm uppercase tracking-wider text-muted-foreground">{{ t('app.name') }}</p>
      <h1 class="text-3xl font-semibold">{{ t('app.tagline') }}</h1>
    </header>

    <section
      class="w-full rounded-lg border border-border bg-card p-6 text-card-foreground shadow-sm"
      data-testid="api-status"
    >
      <h2 class="text-sm font-medium text-muted-foreground">
        {{ t('pages.home.api_status_heading') }}
      </h2>

      <div class="mt-3 flex items-center gap-3">
        <Loader2 v-if="isPending" class="size-5 animate-spin text-muted-foreground" aria-hidden="true" />
        <XCircle v-else-if="isError" class="size-5 text-destructive" aria-hidden="true" />
        <CheckCircle2 v-else class="size-5 text-emerald-600 dark:text-emerald-400" aria-hidden="true" />
        <span :class="cn('text-lg font-medium', statusTone)">{{ statusLabel }}</span>
      </div>

      <dl v-if="data" class="mt-4 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
        <dt class="text-muted-foreground">{{ t('pages.home.api_service') }}</dt>
        <dd class="font-mono">{{ data.service }}</dd>
        <dt class="text-muted-foreground">{{ t('pages.home.api_version') }}</dt>
        <dd class="font-mono">{{ data.version }}</dd>
      </dl>
    </section>

    <section class="w-full" aria-label="Theme">
      <div
        role="group"
        :aria-label="t('theme.toggle_aria')"
        class="inline-flex items-center gap-1 rounded-md border border-border bg-card p-1"
      >
        <button
          v-for="option in themeOptions"
          :key="option.value"
          type="button"
          :aria-pressed="mode === option.value"
          :class="
            cn(
              'inline-flex items-center gap-1.5 rounded-sm px-2 py-1 text-xs transition-colors',
              mode === option.value
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground',
            )
          "
          @click="setMode(option.value)"
        >
          <component :is="option.icon" class="size-3.5" aria-hidden="true" />
          <span>{{ t(option.key) }}</span>
        </button>
      </div>
    </section>
  </main>
</template>
