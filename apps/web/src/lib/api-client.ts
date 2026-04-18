import ky, { HTTPError, type KyInstance } from 'ky'

import { problemDetailsSchema, type ProblemDetails } from '@/lib/api-contracts'

/**
 * Singleton HTTP client. **Never** call `fetch` or instantiate `ky` outside
 * this file — routing every request through here gives us a single place
 * to handle 401 refresh, retry/backoff, and `traceparent` propagation.
 *
 * `baseUrl`:
 * - Dev: `VITE_API_BASE_URL` is unset, so we fall back to `/api`, which the
 *   Vite dev proxy rewrites to `http://127.0.0.1:3000`.
 * - Prod: set `VITE_API_BASE_URL` at build time (either absolute URL or
 *   `/api` if the web app is served under the same origin as the API).
 */
const baseUrl = import.meta.env.VITE_API_BASE_URL ?? '/api'

// RFC 7231 §4.2.1 — safe methods do not mutate server state, so CSRF
// tokens are not required on them.
const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS', 'TRACE'])

/**
 * The session store registers a getter here so the request hook can
 * read the current CSRF token without importing the store (which would
 * be a circular dependency — the store itself uses `apiClient` for
 * `/auth/me`). `null` means "no token known", which is the safe default
 * on boot before `hydrate()` finishes.
 */
let csrfTokenGetter: () => string | null = () => null

export function registerCsrfTokenGetter(getter: () => string | null): void {
  csrfTokenGetter = getter
}

export const apiClient: KyInstance = ky.create({
  prefixUrl: baseUrl.replace(/\/+$/, ''),
  timeout: 15_000,
  retry: {
    limit: 2,
    methods: ['get', 'put', 'head', 'delete', 'options', 'trace'],
    statusCodes: [408, 425, 429, 500, 502, 503, 504],
    backoffLimit: 3_000,
  },
  // Credentialed fetch so session cookies (CLAUDE.md §3.1) travel with
  // requests in both dev (proxy) and prod (same-origin / CORS allowlist).
  credentials: 'include',
  hooks: {
    beforeRequest: [
      (request) => {
        // W3C `traceparent`: version 00, random trace-id, random parent-id, flags 01.
        // Ref: https://www.w3.org/TR/trace-context/#traceparent-header
        if (!request.headers.has('traceparent')) {
          request.headers.set('traceparent', makeTraceparent())
        }

        if (!SAFE_METHODS.has(request.method.toUpperCase())) {
          const token = csrfTokenGetter()
          if (token !== null && token !== '') {
            request.headers.set('x-csrf-token', token)
          }
        }
      },
    ],
  },
})

/**
 * Narrow an unknown error to ky's `HTTPError`, optionally checking the
 * response status. Canonical way to detect API errors — callers should
 * prefer this over `err instanceof Error && 'response' in err`.
 */
export function isHttpError(err: unknown, status?: number): err is HTTPError {
  if (!(err instanceof HTTPError)) return false
  return status === undefined || err.response.status === status
}

/**
 * Parse the RFC 7807 body attached to an `HTTPError`. Returns `null` for
 * non-HTTPError values, malformed bodies, or anything that fails the
 * [`problemDetailsSchema`] contract.
 */
export async function readProblemDetails(err: unknown): Promise<ProblemDetails | null> {
  if (!isHttpError(err)) return null
  try {
    const body: unknown = await err.response.clone().json()
    const parsed = problemDetailsSchema.safeParse(body)
    return parsed.success ? parsed.data : null
  } catch {
    return null
  }
}

function makeTraceparent(): string {
  const bytes = new Uint8Array(16)
  crypto.getRandomValues(bytes)
  const traceId = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')

  const parentBytes = new Uint8Array(8)
  crypto.getRandomValues(parentBytes)
  const parentId = Array.from(parentBytes, (b) => b.toString(16).padStart(2, '0')).join('')

  return `00-${traceId}-${parentId}-01`
}
