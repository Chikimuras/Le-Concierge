import { defineStore } from 'pinia'
import { computed, ref, shallowRef } from 'vue'

import { apiClient, isHttpError } from '@/lib/api-client'
import type {
  AuthenticatedResponse,
  MembershipSummary,
  MeResponse,
  Role,
} from '@/lib/api-contracts'

/**
 * Holds the hydrated session that the API vouches for via the `lc_sid`
 * cookie. The cookie itself is HttpOnly and not reachable from JS —
 * everything we read here comes from `/auth/me`, `/auth/login`, or
 * `/auth/signup` response bodies.
 *
 * Never treat the state in this store as authoritative for access
 * control: the backend is the source of truth. This is purely a UI
 * cache so we don't round-trip for every rendered badge.
 */
export const useSessionStore = defineStore('session', () => {
  const userId = ref<string | null>(null)
  const memberships = shallowRef<readonly MembershipSummary[]>([])
  const isPlatformAdmin = ref(false)
  const csrfToken = ref<string | null>(null)
  const absoluteExpiresAt = ref<string | null>(null)
  const mfaVerified = ref(false)
  const mfaEnrolled = ref(false)
  const mfaRequired = ref(false)
  const hydrated = ref(false)
  // Outstanding `/auth/me` request, shared across concurrent callers so
  // the router guard's `await session.hydrate()` latches onto the
  // bootstrap's fire-and-forget call instead of firing a second one.
  let hydratePromise: Promise<void> | null = null

  const isAuthenticated = computed(() => userId.value !== null)

  /** 2FA gate: true when the session is allowed past the step-up guard.
   *  An enrolled user must have `mfa_verified`; a non-enrolled user
   *  passes unless their role makes 2FA mandatory. */
  const mfaCleared = computed(() => !mfaEnrolled.value || mfaVerified.value)

  /** The user must go through the enrollment flow before any other
   *  protected action becomes available. Drives the enrollment redirect
   *  in the router guard. */
  const needsEnrollment = computed(() => mfaRequired.value && !mfaEnrolled.value)

  /** The user has 2FA enrolled but has not completed step-up on the
   *  current session yet. Drives the challenge redirect. */
  const needsStepUp = computed(() => mfaEnrolled.value && !mfaVerified.value)

  function hasRoleIn(orgId: string, role: Role): boolean {
    return memberships.value.some((m) => m.org_id === orgId && m.role === role)
  }

  /**
   * Populate the store from a successful auth response (signup, login,
   * `/auth/me`, or `/auth/2fa/verify`).
   */
  function setFromAuth(payload: AuthenticatedResponse | MeResponse): void {
    userId.value = payload.user_id
    memberships.value = payload.memberships
    isPlatformAdmin.value = payload.is_platform_admin
    csrfToken.value = payload.session.csrf_token
    absoluteExpiresAt.value = payload.session.absolute_expires_at
    mfaVerified.value = payload.session.mfa_verified
    mfaEnrolled.value = payload.mfa_enrolled
    mfaRequired.value = payload.mfa_required
    hydrated.value = true
  }

  /**
   * Reset after logout or a 401. Keeps `hydrated = true` because we
   * still queried the server — we just know the answer is "no session".
   */
  function clear(): void {
    userId.value = null
    memberships.value = []
    isPlatformAdmin.value = false
    csrfToken.value = null
    absoluteExpiresAt.value = null
    mfaVerified.value = false
    mfaEnrolled.value = false
    mfaRequired.value = false
    hydrated.value = true
  }

  /**
   * Call `/auth/me` at boot. On 200 populates the store; on 401 marks
   * the user as anonymous. Swallows network failures with a warning —
   * the user will see the login screen anyway.
   *
   * Idempotent: already-hydrated sessions resolve immediately, and an
   * in-flight call is shared across concurrent callers (the bootstrap
   * fires `void hydrate()` while the router guard may `await hydrate()`
   * on the first protected navigation — both must latch onto the same
   * `/auth/me` response, otherwise the guard would decide on a
   * pre-hydrate empty store and bounce a legitimate session to /login).
   */
  function hydrate(): Promise<void> {
    if (hydrated.value) return Promise.resolve()
    if (hydratePromise) return hydratePromise
    hydratePromise = (async () => {
      try {
        const body = await apiClient.get('auth/me').json<MeResponse>()
        setFromAuth(body)
      } catch (err) {
        clear()
        // 401 is the normal anonymous-boot flow; anything else is worth
        // logging (network down, proxy misconfig, backend 5xx).
        if (!isHttpError(err, 401)) {
          console.warn('session hydrate failed', err)
        }
      } finally {
        hydratePromise = null
      }
    })()
    return hydratePromise
  }

  return {
    // state
    userId,
    memberships,
    isPlatformAdmin,
    csrfToken,
    absoluteExpiresAt,
    mfaVerified,
    mfaEnrolled,
    mfaRequired,
    hydrated,
    // getters
    isAuthenticated,
    mfaCleared,
    needsEnrollment,
    needsStepUp,
    hasRoleIn,
    // actions
    setFromAuth,
    clear,
    hydrate,
  }
})
