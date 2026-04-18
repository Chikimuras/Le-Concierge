# ADR 0009 — Team invites

- Status: Accepted
- Date: 2026-04-19
- Deciders: Alexandre Velia (solo)
- Tags: security, auth, tenancy

## Context and Problem Statement

Phase 5b opens organisations to more than one user. A
manager/owner needs to invite a colleague by email; the colleague
follows a link, signs up or logs in, and ends up with a
membership in the inviting organisation with a chosen role. The
tricky part is securing the ~10-minutes-to-7-days window between
"invite created" and "invite consumed" without dragging in a
full-blown email-auth system.

## Decision Drivers

- OWASP ASVS v4.0.3 §2.7 (credential recovery-style flows) — single-
  use tokens, hashed at rest, expiry, constant-time comparison.
- ADR 0008 — the `/orgs/:slug/...` surface and the
  404-over-403 enumeration-safety rule apply.
- Solo dev — no magic, two kinds of acceptance flows only (existing
  user, new user).
- CLAUDE.md §3.1 / §3.3 — no plaintext secrets at rest, no PII in
  logs.

## Decisions

### Token shape and storage

- 32 bytes from `OsRng`, URL-safe base64 (no padding) — 43 chars,
  identical entropy budget to `SessionId` / CSRF tokens
  (ADR 0006). Generated once per invite, returned to the email
  sender exactly once, never stored server-side in plaintext.
- Stored as **HMAC-SHA-256(pepper, token)** hex-encoded in
  `organization_invites.token_hash`. Argon2id is deliberately
  **not** used: the token carries 256 bits of `OsRng` entropy, so
  brute-forcing the pre-image space is infeasible regardless of
  the hash cost — and the linear verify cost Argon2 would impose
  (10-50 active invites × ~50 ms each on every accept) would make
  the flow visibly slow without adding security. The pepper is
  the same `APP_AUTH__PEPPER` used by password hashing; rotating
  it re-keys every invite in the same motion.
- Verification: compute HMAC-SHA-256 of the submitted token,
  query the single matching row via the UNIQUE index on
  `token_hash`. Constant-time comparison is implicit in the
  equality predicate (Postgres `=` over fixed-size `bytea`-like
  hex strings is not a side-channel here — attacker-observable
  latency is dominated by the network round-trip).

### Lifecycle

An invite lives in one of four states:

| State      | Row conditions                              |
| ---------- | ------------------------------------------- |
| Pending    | `accepted_at IS NULL AND cancelled_at IS NULL AND expires_at > now()` |
| Accepted   | `accepted_at IS NOT NULL` — terminal, single-use |
| Cancelled  | `cancelled_at IS NOT NULL` — terminal       |
| Expired    | `expires_at <= now()` on a still-pending row — treated as "gone" on the next access |

A pending invite is unique per `(org_id, email)` via a partial
unique index. Resending = delete-and-recreate (fresh token +
fresh expiry) rather than mutating the existing row.

Default TTL: 7 days, configurable via `APP_AUTH__INVITE_TTL_SECS`.
Shorter TTL is safer; 7 days matches what OSS-Fr team already
tolerates for similar flows (GitHub, Linear, Notion).

### Email-matching rule

Every invite carries a `citext` email column. On acceptance:

- **Authed caller**: the session's user email must match the invite
  email (case-insensitive via `citext`). Mismatch → 404. This
  blocks "forwarded invite" attempts where Alice forwards her
  invite link to Bob — Bob has a valid link but his session
  doesn't line up, so he cannot consume it.
- **Anonymous caller** (signup-and-accept): the new user is created
  with the invite's email. The client does **not** get to pick a
  different address — `POST /auth/invites/signup` takes `{token,
  password}`, email is read from the invite.

### HTTP surface

Three endpoints gate the lifecycle:

- `POST /auth/invites/preview` — **anonymous** — `{token}`, returns
  `{email, org_name, role, expires_at}` or 404/410. Idempotent —
  does not consume. Lets the frontend say "You're being invited as
  Cleaner at Acme" before asking for a password.
- `POST /auth/invites/accept` — **authed** — `{token}`, consumes
  the invite and adds the membership, returns
  `AuthenticatedResponse`.
- `POST /auth/invites/signup` — **anonymous** — `{token, password}`,
  creates a user with the invite's email, consumes the invite,
  mints a session.

Three mutators under `/orgs/:slug/invites` for the manager side:

- `GET /orgs/:slug/invites` — list pending invites. Manager+ only.
- `POST /orgs/:slug/invites` — `{email, role}` → create a new
  pending invite, trigger email delivery. Manager+.
- `DELETE /orgs/:slug/invites/:id` — mark as cancelled. Manager+.

### Error codes

| Condition                               | Status | `kind`          |
| --------------------------------------- | ------ | --------------- |
| Token unknown                           | 404    | `not_found`     |
| Token expired (pending → past TTL)      | 410    | `invite_expired`|
| Email mismatch on authed accept         | 404    | `not_found`     |
| Org not found or caller not manager+    | 404    | `not_found`     |
| Pending invite already exists for email | 409    | `conflict`      |

**Token never appears in a response body, ever.** The only copy
that leaves the server is the invite URL in the email. A leaked
response log cannot be replayed.

### Rate limiting

`POST /auth/invites/preview`, `/accept`, `/signup` sit behind the
same 5/3 min/IP governor config as `/auth/signup` and
`/auth/login`. A leaked token is 43 chars of `OsRng` entropy —
brute force is infeasible regardless, but keeping the limiter on
reduces accidental storm amplification.

### Email delivery

A `crate::email::EmailSender` trait abstracts the transport.
Phase 5b-1 ships `LogEmailSender` which writes the invite URL to
`tracing::info!` — sufficient for solo dev testing. A later ADR
introduces `ResendSender` / `PostmarkSender` when prod delivery
matters. Production configuration that still uses `Log*` is
treated as a deployment bug, surfaced by `Config::log_summary`.

The URL shape: `{public_base_url}/accept-invite?token=<43-char>`.

### Audit

Three events, emitted through the hash-chained `audit_events`
table:

- `invite.created` — `{actor, org_id, email_prefix, role}`
- `invite.accepted` — `{actor, org_id, role}`
- `invite.cancelled` — `{actor, org_id}`

Emails are masked (`a***@b.com`) before hitting the audit payload,
matching the rule from ADR 0005 / CLAUDE.md §3.3.

## Validation

- Unit: token gen + hash round-trip; lifecycle state transitions;
  email match normalisation.
- Integration: full happy paths (authed accept, signup-and-accept);
  email mismatch 404; expired 410; cancel → subsequent accept 404;
  pending uniqueness 409.
- Isolation: `tests/tenant_isolation.rs` extended — user B cannot
  list, create, cancel invites in org A, even with a valid invite
  id guessed.
- `security-review` on the PR (touches `auth/`).

## Related

- ADR 0005 — Auth scheme (Argon2id + pepper reused here)
- ADR 0006 — Sessions / CSRF (token entropy model)
- ADR 0008 — Multi-tenant isolation (404-over-403 rule)
- CLAUDE.md §3.1, §3.3
