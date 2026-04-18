<script setup lang="ts">
  import { toTypedSchema } from '@vee-validate/zod'
  import QRCode from 'qrcode'
  import { useForm } from 'vee-validate'
  import { computed, onMounted, ref, shallowRef } from 'vue'
  import { useI18n } from 'vue-i18n'
  import { useRouter } from 'vue-router'

  import Button from '@/components/ui/Button.vue'
  import Card from '@/components/ui/Card.vue'
  import FormField from '@/components/ui/FormField.vue'
  import Input from '@/components/ui/Input.vue'
  import { useTotpEnrollStart, useTotpEnrollVerify } from '@/composables/useAuth'
  import { readProblemDetails } from '@/lib/api-client'
  import { totpEnrollVerifySchema } from '@/lib/api-contracts'

  const i18n = useI18n()
  const t = i18n.t
  // `tm` returns the raw message so arrays stay arrays; `t` would
  // stringify `auth.totp.setup.instructions` and `v-for` would then
  // iterate character-by-character. Keep it on the `i18n` object so
  // the `this` binding stays intact.
  const instructions = computed<string[]>(() => i18n.tm('auth.totp.setup.instructions'))
  const router = useRouter()
  const start = useTotpEnrollStart()
  const confirm = useTotpEnrollVerify()

  // --- Enrollment state ---
  const secretBase32 = ref<string | null>(null)
  const qrDataUrl = ref<string | null>(null)
  const startError = shallowRef<string | null>(null)
  const verifyError = ref<string | null>(null)

  // --- Post-enrollment state ---
  const recoveryCodes = ref<string[] | null>(null)
  const acknowledged = ref(false)

  const schema = toTypedSchema(
    totpEnrollVerifySchema({ codeInvalid: t('auth.totp.errors.invalid_code') }),
  )
  const { defineField, errors, handleSubmit, isSubmitting } = useForm({
    validationSchema: schema,
  })
  const [code, codeAttrs] = defineField('code')

  async function beginEnrollment() {
    startError.value = null
    try {
      const data = await start.mutateAsync()
      secretBase32.value = data.secret_base32
      qrDataUrl.value = await QRCode.toDataURL(data.otpauth_url, {
        margin: 1,
        width: 256,
      })
    } catch (err) {
      const problem = await readProblemDetails(err)
      startError.value =
        problem?.kind === 'conflict'
          ? t('auth.totp.setup.already_enrolled')
          : t('auth.totp.errors.generic')
    }
  }

  const onSubmit = handleSubmit(async (values) => {
    verifyError.value = null
    try {
      const data = await confirm.mutateAsync(values.code)
      recoveryCodes.value = data.recovery_codes
    } catch (err) {
      const problem = await readProblemDetails(err)
      verifyError.value =
        problem?.kind === 'rate_limited'
          ? t('auth.totp.errors.rate_limited')
          : t('auth.totp.errors.invalid_code')
    }
  })

  function downloadRecoveryCodes() {
    if (!recoveryCodes.value) return
    const blob = new Blob([recoveryCodes.value.join('\n') + '\n'], { type: 'text/plain' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = 'le-concierge-recovery-codes.txt'
    a.click()
    URL.revokeObjectURL(url)
  }

  function continueToDashboard() {
    void router.replace({ name: 'dashboard' })
  }

  onMounted(() => {
    void beginEnrollment()
  })
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-xl flex-col gap-6 px-4 py-12">
    <header class="space-y-1">
      <h1 class="text-2xl font-semibold">
        {{ recoveryCodes ? t('auth.totp.recovery_codes.title') : t('auth.totp.setup.title') }}
      </h1>
      <p class="text-sm text-muted-foreground">
        {{ recoveryCodes ? t('auth.totp.recovery_codes.intro') : t('auth.totp.setup.subtitle') }}
      </p>
    </header>

    <p v-if="startError" role="alert" class="text-sm text-destructive">{{ startError }}</p>

    <Card v-if="recoveryCodes" class="p-6">
      <p class="rounded-sm bg-destructive/10 px-3 py-2 text-sm text-destructive" role="alert">
        {{ t('auth.totp.recovery_codes.warning') }}
      </p>
      <ul class="mt-4 grid grid-cols-2 gap-2 font-mono text-sm">
        <li
          v-for="rc in recoveryCodes"
          :key="rc"
          class="rounded-sm border border-border bg-muted px-2 py-1 text-center"
        >
          {{ rc }}
        </li>
      </ul>
      <div class="mt-6 flex flex-wrap items-center gap-3">
        <Button variant="outline" type="button" @click="downloadRecoveryCodes">
          {{ t('auth.totp.recovery_codes.download') }}
        </Button>
        <label class="inline-flex items-center gap-2 text-sm">
          <input v-model="acknowledged" type="checkbox" class="size-4" />
          <span>{{ t('auth.totp.recovery_codes.acknowledge') }}</span>
        </label>
        <Button
          type="button"
          :disabled="!acknowledged"
          class="ml-auto"
          @click="continueToDashboard"
        >
          {{ t('auth.totp.recovery_codes.continue') }}
        </Button>
      </div>
    </Card>

    <Card v-else-if="qrDataUrl && secretBase32" class="p-6">
      <section class="grid gap-4">
        <div class="flex flex-col items-center gap-3">
          <img
            :src="qrDataUrl"
            :alt="t('auth.totp.setup.qr_alt')"
            class="rounded-md border border-border bg-white p-2"
            width="256"
            height="256"
          />
        </div>
        <div>
          <h2 class="text-sm font-medium">{{ t('auth.totp.setup.secret_heading') }}</h2>
          <p class="text-sm text-muted-foreground">{{ t('auth.totp.setup.secret_hint') }}</p>
          <code class="mt-2 block break-all rounded-sm bg-muted p-2 font-mono text-sm">{{
            secretBase32
          }}</code>
        </div>
        <ol class="list-decimal space-y-1 pl-5 text-sm text-muted-foreground">
          <li v-for="(step, i) in instructions" :key="i">{{ step }}</li>
        </ol>

        <form novalidate class="grid gap-3" @submit.prevent="onSubmit">
          <FormField :label="t('auth.totp.setup.code_label')" :error-message="errors.code" required>
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

          <p v-if="verifyError" role="alert" class="text-sm text-destructive">{{ verifyError }}</p>

          <Button type="submit" :loading="isSubmitting" class="w-full">{{
            t('auth.totp.setup.submit')
          }}</Button>
        </form>
      </section>
    </Card>
  </main>
</template>
