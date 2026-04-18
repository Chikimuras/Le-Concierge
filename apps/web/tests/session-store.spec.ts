import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import type { AuthenticatedResponse } from '@/lib/api-contracts'
import { useSessionStore } from '@/stores/session'

function authPayload(): AuthenticatedResponse {
  return {
    session: {
      user_id: '11111111-1111-4111-8111-111111111111',
      csrf_token: 'a'.repeat(43),
      mfa_verified: false,
      created_at: '2026-04-18T10:00:00Z',
      absolute_expires_at: '2026-05-18T10:00:00Z',
    },
    user_id: '11111111-1111-4111-8111-111111111111',
    memberships: [
      {
        org_id: '22222222-2222-4222-8222-222222222222',
        org_slug: 'acme',
        org_name: 'Acme',
        role: 'owner',
      },
    ],
    is_platform_admin: false,
  }
}

describe('session store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('starts unauthenticated and not hydrated', () => {
    const s = useSessionStore()
    expect(s.isAuthenticated).toBe(false)
    expect(s.userId).toBeNull()
    expect(s.csrfToken).toBeNull()
    expect(s.hydrated).toBe(false)
  })

  it('setFromAuth populates everything and flips isAuthenticated', () => {
    const s = useSessionStore()
    s.setFromAuth(authPayload())
    expect(s.isAuthenticated).toBe(true)
    expect(s.userId).toBe('11111111-1111-4111-8111-111111111111')
    expect(s.csrfToken).toHaveLength(43)
    expect(s.memberships).toHaveLength(1)
    expect(s.hydrated).toBe(true)
  })

  it('clear wipes the user but marks hydrated', () => {
    const s = useSessionStore()
    s.setFromAuth(authPayload())
    s.clear()
    expect(s.isAuthenticated).toBe(false)
    expect(s.userId).toBeNull()
    expect(s.csrfToken).toBeNull()
    expect(s.memberships).toHaveLength(0)
    // We *did* answer the "is the user logged in?" question — answer is no.
    expect(s.hydrated).toBe(true)
  })

  it('hasRoleIn checks the right tuple', () => {
    const s = useSessionStore()
    s.setFromAuth(authPayload())
    expect(s.hasRoleIn('22222222-2222-4222-8222-222222222222', 'owner')).toBe(true)
    expect(s.hasRoleIn('22222222-2222-4222-8222-222222222222', 'manager')).toBe(false)
    expect(s.hasRoleIn('00000000-0000-0000-0000-000000000000', 'owner')).toBe(false)
  })
})
