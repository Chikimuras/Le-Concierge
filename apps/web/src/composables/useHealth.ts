import { useQuery } from '@tanstack/vue-query'

import { apiClient } from '@/lib/api-client'

/**
 * Shape of the `GET /healthz` response. Mirrors `apps/api/src/health/dto.rs`.
 *
 * When `packages/contracts` lands (future phase), this type will be generated
 * from the OpenAPI document instead of duplicated here.
 */
export interface HealthStatus {
  status: 'ok'
  version: string
  service: string
}

/**
 * Vue Query composable that polls the API health endpoint. Never use
 * `fetch` directly in components — per CLAUDE.md §2.2.
 *
 * Return type is inferred: `UseQueryReturnType<TData, TError>` from
 * `@tanstack/vue-query` is a heavy generic that typescript-eslint sometimes
 * resolves to `any`/error in flat-config + strict-type-checked mode. Letting
 * inference do the work keeps the call sites fully typed.
 */
export function useHealth() {
  return useQuery({
    queryKey: ['health'] as const,
    queryFn: () => apiClient.get('healthz').json<HealthStatus>(),
    staleTime: 15_000,
    retry: 0,
  })
}
