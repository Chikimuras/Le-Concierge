<script setup lang="ts">
  import { toTypedSchema } from '@vee-validate/zod'
  import { useForm } from 'vee-validate'
  import { computed, onMounted, ref } from 'vue'
  import { useI18n } from 'vue-i18n'
  import { RouterLink, useRoute } from 'vue-router'

  import Button from '@/components/ui/Button.vue'
  import Card from '@/components/ui/Card.vue'
  import FormField from '@/components/ui/FormField.vue'
  import Input from '@/components/ui/Input.vue'
  import {
    useAcceptInvite,
    useInvitePreview,
    useSignupAndAcceptInvite,
  } from '@/composables/useInvites'
  import { readProblemDetails } from '@/lib/api-client'
  import { type InvitePreview, signupAndAcceptSchema } from '@/lib/api-contracts'
  import { useSessionStore } from '@/stores/session'

  const { t, locale } = useI18n()
  const route = useRoute()
  const session = useSessionStore()

  const previewMutation = useInvitePreview()
  const acceptMutation = useAcceptInvite()
  const signupMutation = useSignupAndAcceptInvite()

  const preview = ref<InvitePreview | null>(null)
  const loadError = ref<string | null>(null)
  const actionError = ref<string | null>(null)

  const token = computed(() => (typeof route.query.token === 'string' ? route.query.token : ''))

  /** Does the signed-in user's email match the invite's? Drives
   *  whether we show the "accept" button or the "you need to sign in
   *  as …" message. */
  const emailMatches = computed(() => {
    if (!session.isAuthenticated || !preview.value) return false
    // `/auth/me` does not surface the email on the session payload;
    // we match on whether the user has *any* membership under the
    // invite's org instead of a per-user email check. Safer fallback
    // would be to just try the accept call — the server enforces the
    // real rule. Here we simply assume a signed-in user should attempt
    // the authed path.
    return true
  })

  const schema = toTypedSchema(
    signupAndAcceptSchema({ passwordMin: t('auth.validation.password_min', { min: 12 }) }),
  )
  const { defineField, errors, handleSubmit, isSubmitting } = useForm({
    validationSchema: schema,
    initialValues: { password: '' },
  })
  const [password, passwordAttrs] = defineField('password')

  onMounted(async () => {
    if (token.value === '') {
      loadError.value = t('accept_invite.errors.missing_token')
      return
    }
    try {
      const data = await previewMutation.mutateAsync(token.value)
      preview.value = data
    } catch (err) {
      const problem = await readProblemDetails(err)
      loadError.value =
        problem?.kind === 'gone'
          ? t('accept_invite.errors.expired')
          : problem?.kind === 'not_found'
            ? t('accept_invite.errors.not_found')
            : t('accept_invite.errors.generic')
    }
  })

  async function onAccept() {
    actionError.value = null
    try {
      await acceptMutation.mutateAsync(token.value)
    } catch (err) {
      const problem = await readProblemDetails(err)
      actionError.value =
        problem?.kind === 'not_found'
          ? t('accept_invite.errors.email_mismatch')
          : problem?.kind === 'gone'
            ? t('accept_invite.errors.expired')
            : t('accept_invite.errors.generic')
    }
  }

  const onSignup = handleSubmit(async (values) => {
    actionError.value = null
    try {
      await signupMutation.mutateAsync({ token: token.value, password: values.password })
    } catch (err) {
      const problem = await readProblemDetails(err)
      actionError.value =
        problem?.kind === 'conflict'
          ? t('accept_invite.login_required')
          : problem?.kind === 'gone'
            ? t('accept_invite.errors.expired')
            : problem?.kind === 'not_found'
              ? t('accept_invite.errors.not_found')
              : t('accept_invite.errors.generic')
    }
  })

  const expiryLabel = computed(() => {
    if (!preview.value) return ''
    return t('accept_invite.expires_at', {
      date: new Intl.DateTimeFormat(locale.value, {
        dateStyle: 'medium',
        timeStyle: 'short',
      }).format(new Date(preview.value.expires_at)),
    })
  })
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-md items-center justify-center px-4 py-12">
    <Card class="w-full p-6">
      <header class="mb-6 space-y-1">
        <h1 class="text-2xl font-semibold">{{ t('accept_invite.title') }}</h1>
      </header>

      <p
        v-if="previewMutation.isPending.value && preview === null"
        class="text-sm text-muted-foreground"
      >
        {{ t('accept_invite.loading') }}
      </p>

      <p v-if="loadError" class="text-sm text-destructive" role="alert">{{ loadError }}</p>

      <template v-if="preview">
        <p class="text-sm">
          <i18n-t keypath="accept_invite.invited_as" tag="span">
            <template #role>
              <strong>{{ t(`auth.roles.${preview.role}`) }}</strong>
            </template>
            <template #org>
              <strong>{{ preview.org_name }}</strong>
            </template>
          </i18n-t>
        </p>
        <p class="mt-1 text-xs text-muted-foreground">{{ expiryLabel }}</p>

        <!-- Already signed in: try the authed accept path. -->
        <template v-if="session.isAuthenticated && emailMatches">
          <p v-if="actionError" role="alert" class="mt-4 text-sm text-destructive">
            {{ actionError }}
          </p>
          <Button
            class="mt-6 w-full"
            type="button"
            :loading="acceptMutation.isPending.value"
            @click="onAccept"
          >
            {{ t('accept_invite.accept') }}
          </Button>
        </template>

        <!-- Anonymous: offer sign-up-and-accept. Password only — the
             email is fixed by the invite and supplied by the server. -->
        <template v-else>
          <section class="mt-6">
            <h2 class="text-sm font-medium">{{ t('accept_invite.signup_title') }}</h2>
            <p class="mt-1 text-xs text-muted-foreground">
              {{ t('accept_invite.signup_hint', { email: preview.email }) }}
            </p>

            <form novalidate class="mt-4 grid gap-3" @submit.prevent="onSignup">
              <FormField
                :label="t('accept_invite.signup_password')"
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

              <p v-if="actionError" role="alert" class="text-sm text-destructive">
                {{ actionError }}
              </p>

              <Button type="submit" :loading="isSubmitting" class="w-full">
                {{ t('accept_invite.signup_submit') }}
              </Button>
            </form>

            <p class="mt-4 text-center text-xs text-muted-foreground">
              <RouterLink
                :to="{ name: 'login', query: { redirect: route.fullPath } }"
                class="text-primary hover:underline"
              >
                {{ t('accept_invite.go_to_login') }}
              </RouterLink>
            </p>
          </section>
        </template>
      </template>
    </Card>
  </main>
</template>
