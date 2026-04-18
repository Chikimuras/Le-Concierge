# ADR 0006 — Sessions, CSRF, and rate limiting

- Status: Accepted
- Date: 2026-04-18
- Deciders: Alexandre Velia (solo)
- Tags: security, auth

## Context and Problem Statement

Phase 4b-1 adds the HTTP surface for authentication: `/auth/signup`,
`/auth/login`, `/auth/logout`, `/auth/me`. That requires concrete choices
for session storage, identifier format, cookie attributes, CSRF defence,
progressive lockout, and rate-limiting — all mandated in broad terms by
CLAUDE.md §3 but unspecified in detail.

## Decision Drivers

- OWASP ASVS v4.0.3 §3 (session management), §4 (access control), §2.1.1
  (enumeration-safe error messages).
- CLAUDE.md §3.1 (server sessions in Redis, no stateless JWT), §3.2
  (cookie attributes), §10 (anti-patterns).
- Solo dev: prefer a single, well-tread crate per concern, no custom
  crypto.
- Zero runtime overhead for anonymous routes.

## Decisions

### Session identifier

- 32 bytes of `OsRng` entropy, URL-safe base64 without padding
  (43 chars). Implemented by [`SessionId::generate`].
- Stored in Redis under the key `session:<id>`. Never hashed or derived —
  the bearer of the raw cookie value is the session. This matches
  ASVS 3.2.3 (high-entropy random tokens).
- Rejected alternative: UUIDv7 as the SID. Gives us time-ordering we do
  not need, with less entropy than 32 random bytes.

### Session payload

Stored as a JSON blob beside the key:

```
{
  "user_id":          uuid,
  "csrf_token":       43-char base64url,
  "mfa_verified":     bool,
  "created_at":       timestamptz,
  "absolute_expires_at": timestamptz,
  "ip_masked":        "1.2.3.0" | "2001:db8:0::",
  "user_agent_fingerprint": 16-hex-char SHA-256 prefix
}
```

- IP is masked to /24 (IPv4) or /48 (IPv6) before storage, preserving
  enough signal for audit without hoarding precise geolocation
  (CLAUDE.md §3.3, RGPD Art. 5(1)(c)).
- User-Agent is fingerprinted (SHA-256 truncated to 8 bytes hex) — we
  can detect client changes without storing the raw header.

### TTL model (dual)

- **Idle TTL** (`APP_SESSION__IDLE_TTL_SECS`, default 7 days) —
  refreshed on every successful `lookup`. A session left alone for
  longer than this disappears from Redis automatically.
- **Absolute TTL** (`APP_SESSION__ABSOLUTE_TTL_SECS`, default 30 days)
  — computed at `create` time. Even an always-active session dies after
  this hard cap.

The cookie's `Max-Age` tracks the idle TTL so the browser and Redis
drop the session in lockstep.

### Cookie

- Name `lc_sid`. Short, project-namespaced, non-PII.
- Attributes: `HttpOnly`, `Secure` (prod), `SameSite=Lax`, `Path=/`,
  `Max-Age = idle_ttl_secs`. Optional `Domain` when a specific
  sub-domain scope is required.
- `SameSite=Strict` was considered — rejected for the first iteration
  because it breaks a chunk of SSO flows and top-level redirects into
  the app. `Lax` + CSRF token already blocks the CSRF attack classes we
  care about.

### CSRF defence

Double-submit CSRF: the session payload includes a 43-char token
(generated the same way as the SID). The frontend reads it from the
`AuthenticatedResponse` / `/auth/me` body and echoes it in the
`X-CSRF-Token` header on every state-changing request.

The [`session::csrf::guard`] middleware:

1. Bypasses safe methods (GET, HEAD, OPTIONS, TRACE).
2. Lets anonymous unsafe requests through — the handler-level auth
   extractor will 401 if that route required a user.
3. For authenticated unsafe requests, constant-time-compares the header
   against the session token and responds 403 on mismatch.

Comparison is constant-time in byte length (ASVS 4.1.2). Missing
tokens fail identically to wrong tokens — no oracle.

### Rate limiting

`tower_governor` with `SmartIpKeyExtractor` applied only to the
anonymous `/auth/*` router (signup + login). Schedule:

- Burst: 5 requests.
- Replenish: 1 request every 180 s.

This matches the OWASP ASVS 2.2.1 intent of ≈ 5 attempts / 15 min / IP.
Authenticated endpoints (`/auth/logout`, `/auth/me`) are NOT rate-limited
at this layer — `/me` in particular is polled by the frontend.

**Limitation**: the governor state is in-memory. A multi-instance
deployment shares attacker budget across replicas, which an attacker
could exploit by round-robin-ing between instances. When we scale out,
the governor switches to a Redis-backed counter — tracked as tech debt.

### Progressive account lockout

Implemented at the DB layer (`users.failed_login_attempts`,
`users.locked_until`). Windows:

| attempts | cool-off |
| -------- | -------- |
| 1-4      | none     |
| 5-9      | 10 min   |
| 10-19    | 1 h      |
| 20-49    | 1 day    |
| ≥ 50     | 7 days   |

A locked account surfaces the same `401 Unauthorized` as a wrong
password — the client cannot distinguish "wrong password",
"unknown email", or "locked" (enumeration-safe per ASVS 2.1.1).

Successful authentication resets the counter and clears the lockout.

### Audit events

`auth.signup`, `auth.login.success`, `auth.login.failure`,
`auth.logout` are emitted via [`audit::AuditRepo::record`], which takes
a `pg_advisory_xact_lock`, reads the previous hash, and writes a new
row chained by `SHA-256(prev_hash || canonical_row)`.

Emission is **best-effort** for 4b-1: if Postgres temporarily fails at
the audit-insert stage, the user-facing operation still succeeds and
an `ERROR` is logged. We accept the atomicity gap because the alternative
(rolling back a successful login because audit flapped) is worse for
availability. Future tightening plan: signup-grade operations move to
a single transaction that includes the audit insert.

## Validation

- Unit tests in `session::dto` cover SID generation, parse rejection,
  CSRF length-safe compare, IP masking.
- Integration tests in `apps/api/tests/auth.rs` cover the repo-level
  behaviours (lockout schedule, duplicate rejection, memberships).
- End-to-end HTTP tests in `apps/api/tests/auth_http.rs` exercise the
  whole cookie → CSRF → logout cycle over a real server with
  testcontainers Postgres and Redis.

## Related

- ADR 0002 — Security baseline
- ADR 0005 — Auth scheme
- CLAUDE.md §3.1 (auth), §3.2 (transport), §3.3 (data & logs),
  §9.8 (i18n)
