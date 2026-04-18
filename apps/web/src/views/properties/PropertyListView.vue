<script setup lang="ts">
  import { Plus } from 'lucide-vue-next'
  import { computed } from 'vue'
  import { useI18n } from 'vue-i18n'
  import { RouterLink, useRoute } from 'vue-router'

  import Button from '@/components/ui/Button.vue'
  import Card from '@/components/ui/Card.vue'
  import { useProperties } from '@/composables/useProperties'
  import { useActiveOrgStore } from '@/stores/activeOrg'

  const { t } = useI18n()
  const route = useRoute()
  const activeOrg = useActiveOrgStore()
  // `route.params.slug` is `string | string[]` in Vue Router; narrow it.
  const slug = computed(() => (typeof route.params.slug === 'string' ? route.params.slug : ''))

  const query = useProperties(slug)
  const properties = computed(() => query.data.value?.properties ?? [])
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-5xl flex-col gap-6 px-6 py-12">
    <header class="flex flex-wrap items-center justify-between gap-3">
      <div>
        <p class="text-sm uppercase tracking-wider text-muted-foreground">{{ t('app.name') }}</p>
        <h1 class="text-3xl font-semibold">{{ t('properties.list.title') }}</h1>
        <p class="mt-1 text-sm text-muted-foreground">{{ t('properties.list.subtitle') }}</p>
      </div>
      <RouterLink
        v-if="activeOrg.canManage"
        v-slot="{ navigate }"
        :to="{ name: 'property-new', params: { slug } }"
        custom
      >
        <Button @click="navigate">
          <Plus class="size-4" aria-hidden="true" />
          {{ t('properties.list.create') }}
        </Button>
      </RouterLink>
    </header>

    <p v-if="query.isPending.value" class="text-sm text-muted-foreground">
      {{ t('properties.list.loading') }}
    </p>
    <p v-else-if="query.isError.value" class="text-sm text-destructive" role="alert">
      {{ t('properties.list.error') }}
    </p>

    <Card v-else-if="properties.length === 0" class="p-8 text-center">
      <p class="text-sm text-muted-foreground">{{ t('properties.list.empty') }}</p>
    </Card>

    <Card v-else class="overflow-hidden p-0">
      <table class="w-full text-sm">
        <thead class="bg-muted text-left">
          <tr>
            <th class="px-4 py-3 font-medium">{{ t('properties.list.column_name') }}</th>
            <th class="px-4 py-3 font-medium">{{ t('properties.list.column_slug') }}</th>
            <th class="px-4 py-3 font-medium">{{ t('properties.list.column_city') }}</th>
            <th class="px-4 py-3 text-right font-medium">
              {{ t('properties.list.column_bedrooms') }}
            </th>
            <th class="px-4 py-3 text-right font-medium">
              {{ t('properties.list.column_guests') }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="p in properties"
            :key="p.id"
            class="cursor-pointer border-t border-border transition-colors hover:bg-accent/40"
          >
            <td class="px-4 py-3">
              <RouterLink
                :to="{ name: 'property-detail', params: { slug, id: p.id } }"
                class="font-medium text-foreground hover:underline"
              >
                {{ p.name }}
              </RouterLink>
            </td>
            <td class="px-4 py-3 font-mono text-xs text-muted-foreground">{{ p.slug }}</td>
            <td class="px-4 py-3 text-muted-foreground">{{ p.city ?? '—' }}</td>
            <td class="px-4 py-3 text-right tabular-nums">{{ p.bedrooms ?? '—' }}</td>
            <td class="px-4 py-3 text-right tabular-nums">{{ p.max_guests ?? '—' }}</td>
          </tr>
        </tbody>
      </table>
    </Card>
  </main>
</template>
