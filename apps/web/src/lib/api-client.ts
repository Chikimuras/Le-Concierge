import ky, { type KyInstance } from 'ky'

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
        // Echo back a W3C `traceparent` so spans stitch across tiers.
        // Minimal form: version 00, random trace-id, random parent-id, flags 01.
        // Ref: https://www.w3.org/TR/trace-context/#traceparent-header
        if (!request.headers.has('traceparent')) {
          request.headers.set('traceparent', makeTraceparent())
        }
      },
    ],
    afterResponse: [
      (_request, _options, response) => {
        if (response.status === 401) {
          // TODO(auth): wire up refresh or redirect-to-login here when the
          // auth module lands. Will need to become `async` again at that
          // point. For now the 401 bubbles up to the caller synchronously.
        }
        return response
      },
    ],
  },
})

function makeTraceparent(): string {
  const bytes = new Uint8Array(16)
  crypto.getRandomValues(bytes)
  const traceId = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')

  const parentBytes = new Uint8Array(8)
  crypto.getRandomValues(parentBytes)
  const parentId = Array.from(parentBytes, (b) => b.toString(16).padStart(2, '0')).join('')

  return `00-${traceId}-${parentId}-01`
}
