<script setup lang="ts">
  import { LogOut, Settings as SettingsIcon, ShieldCheck } from 'lucide-vue-next'
  import { useI18n } from 'vue-i18n'
  import { RouterLink } from 'vue-router'

  import Button from '@/components/ui/Button.vue'
  import Card from '@/components/ui/Card.vue'
  import { useLogout } from '@/composables/useAuth'
  import { useSessionStore } from '@/stores/session'

  const { t } = useI18n()
  const session = useSessionStore()
  const logout = useLogout()
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-3xl flex-col gap-8 px-6 py-12">
    <header class="flex items-center justify-between gap-2">
      <div>
        <p class="text-sm uppercase tracking-wider text-muted-foreground">
          {{ t('app.name') }}
        </p>
        <h1 class="text-3xl font-semibold">{{ t('pages.dashboard.title') }}</h1>
      </div>
      <div class="flex items-center gap-2">
        <RouterLink v-slot="{ navigate }" :to="{ name: 'settings' }" custom>
          <Button variant="ghost" @click="navigate">
            <SettingsIcon class="size-4" aria-hidden="true" />
            {{ t('pages.settings.title') }}
          </Button>
        </RouterLink>
        <Button variant="ghost" :loading="logout.isPending.value" @click="logout.mutate()">
          <LogOut class="size-4" aria-hidden="true" />
          {{ t('auth.logout.submit') }}
        </Button>
      </div>
    </header>

    <Card class="p-6">
      <h2 class="text-sm font-medium text-muted-foreground">
        {{ t('pages.dashboard.organizations') }}
      </h2>

      <ul v-if="session.memberships.length > 0" class="mt-3 divide-y divide-border">
        <li
          v-for="m in session.memberships"
          :key="m.org_id"
          class="flex items-center justify-between py-3 text-sm"
        >
          <div>
            <p class="font-medium">{{ m.org_name }}</p>
            <p class="font-mono text-xs text-muted-foreground">{{ m.org_slug }}</p>
          </div>
          <span class="rounded-sm bg-secondary px-2 py-0.5 text-xs font-medium uppercase">
            {{ t(`auth.roles.${m.role}`) }}
          </span>
        </li>
      </ul>
      <p v-else class="mt-3 text-sm text-muted-foreground">
        {{ t('pages.dashboard.no_orgs') }}
      </p>
    </Card>

    <Card v-if="session.isPlatformAdmin" class="p-6">
      <h2 class="flex items-center gap-2 text-sm font-medium text-muted-foreground">
        <ShieldCheck class="size-4" aria-hidden="true" />
        {{ t('pages.dashboard.platform_admin') }}
      </h2>
      <p class="mt-2 text-sm">{{ t('pages.dashboard.platform_admin_body') }}</p>
    </Card>
  </main>
</template>
