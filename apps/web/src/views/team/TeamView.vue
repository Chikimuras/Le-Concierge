<script setup lang="ts">
  import { toTypedSchema } from '@vee-validate/zod'
  import { useForm } from 'vee-validate'
  import { computed, ref } from 'vue'
  import { useI18n } from 'vue-i18n'
  import { useRoute } from 'vue-router'

  import Button from '@/components/ui/Button.vue'
  import Card from '@/components/ui/Card.vue'
  import FormField from '@/components/ui/FormField.vue'
  import Input from '@/components/ui/Input.vue'
  import { useCancelInvite, useCreateInvite, useInvites } from '@/composables/useInvites'
  import { readProblemDetails } from '@/lib/api-client'
  import { createInviteSchema } from '@/lib/api-contracts'
  import { useActiveOrgStore } from '@/stores/activeOrg'

  const { t, locale } = useI18n()
  const route = useRoute()
  const activeOrg = useActiveOrgStore()

  const slug = computed(() => (typeof route.params.slug === 'string' ? route.params.slug : ''))

  const invites = useInvites(slug)
  const createMutation = useCreateInvite(slug)
  const cancelMutation = useCancelInvite(slug)

  const serverMessage = ref<string | null>(null)
  const successMessage = ref<string | null>(null)

  const schema = toTypedSchema(
    createInviteSchema({
      emailRequired: t('auth.validation.email_required'),
      emailInvalid: t('team.errors.invalid_email'),
      roleRequired: t('team.errors.invalid_role'),
    }),
  )
  const { defineField, errors, handleSubmit, isSubmitting, resetForm } = useForm({
    validationSchema: schema,
    initialValues: { email: '', role: 'manager' as const },
  })
  const [email, emailAttrs] = defineField('email')
  const [role, roleAttrs] = defineField('role')

  const pendingInvites = computed(() => invites.data.value?.invites ?? [])

  const onSubmit = handleSubmit(async (values) => {
    serverMessage.value = null
    successMessage.value = null
    try {
      await createMutation.mutateAsync({ email: values.email, role: values.role })
      successMessage.value = t('team.form.sent')
      resetForm({ values: { email: '', role: 'manager' } })
    } catch (err) {
      const problem = await readProblemDetails(err)
      serverMessage.value =
        problem?.kind === 'conflict' ? t('team.errors.conflict') : t('team.errors.generic')
    }
  })

  async function onCancel(id: string) {
    serverMessage.value = null
    try {
      await cancelMutation.mutateAsync(id)
    } catch {
      serverMessage.value = t('team.errors.generic')
    }
  }

  const dateFormatter = computed(
    () =>
      new Intl.DateTimeFormat(locale.value, {
        dateStyle: 'medium',
        timeStyle: 'short',
      }),
  )
  function formatDate(iso: string): string {
    return dateFormatter.value.format(new Date(iso))
  }
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-3xl flex-col gap-8 px-6 py-12">
    <header class="space-y-1">
      <p class="text-sm uppercase tracking-wider text-muted-foreground">{{ t('app.name') }}</p>
      <h1 class="text-3xl font-semibold">{{ t('team.title') }}</h1>
      <p class="text-sm text-muted-foreground">{{ t('team.subtitle') }}</p>
    </header>

    <Card v-if="activeOrg.canManage" class="p-6">
      <h2 class="mb-3 text-lg font-semibold">{{ t('team.section_create') }}</h2>
      <form novalidate class="grid gap-4" @submit.prevent="onSubmit">
        <FormField :label="t('team.form.email')" :error-message="errors.email" required>
          <template #default="{ id, describedby, invalid }">
            <Input
              :id="id"
              v-model="email"
              v-bind="emailAttrs"
              type="email"
              autocomplete="email"
              required
              :aria-describedby="describedby"
              :invalid="invalid"
            />
          </template>
        </FormField>

        <FormField :label="t('team.form.role')" :error-message="errors.role" required>
          <template #default="{ id, describedby, invalid }">
            <select
              :id="id"
              v-model="role"
              v-bind="roleAttrs"
              class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
              :aria-describedby="describedby"
              :aria-invalid="invalid || undefined"
            >
              <option value="manager">{{ t('auth.roles.manager') }}</option>
              <option value="cleaner">{{ t('auth.roles.cleaner') }}</option>
              <option value="guest">{{ t('auth.roles.guest') }}</option>
            </select>
          </template>
        </FormField>

        <p v-if="serverMessage" role="alert" class="text-sm text-destructive">
          {{ serverMessage }}
        </p>
        <p v-else-if="successMessage" role="status" class="text-sm text-emerald-600">
          {{ successMessage }}
        </p>

        <Button type="submit" :loading="isSubmitting" class="w-full sm:w-auto">
          {{ t('team.form.submit') }}
        </Button>
      </form>
    </Card>

    <section>
      <h2 class="mb-3 text-lg font-semibold">{{ t('team.section_pending') }}</h2>

      <p v-if="invites.isPending.value" class="text-sm text-muted-foreground">
        {{ t('properties.list.loading') }}
      </p>
      <p v-else-if="invites.isError.value" class="text-sm text-destructive" role="alert">
        {{ t('team.errors.generic') }}
      </p>
      <Card v-else-if="pendingInvites.length === 0" class="p-8 text-center">
        <p class="text-sm text-muted-foreground">{{ t('team.empty') }}</p>
      </Card>
      <Card v-else class="overflow-hidden p-0">
        <table class="w-full text-sm">
          <thead class="bg-muted text-left">
            <tr>
              <th class="px-4 py-3 font-medium">{{ t('team.table.email') }}</th>
              <th class="px-4 py-3 font-medium">{{ t('team.table.role') }}</th>
              <th class="px-4 py-3 font-medium">{{ t('team.table.sent') }}</th>
              <th class="px-4 py-3 font-medium">{{ t('team.table.expires') }}</th>
              <th class="px-4 py-3 text-right font-medium">&nbsp;</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="i in pendingInvites" :key="i.id" class="border-t border-border">
              <td class="px-4 py-3 font-medium">{{ i.email }}</td>
              <td class="px-4 py-3 text-muted-foreground">{{ t(`auth.roles.${i.role}`) }}</td>
              <td class="px-4 py-3 text-muted-foreground">{{ formatDate(i.created_at) }}</td>
              <td class="px-4 py-3 text-muted-foreground">{{ formatDate(i.expires_at) }}</td>
              <td class="px-4 py-3 text-right">
                <Button
                  v-if="activeOrg.canManage"
                  variant="ghost"
                  size="sm"
                  :loading="cancelMutation.isPending.value"
                  @click="onCancel(i.id)"
                >
                  {{ t('team.table.cancel') }}
                </Button>
              </td>
            </tr>
          </tbody>
        </table>
      </Card>
    </section>
  </main>
</template>
