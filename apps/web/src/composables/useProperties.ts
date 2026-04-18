import { useMutation, useQuery, useQueryClient } from '@tanstack/vue-query'
import { type Ref, computed } from 'vue'
import { useRouter } from 'vue-router'

import { apiClient } from '@/lib/api-client'
import type { Property, PropertyListResponse } from '@/lib/api-contracts'

// ---- Queries --------------------------------------------------------------

export function useProperties(slug: Ref<string>) {
  return useQuery({
    queryKey: computed(() => ['properties', slug.value] as const),
    queryFn: () => apiClient.get(`orgs/${slug.value}/properties`).json<PropertyListResponse>(),
    // Memberships rarely change mid-session; 30 s staleTime matches
    // the app default and keeps the list snappy across navigations.
    staleTime: 30_000,
  })
}

export function useProperty(slug: Ref<string>, id: Ref<string>) {
  return useQuery({
    queryKey: computed(() => ['properties', slug.value, id.value] as const),
    queryFn: () => apiClient.get(`orgs/${slug.value}/properties/${id.value}`).json<Property>(),
    staleTime: 30_000,
  })
}

// ---- Mutations ------------------------------------------------------------

export interface PropertyPayload {
  slug: string
  name: string
  timezone?: string
  address_line1?: string
  address_line2?: string
  city?: string
  postal_code?: string
  country?: string
  bedrooms?: number | null
  max_guests?: number | null
  notes?: string
}

/** POST /orgs/:slug/properties — invalidates the list on success and
 *  routes the caller to the new property's detail page. */
export function useCreateProperty(orgSlug: Ref<string>) {
  const qc = useQueryClient()
  const router = useRouter()

  return useMutation({
    mutationFn: (payload: PropertyPayload) =>
      apiClient.post(`orgs/${orgSlug.value}/properties`, { json: clean(payload) }).json<Property>(),
    onSuccess: (created) => {
      void qc.invalidateQueries({ queryKey: ['properties', orgSlug.value] })
      void router.replace({
        name: 'property-detail',
        params: { slug: orgSlug.value, id: created.id },
      })
    },
  })
}

/** PATCH /orgs/:slug/properties/:id — refreshes both the list and the
 *  detail cache. The server ignores `undefined` fields (COALESCE). */
export function useUpdateProperty(orgSlug: Ref<string>, id: Ref<string>) {
  const qc = useQueryClient()

  return useMutation({
    mutationFn: (payload: Partial<PropertyPayload>) =>
      apiClient
        .patch(`orgs/${orgSlug.value}/properties/${id.value}`, { json: clean(payload) })
        .json<Property>(),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['properties', orgSlug.value] })
      void qc.invalidateQueries({ queryKey: ['properties', orgSlug.value, id.value] })
    },
  })
}

/** DELETE /orgs/:slug/properties/:id — clears the local cache and
 *  returns to the list. 204 is the success contract. */
export function useDeleteProperty(orgSlug: Ref<string>) {
  const qc = useQueryClient()
  const router = useRouter()

  return useMutation({
    mutationFn: (id: string) =>
      apiClient.delete(`orgs/${orgSlug.value}/properties/${id}`).then(() => null),
    onSuccess: (_data, id) => {
      void qc.invalidateQueries({ queryKey: ['properties', orgSlug.value] })
      qc.removeQueries({ queryKey: ['properties', orgSlug.value, id] })
      void router.replace({ name: 'property-list', params: { slug: orgSlug.value } })
    },
  })
}

// Strip keys with `undefined` / `null` / `""` so the JSON body stays
// compact and the backend's "don't-touch" COALESCE semantics apply to
// every optional field uniformly.
function clean<T extends object>(obj: T): Partial<T> {
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
    if (v === undefined || v === null || v === '') continue
    out[k] = v
  }
  return out as Partial<T>
}
