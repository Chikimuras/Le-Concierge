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
  import { useSignup } from '@/composables/useAuth'
  import { readProblemDetails } from '@/lib/api-client'
  import { ORG_NAME_MAX, PASSWORD_MIN, signupRequestSchema } from '@/lib/api-contracts'

  const { t } = useI18n()
  const signup = useSignup()

  const schema = toTypedSchema(
    signupRequestSchema({
      emailRequired: t('auth.validation.email_required'),
      emailInvalid: t('auth.validation.email_invalid'),
      passwordMin: t('auth.validation.password_min', { min: PASSWORD_MIN }),
      orgNameRequired: t('auth.validation.org_name_required'),
      orgNameTooLong: t('auth.validation.org_name_too_long', { max: ORG_NAME_MAX }),
      slugInvalid: t('auth.validation.slug_invalid'),
    }),
  )

  const { defineField, errors, handleSubmit, isSubmitting } = useForm({
    validationSchema: schema,
  })

  const [email, emailAttrs] = defineField('email')
  const [password, passwordAttrs] = defineField('password')
  const [orgName, orgNameAttrs] = defineField('organization_name')
  const [orgSlug, orgSlugAttrs] = defineField('organization_slug')

  const serverMessage = ref<string | null>(null)

  const onSubmit = handleSubmit(async (values) => {
    serverMessage.value = null
    try {
      await signup.mutateAsync(values)
    } catch (err) {
      const problem = await readProblemDetails(err)
      switch (problem?.kind) {
        case 'conflict':
          serverMessage.value = t('auth.signup.errors.conflict')
          break
        case 'rate_limited':
          serverMessage.value = t('auth.signup.errors.rate_limited')
          break
        default:
          serverMessage.value = t('auth.signup.errors.generic')
      }
    }
  })
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-md items-center justify-center px-4 py-12">
    <Card class="w-full p-6">
      <header class="mb-6 space-y-1">
        <h1 class="text-2xl font-semibold">{{ t('auth.signup.title') }}</h1>
        <p class="text-sm text-muted-foreground">{{ t('auth.signup.subtitle') }}</p>
      </header>

      <form novalidate class="grid gap-4" @submit.prevent="onSubmit">
        <FormField
          :label="t('auth.fields.org_name')"
          :error-message="errors.organization_name"
          required
        >
          <template #default="{ id, describedby, invalid }">
            <Input
              :id="id"
              v-model="orgName"
              v-bind="orgNameAttrs"
              autocomplete="organization"
              required
              :aria-describedby="describedby"
              :invalid="invalid"
            />
          </template>
        </FormField>

        <FormField
          :label="t('auth.fields.org_slug')"
          :hint="t('auth.fields.org_slug_hint')"
          :error-message="errors.organization_slug"
          required
        >
          <template #default="{ id, describedby, invalid }">
            <Input
              :id="id"
              v-model="orgSlug"
              v-bind="orgSlugAttrs"
              autocomplete="off"
              required
              :aria-describedby="describedby"
              :invalid="invalid"
            />
          </template>
        </FormField>

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

        <FormField
          :label="t('auth.fields.password')"
          :hint="t('auth.validation.password_min', { min: PASSWORD_MIN })"
          :error-message="errors.password"
          required
        >
          <template #default="{ id, describedby, invalid }">
            <Input
              :id="id"
              v-model="password"
              v-bind="passwordAttrs"
              type="password"
              autocomplete="new-password"
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
          {{ t('auth.signup.submit') }}
        </Button>
      </form>

      <p class="mt-6 text-center text-sm text-muted-foreground">
        {{ t('auth.signup.has_account') }}
        <RouterLink :to="{ name: 'login' }" class="text-primary hover:underline">
          {{ t('auth.login.cta') }}
        </RouterLink>
      </p>
    </Card>
  </main>
</template>
