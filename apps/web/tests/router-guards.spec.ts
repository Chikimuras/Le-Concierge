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
]

function buildRouter() {
  const router = createRouter({ history: createMemoryHistory(), routes })
  router.beforeEach(authGuard)
  return router
}

function authPayload(): AuthenticatedResponse {
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
})
