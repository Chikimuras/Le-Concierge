<script setup lang="ts">
  import { toTypedSchema } from '@vee-validate/zod'
  import { useForm } from 'vee-validate'
  import { ref } from 'vue'
  import { useI18n } from 'vue-i18n'

  import Button from '@/components/ui/Button.vue'
  import Card from '@/components/ui/Card.vue'
  import FormField from '@/components/ui/FormField.vue'
  import Input from '@/components/ui/Input.vue'
  import { useTotpVerify } from '@/composables/useAuth'
  import { readProblemDetails } from '@/lib/api-client'
  import { totpVerifyRequestSchema } from '@/lib/api-contracts'

  const { t } = useI18n()
  const verify = useTotpVerify()

  const schema = toTypedSchema(
    totpVerifyRequestSchema({ codeInvalid: t('auth.totp.errors.invalid_code') }),
  )
  const { defineField, errors, handleSubmit, isSubmitting } = useForm({
    validationSchema: schema,
  })
  const [code, codeAttrs] = defineField('code')

  const serverMessage = ref<string | null>(null)

  const onSubmit = handleSubmit(async (values) => {
    serverMessage.value = null
    try {
      await verify.mutateAsync({ code: values.code })
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
  <main class="mx-auto flex min-h-dvh max-w-md items-center justify-center px-4 py-12">
    <Card class="w-full p-6">
      <header class="mb-6 space-y-1">
        <h1 class="text-2xl font-semibold">{{ t('auth.totp.challenge.title') }}</h1>
        <p class="text-sm text-muted-foreground">{{ t('auth.totp.challenge.subtitle') }}</p>
      </header>

      <form novalidate class="grid gap-4" @submit.prevent="onSubmit">
        <FormField
          :label="t('auth.totp.challenge.code_label')"
          :error-message="errors.code"
          required
        >
          <template #default="{ id, describedby, invalid }">
            <Input
              :id="id"
              v-model="code"
              v-bind="codeAttrs"
              inputmode="text"
              autocomplete="one-time-code"
              autofocus
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
          {{ t('auth.totp.challenge.submit') }}
        </Button>
      </form>

      <p class="mt-6 text-center text-xs text-muted-foreground">
        {{ t('auth.totp.challenge.recovery_hint') }}
      </p>
    </Card>
  </main>
</template>
