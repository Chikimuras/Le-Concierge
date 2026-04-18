import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'
import { type RouteRecordRaw, createMemoryHistory, createRouter } from 'vue-router'

import type { AuthenticatedResponse } from '@/lib/api-contracts'
import { authGuard } from '@/router'
import { useSessionStore } from '@/stores/session'

const Stub = { template: '<div />' }

const routes: RouteRecordRaw[] = [
  { path: '/', name: 'home', component: Stub },
  { path: '/login', name: 'login', component: Stub, meta: { requiresAnonymous: true } },
  { path: '/signup', name: 'signup', component: Stub, meta: { requiresAnonymous: true } },
  { path: '/dashboard', name: 'dashboard', component: Stub, meta: { requiresAuth: true } },
  { path: '/settings', name: 'settings', component: Stub, meta: { requiresAuth: true } },
  {
    path: '/auth/2fa/setup',
    name: 'totp-setup',
    component: Stub,
    meta: { requiresAuth: true, mfaFlow: true },
  },
  {
    path: '/auth/2fa/challenge',
    name: 'totp-challenge',
    component: Stub,
    meta: { requiresAuth: true, mfaFlow: true },
  },
]

function buildRouter() {
  const router = createRouter({ history: createMemoryHistory(), routes })
  router.beforeEach(authGuard)
  return router
}

function authPayload(overrides: Partial<AuthenticatedResponse> = {}): AuthenticatedResponse {
  return {
    session: {
      user_id: 'u',
      csrf_token: 'c',
      mfa_verified: false,
      created_at: '2026-04-18T10:00:00Z',
      absolute_expires_at: '2026-05-18T10:00:00Z',
    },
    user_id: 'u',
    memberships: [],
    is_platform_admin: false,
    mfa_enrolled: false,
    mfa_required: false,
    ...overrides,
  }
}

describe('router guards', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('redirects anonymous access to a protected route', async () => {
    const router = buildRouter()
    await router.push('/dashboard')
    expect(router.currentRoute.value.name).toBe('login')
    expect(router.currentRoute.value.query.redirect).toBe('/dashboard')
  })

  it('lets an authenticated user into a protected route', async () => {
    const router = buildRouter()
    useSessionStore().setFromAuth(authPayload())
    await router.push('/dashboard')
    expect(router.currentRoute.value.name).toBe('dashboard')
  })

  it('bounces an authenticated user away from /login', async () => {
    const router = buildRouter()
    useSessionStore().setFromAuth(authPayload())
    await router.push('/login')
    expect(router.currentRoute.value.name).toBe('dashboard')
  })

  it('lets an anonymous user reach /login', async () => {
    const router = buildRouter()
    await router.push('/login')
    expect(router.currentRoute.value.name).toBe('login')
  })

  it('redirects an enrolled but not-yet-verified user to the step-up challenge', async () => {
    const router = buildRouter()
    useSessionStore().setFromAuth(
      authPayload({
        mfa_enrolled: true,
        session: {
          user_id: 'u',
          csrf_token: 'c',
          mfa_verified: false,
          created_at: '2026-04-18T10:00:00Z',
          absolute_expires_at: '2026-05-18T10:00:00Z',
        },
      }),
    )
    await router.push('/dashboard')
    expect(router.currentRoute.value.name).toBe('totp-challenge')
    expect(router.currentRoute.value.query.redirect).toBe('/dashboard')
  })

  it('does not loop the challenge redirect on its own route', async () => {
    const router = buildRouter()
    useSessionStore().setFromAuth(
      authPayload({
        mfa_enrolled: true,
      }),
    )
    await router.push('/auth/2fa/challenge')
    expect(router.currentRoute.value.name).toBe('totp-challenge')
  })

  it('redirects a mandatory-2fa unenrolled user to setup', async () => {
    const router = buildRouter()
    useSessionStore().setFromAuth(
      authPayload({
        mfa_required: true,
        mfa_enrolled: false,
      }),
    )
    await router.push('/dashboard')
    expect(router.currentRoute.value.name).toBe('totp-setup')
  })

  it('does not loop the setup redirect on its own route', async () => {
    const router = buildRouter()
    useSessionStore().setFromAuth(
      authPayload({
        mfa_required: true,
        mfa_enrolled: false,
      }),
    )
    await router.push('/auth/2fa/setup')
    expect(router.currentRoute.value.name).toBe('totp-setup')
  })

  it('lets a verified user through to regular protected routes', async () => {
    const router = buildRouter()
    useSessionStore().setFromAuth(
      authPayload({
        mfa_enrolled: true,
        session: {
          user_id: 'u',
          csrf_token: 'c',
          mfa_verified: true,
          created_at: '2026-04-18T10:00:00Z',
          absolute_expires_at: '2026-05-18T10:00:00Z',
        },
      }),
    )
    await router.push('/settings')
    expect(router.currentRoute.value.name).toBe('settings')
  })
})
