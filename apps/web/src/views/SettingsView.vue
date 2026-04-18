<script setup lang="ts">
  import { toTypedSchema } from '@vee-validate/zod'
  import { useForm } from 'vee-validate'
  import { ref } from 'vue'
  import { useI18n } from 'vue-i18n'
  import { RouterLink } from 'vue-router'

  import Button from '@/components/ui/Button.vue'
  import Card from '@/components/ui/Card.vue'
  import FormField from '@/components/ui/FormField.vue'
  import Input from '@/components/ui/Input.vue'
  import { useTotpDisable } from '@/composables/useAuth'
  import { readProblemDetails } from '@/lib/api-client'
  import { disableRequestSchema } from '@/lib/api-contracts'
  import { useSessionStore } from '@/stores/session'

  const { t } = useI18n()
  const session = useSessionStore()
  const disable = useTotpDisable()

  const showDisableForm = ref(false)
  const serverMessage = ref<string | null>(null)

  const schema = toTypedSchema(
    disableRequestSchema({
      passwordRequired: t('auth.validation.password_required'),
      codeInvalid: t('auth.totp.errors.invalid_code'),
    }),
  )
  const { defineField, errors, handleSubmit, isSubmitting } = useForm({
    validationSchema: schema,
  })
  const [password, passwordAttrs] = defineField('password')
  const [code, codeAttrs] = defineField('code')

  const onSubmit = handleSubmit(async (values) => {
    serverMessage.value = null
    try {
      await disable.mutateAsync({ password: values.password, code: values.code })
    } catch (err) {
      const problem = await readProblemDetails(err)
      serverMessage.value =
        problem?.kind === 'rate_limited'
          ? t('auth.totp.errors.rate_limited')
          : t('auth.totp.errors.invalid_code')
    }
  })
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-3xl flex-col gap-8 px-6 py-12">
    <header class="flex items-center justify-between">
      <div>
        <p class="text-sm uppercase tracking-wider text-muted-foreground">{{ t('app.name') }}</p>
        <h1 class="text-3xl font-semibold">{{ t('pages.settings.title') }}</h1>
      </div>
      <RouterLink v-slot="{ navigate }" :to="{ name: 'dashboard' }" custom>
        <Button variant="ghost" @click="navigate">{{ t('pages.home.go_to_dashboard') }}</Button>
      </RouterLink>
    </header>

    <Card class="p-6">
      <header class="mb-4">
        <h2 class="text-lg font-semibold">{{ t('auth.totp.disable.title') }}</h2>
        <p v-if="session.mfaEnrolled" class="mt-1 text-sm text-muted-foreground">
          {{ t('auth.totp.disable.warning') }}
        </p>
        <p v-else class="mt-1 text-sm text-muted-foreground">
          {{ t('auth.totp.setup.subtitle') }}
        </p>
      </header>

      <RouterLink
        v-if="!session.mfaEnrolled"
        v-slot="{ navigate }"
        :to="{ name: 'totp-setup' }"
        custom
      >
        <Button variant="default" @click="navigate">{{ t('auth.totp.setup.submit') }}</Button>
      </RouterLink>

      <div v-else-if="!showDisableForm">
        <Button variant="destructive" @click="showDisableForm = true">{{
          t('auth.totp.disable.cta_open')
        }}</Button>
      </div>

      <form v-else novalidate class="grid gap-4" @submit.prevent="onSubmit">
        <FormField
          :label="t('auth.totp.disable.password_label')"
          :error-message="errors.password"
          required
        >
          <template #default="{ id, describedby, invalid }">
            <Input
              :id="id"
              v-model="password"
              v-bind="passwordAttrs"
              type="password"
              autocomplete="current-password"
              required
              :aria-describedby="describedby"
              :invalid="invalid"
            />
          </template>
        </FormField>

        <FormField :label="t('auth.totp.disable.code_label')" :error-message="errors.code" required>
          <template #default="{ id, describedby, invalid }">
            <Input
              :id="id"
              v-model="code"
              v-bind="codeAttrs"
              inputmode="numeric"
              autocomplete="one-time-code"
              maxlength="6"
              required
              :aria-describedby="describedby"
              :invalid="invalid"
            />
          </template>
        </FormField>

        <p v-if="serverMessage" role="alert" class="text-sm text-destructive">
          {{ serverMessage }}
        </p>

        <Button type="submit" variant="destructive" :loading="isSubmitting" class="w-full">{{
          t('auth.totp.disable.submit')
        }}</Button>
      </form>
    </Card>
  </main>
</template>
