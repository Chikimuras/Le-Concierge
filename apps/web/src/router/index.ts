import {
  type NavigationGuardReturn,
  type RouteLocationNormalized,
  type RouteRecordRaw,
  createRouter,
  createWebHistory,
} from 'vue-router'

import HomeView from '@/views/HomeView.vue'
import { useSessionStore } from '@/stores/session'

declare module 'vue-router' {
  interface RouteMeta {
    /** Route is only available to authenticated users; unauth → /login. */
    requiresAuth?: boolean
    /** Route is only available to anonymous users; auth → /dashboard. */
    requiresAnonymous?: boolean
    /** i18n key for the page title. */
    titleKey?: string
    /** Part of the MFA flow — exempt from the MFA enforcement redirects
     *  below so we never loop (you can't send a user to the challenge
     *  page *from* the challenge page). */
    mfaFlow?: boolean
  }
}

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    name: 'home',
    component: HomeView,
    meta: { titleKey: 'pages.home.title' },
  },
  {
    path: '/signup',
    name: 'signup',
    component: () => import('@/views/SignupView.vue'),
    meta: { requiresAnonymous: true, titleKey: 'pages.signup.title' },
  },
  {
    path: '/login',
    name: 'login',
    component: () => import('@/views/LoginView.vue'),
    meta: { requiresAnonymous: true, titleKey: 'pages.login.title' },
  },
  {
    path: '/dashboard',
    name: 'dashboard',
    component: () => import('@/views/DashboardView.vue'),
    meta: { requiresAuth: true, titleKey: 'pages.dashboard.title' },
  },
  {
    path: '/auth/2fa/setup',
    name: 'totp-setup',
    component: () => import('@/views/auth/TotpSetupView.vue'),
    meta: { requiresAuth: true, mfaFlow: true, titleKey: 'pages.totp.setup_title' },
  },
  {
    path: '/auth/2fa/challenge',
    name: 'totp-challenge',
    component: () => import('@/views/auth/TotpChallengeView.vue'),
    meta: { requiresAuth: true, mfaFlow: true, titleKey: 'pages.totp.challenge_title' },
  },
  {
    path: '/settings',
    name: 'settings',
    component: () => import('@/views/SettingsView.vue'),
    meta: { requiresAuth: true, titleKey: 'pages.settings.title' },
  },
  {
    path: '/orgs/:slug/properties',
    name: 'property-list',
    component: () => import('@/views/properties/PropertyListView.vue'),
    meta: { requiresAuth: true, titleKey: 'pages.properties.list_title' },
  },
  {
    path: '/orgs/:slug/properties/new',
    name: 'property-new',
    component: () => import('@/views/properties/PropertyFormView.vue'),
    meta: { requiresAuth: true, titleKey: 'pages.properties.new_title' },
  },
  {
    path: '/orgs/:slug/properties/:id',
    name: 'property-detail',
    component: () => import('@/views/properties/PropertyFormView.vue'),
    meta: { requiresAuth: true, titleKey: 'pages.properties.detail_title' },
  },
]

/**
 * Named-route guard enforcing authentication and the MFA posture
 * (`mfa_required` / `mfa_enrolled` / `mfa_verified`). Exported so
 * integration tests can import the exact same function the real router
 * wires up, instead of reconstructing it inline and silently drifting.
 *
 * Ordering matters: hydrate wait → `requiresAuth` → `requiresAnonymous`
 * → MFA gating. Routes with `mfaFlow: true` are always allowed for an
 * authenticated user so the guard never loops (you can't send a user
 * to the challenge page *from* the challenge page).
 *
 * On the first navigation, the bootstrap's non-blocking
 * `session.hydrate()` may still be in flight. Awaiting here ensures a
 * legitimate session loaded from the `lc_sid` cookie is not mistaken
 * for an anonymous one and bounced to /login.
 */
export async function authGuard(to: RouteLocationNormalized): Promise<NavigationGuardReturn> {
  const session = useSessionStore()

  // Only protected / anonymous-only routes depend on session state.
  // Truly public routes (home) can render without waiting for
  // `/auth/me` so we do not regress the anonymous first-paint budget.
  const needsSession = to.meta.requiresAuth === true || to.meta.requiresAnonymous === true
  if (needsSession && !session.hydrated) {
    await session.hydrate()
  }

  if (to.meta.requiresAuth && !session.isAuthenticated) {
    return { name: 'login', query: { redirect: to.fullPath } }
  }
  if (to.meta.requiresAnonymous && session.isAuthenticated) {
    return { name: 'dashboard' }
  }

  // MFA enforcement: only protected, non-MFA routes redirect. The login
  // flow routes the user into challenge / setup directly on success; this
  // guard covers back-nav, refresh, or deep links that bypass login.
  if (to.meta.requiresAuth && !to.meta.mfaFlow) {
    if (session.needsStepUp) {
      return { name: 'totp-challenge', query: { redirect: to.fullPath } }
    }
    if (session.needsEnrollment) {
      return { name: 'totp-setup' }
    }
  }

  return true
}

export const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes,
})

router.beforeEach(authGuard)
