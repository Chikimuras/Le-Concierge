import { defineStore } from 'pinia'
import { computed } from 'vue'
import { useRoute } from 'vue-router'

import type { Role } from '@/lib/api-contracts'
import { useSessionStore } from '@/stores/session'

/**
 * Resolves the org the current view is acting against, based on the
 * `slug` path parameter. The server remains the authority for access
 * checks — this store only drives what the UI renders (per-role
 * affordances, "you're acting as … manager" hints).
 */
export const useActiveOrgStore = defineStore('activeOrg', () => {
  const route = useRoute()
  const session = useSessionStore()

  /** The slug currently in the route, or `null` when no `:slug` param
   *  is present (e.g. on /dashboard). */
  const slug = computed<string | null>(() => {
    const raw = route.params.slug
    if (typeof raw === 'string' && raw.length > 0) return raw
    return null
  })

  /** Membership that matches the active slug, if any. `null` means the
   *  user has no membership in that org — expect a 404 from every
   *  tenant-scoped API call. */
  const membership = computed(() => {
    if (slug.value === null) return null
    return session.memberships.find((m) => m.org_slug === slug.value) ?? null
  })

  const role = computed<Role | null>(() => membership.value?.role ?? null)

  /** Gate UX affordances without second-guessing the backend. Use for
   *  showing / hiding buttons; never for access control. */
  const canManage = computed<boolean>(() => {
    const r = role.value
    return r === 'owner' || r === 'manager' || r === 'admin'
  })

  return { slug, membership, role, canManage }
})
