import { useMutation } from '@tanstack/vue-query'
import { useRouter } from 'vue-router'

import { apiClient } from '@/lib/api-client'
import type { AuthenticatedResponse, LoginRequest, SignupRequest } from '@/lib/api-contracts'
import { useSessionStore } from '@/stores/session'

export function useSignup() {
  const session = useSessionStore()
  const router = useRouter()

  return useMutation({
    mutationFn: (input: SignupRequest) =>
      apiClient.post('auth/signup', { json: input }).json<AuthenticatedResponse>(),
    onSuccess: (data) => {
      session.setFromAuth(data)
      void router.replace({ name: 'dashboard' })
    },
  })
}

export function useLogin() {
  const session = useSessionStore()
  const router = useRouter()

  return useMutation({
    mutationFn: (input: LoginRequest) =>
      apiClient.post('auth/login', { json: input }).json<AuthenticatedResponse>(),
    onSuccess: (data) => {
      session.setFromAuth(data)
      const redirect = router.currentRoute.value.query.redirect
      const target = isSameOriginPath(redirect) ? redirect : { name: 'dashboard' as const }
      void router.replace(target)
    },
  })
}

export function useLogout() {
  const session = useSessionStore()
  const router = useRouter()

  return useMutation({
    mutationFn: () => apiClient.post('auth/logout').then(() => null),
    onSuccess: () => {
      session.clear()
      void router.replace({ name: 'login' })
    },
    onError: () => {
      // Even if the server-side call fails, drop the local cache so
      // the user is functionally logged out from the UI's perspective.
      session.clear()
      void router.replace({ name: 'login' })
    },
  })
}

/**
 * Accept a `?redirect=...` query only if it is a same-origin absolute
 * path. Rejects protocol-relative URLs (`//evil.com/...`) and
 * backslash-prefixed paths some browsers treat as scheme-relative —
 * without this guard the login form would be an open redirect.
 */
function isSameOriginPath(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    value.length > 1 &&
    value.startsWith('/') &&
    !value.startsWith('//') &&
    !value.startsWith('/\\')
  )
}
