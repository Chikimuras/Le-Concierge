import { useMutation } from '@tanstack/vue-query'
import { type RouteLocationRaw, useRouter } from 'vue-router'

import { apiClient } from '@/lib/api-client'
import type {
  AuthenticatedResponse,
  EnrollStartResponse,
  EnrollVerifyResponse,
  LoginRequest,
  SignupRequest,
  TotpVerifyResponse,
} from '@/lib/api-contracts'
import { useSessionStore } from '@/stores/session'

export function useSignup() {
  const session = useSessionStore()
  const router = useRouter()

  return useMutation({
    mutationFn: (input: SignupRequest) =>
      apiClient.post('auth/signup', { json: input }).json<AuthenticatedResponse>(),
    onSuccess: (data) => {
      session.setFromAuth(data)
      void router.replace(postAuthTarget(session))
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
      const redirectRaw = router.currentRoute.value.query.redirect
      const redirect = isSameOriginPath(redirectRaw) ? redirectRaw : null
      void router.replace(postAuthTarget(session, redirect))
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

// ---- TOTP 2FA -------------------------------------------------------------

export interface TotpVerifyInput {
  code: string
}

export interface TotpDisableInput {
  password: string
  code: string
}

/** Start (or restart) an enrollment. The response includes the `otpauth://`
 *  URL and the base32 secret — the caller renders a QR from the former
 *  and shows the latter as a manual-entry fallback. */
export function useTotpEnrollStart() {
  return useMutation({
    mutationFn: () => apiClient.post('auth/2fa/enroll/start').json<EnrollStartResponse>(),
  })
}

/** Confirm the first TOTP code from the authenticator. On success the
 *  backend returns the 10 recovery codes (shown exactly once) and marks
 *  the user as `mfa_enrolled` in the next `/auth/me`. The store flips
 *  locally so the router guard sees the change without a round-trip. */
export function useTotpEnrollVerify() {
  const session = useSessionStore()

  return useMutation({
    mutationFn: (code: string) =>
      apiClient.post('auth/2fa/enroll/verify', { json: { code } }).json<EnrollVerifyResponse>(),
    onSuccess: () => {
      // Enrollment succeeded — reflect locally. The user is still
      // `mfa_verified: false` on this session; they clear step-up on
      // first `/auth/2fa/verify`.
      session.mfaEnrolled = true
    },
  })
}

/** Step-up challenge. Accepts a 6-digit TOTP code **or** an 8-char
 *  recovery code (dash optional). On success the backend rotates the
 *  session and returns the new `AuthenticatedResponse` with
 *  `mfa_verified: true`; we feed that back into the store. */
export function useTotpVerify() {
  const session = useSessionStore()
  const router = useRouter()

  return useMutation({
    mutationFn: (input: TotpVerifyInput) =>
      apiClient.post('auth/2fa/verify', { json: input }).json<TotpVerifyResponse>(),
    onSuccess: (data) => {
      session.setFromAuth(data)
      const redirectRaw = router.currentRoute.value.query.redirect
      const redirect = isSameOriginPath(redirectRaw) ? redirectRaw : null
      void router.replace(redirect ?? { name: 'dashboard' })
    },
  })
}

/** Disable 2FA after the user has already cleared step-up. The backend
 *  also destroys the current session, so we clear locally and send the
 *  user back to `/login` — they re-auth without the step-up prompt. */
export function useTotpDisable() {
  const session = useSessionStore()
  const router = useRouter()

  return useMutation({
    mutationFn: (input: TotpDisableInput) =>
      apiClient.post('auth/2fa/disable', { json: input }).then(() => null),
    onSuccess: () => {
      session.clear()
      void router.replace({ name: 'login' })
    },
  })
}

/**
 * Choose where to send the user after a successful authentication. The
 * priority is:
 *
 *   1. Step-up if the user is enrolled but has not cleared MFA yet.
 *   2. Enrollment if the user's role makes 2FA mandatory and they have
 *      none yet.
 *   3. The `?redirect=...` query parameter when it is a safe same-origin
 *      path.
 *   4. Fall back to the dashboard.
 */
function postAuthTarget(
  session: ReturnType<typeof useSessionStore>,
  redirect: string | null = null,
): RouteLocationRaw {
  if (session.needsStepUp) {
    return redirect !== null
      ? { name: 'totp-challenge', query: { redirect } }
      : { name: 'totp-challenge' }
  }
  if (session.needsEnrollment) {
    return { name: 'totp-setup' }
  }
  return redirect ?? { name: 'dashboard' }
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
