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
  import { useLogin } from '@/composables/useAuth'
  import { readProblemDetails } from '@/lib/api-client'
  import { loginRequestSchema } from '@/lib/api-contracts'

  const { t } = useI18n()
  const login = useLogin()

  const schema = toTypedSchema(
    loginRequestSchema({
      emailRequired: t('auth.validation.email_required'),
      emailInvalid: t('auth.validation.email_invalid'),
      passwordRequired: t('auth.validation.password_required'),
    }),
  )

  const { defineField, errors, handleSubmit, isSubmitting } = useForm({
    validationSchema: schema,
  })

  const [email, emailAttrs] = defineField('email')
  const [password, passwordAttrs] = defineField('password')

  const serverMessage = ref<string | null>(null)

  const onSubmit = handleSubmit(async (values) => {
    serverMessage.value = null
    try {
      await login.mutateAsync({ email: values.email, password: values.password })
    } catch (err) {
      const problem = await readProblemDetails(err)
      switch (problem?.kind) {
        case 'rate_limited':
          serverMessage.value = t('auth.login.errors.rate_limited')
          break
        case 'unauthorized':
          serverMessage.value = t('auth.login.errors.unauthorized')
          break
        default:
          serverMessage.value = t('auth.login.errors.generic')
      }
    }
  })
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-md items-center justify-center px-4 py-12">
    <Card class="w-full p-6">
      <header class="mb-6 space-y-1">
        <h1 class="text-2xl font-semibold">{{ t('auth.login.title') }}</h1>
        <p class="text-sm text-muted-foreground">{{ t('auth.login.subtitle') }}</p>
      </header>

      <form novalidate class="grid gap-4" @submit.prevent="onSubmit">
        <FormField :label="t('auth.fields.email')" :error-message="errors.email" required>
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

        <FormField :label="t('auth.fields.password')" :error-message="errors.password" required>
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

        <p v-if="serverMessage" class="text-sm text-destructive" role="alert">
          {{ serverMessage }}
        </p>

        <Button type="submit" :loading="isSubmitting" class="mt-2 w-full">
          {{ t('auth.login.submit') }}
        </Button>
      </form>

      <p class="mt-6 text-center text-sm text-muted-foreground">
        {{ t('auth.login.no_account') }}
        <RouterLink :to="{ name: 'signup' }" class="text-primary hover:underline">
          {{ t('auth.signup.cta') }}
        </RouterLink>
      </p>
    </Card>
  </main>
</template>
