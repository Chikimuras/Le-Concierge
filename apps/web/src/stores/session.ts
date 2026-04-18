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
  const hydrated = ref(false)

  const isAuthenticated = computed(() => userId.value !== null)

  function hasRoleIn(orgId: string, role: Role): boolean {
    return memberships.value.some((m) => m.org_id === orgId && m.role === role)
  }

  /**
   * Populate the store from a successful auth response (signup, login,
   * or `/auth/me`).
   */
  function setFromAuth(payload: AuthenticatedResponse | MeResponse): void {
    userId.value = payload.user_id
    memberships.value = payload.memberships
    isPlatformAdmin.value = payload.is_platform_admin
    csrfToken.value = payload.session.csrf_token
    absoluteExpiresAt.value = payload.session.absolute_expires_at
    mfaVerified.value = payload.session.mfa_verified
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
    hydrated.value = true
  }

  /**
   * Call `/auth/me` at boot. On 200 populates the store; on 401 marks
   * the user as anonymous. Swallows network failures with a warning —
   * the user will see the login screen anyway.
   */
  async function hydrate(): Promise<void> {
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
    }
  }

  return {
    // state
    userId,
    memberships,
    isPlatformAdmin,
    csrfToken,
    absoluteExpiresAt,
    mfaVerified,
    hydrated,
    // getters
    isAuthenticated,
    hasRoleIn,
    // actions
    setFromAuth,
    clear,
    hydrate,
  }
})
