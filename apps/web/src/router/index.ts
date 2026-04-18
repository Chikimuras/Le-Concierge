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
]

/**
 * Named-route guard enforcing `requiresAuth` / `requiresAnonymous` meta
 * against the current session. Exported so integration tests can import
 * the exact same function the real router wires up, instead of
 * reconstructing it inline and silently drifting.
 */
export function authGuard(to: RouteLocationNormalized): NavigationGuardReturn {
  const session = useSessionStore()
  if (to.meta.requiresAuth && !session.isAuthenticated) {
    return { name: 'login', query: { redirect: to.fullPath } }
  }
  if (to.meta.requiresAnonymous && session.isAuthenticated) {
    return { name: 'dashboard' }
  }
  return true
}

export const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes,
})

router.beforeEach(authGuard)
