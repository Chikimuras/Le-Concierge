# ADR 0002 — Security baseline

- Status: Accepted
- Date: 2026-04-18
- Deciders: Alexandre Velia (solo)
- Tags: security, auth, rgpd

## Context and Problem Statement

*Le Concierge* processes payment data, personal identifying information (PII),
and OAuth tokens for third-party calendar sync. A single breach would be
existential for the business. What is the minimum security posture that the
product must implement from day one, before any feature work?

This ADR defines the non-negotiable floor. Deviations require a new ADR
superseding the relevant section.

## Decision Drivers

- OWASP Application Security Verification Standard (ASVS) v4.0.3 Level 2+.
- OWASP Top 10 2021.
- GDPR (RGPD) Articles 5 (minimization), 17 (erasure), 20 (portability), 32
  (security of processing), 33-34 (breach notification).
- PCI-DSS v4.0 — avoid entering scope by never touching PAN/CVV directly.
- CNIL recommendations on password hashing (2023) and cookies.

## Considered Options

Only the "no security" baseline is rejected; each control below was evaluated
against weaker alternatives (e.g. bcrypt vs Argon2id, JWT vs server sessions)
and justified inline.

## Decision Outcome

The controls below are mandatory. Each references the primary source so it
can be cited in commits and code comments when implemented.

### Authentication

- **Password hashing**: Argon2id with parameters `m=19456, t=2, p=1` (or
  stronger) plus an application-wide **pepper** stored in Docker secrets /
  SOPS, never in the database. Ref: OWASP ASVS 2.4.1, 2.4.2, 2.4.4; OWASP
  Password Storage Cheat Sheet (2024).
- **Sessions**: server-side sessions stored in Redis. JWT is explicitly
  rejected for primary auth because revocation must be immediate (role
  change, password reset, account takeover). Ref: ASVS 3.2, 3.3.
- **Cookies**: `HttpOnly; Secure; SameSite=Lax; Path=/` scoped to the app
  domain. `SameSite=Strict` where feasible (admin console). Ref: ASVS 3.4.1-3.
- **2FA**: TOTP (RFC 6238) mandatory for `admin` and `manager`, optional but
  encouraged for `owner`. Recovery codes generated once, hashed with Argon2id.
  Ref: ASVS 2.7, NIST SP 800-63B §5.1.4.
- **Session ID rotation**: new session identifier issued on every privilege
  elevation (after 2FA, role change). Ref: ASVS 3.2.1.
- **Rate limiting**: `tower-governor` backed by Redis. Stricter limits on
  `/auth/*` (≤ 5 attempts / 15 min / IP, progressive account lockout).
  Ref: ASVS 2.2.1.

### Transport and headers

- **TLS only** in production. Reverse proxy (Caddy or Traefik) with
  Let's Encrypt. Ref: ASVS 9.1.1.
- **Security headers** (always present):
  - `Strict-Transport-Security: max-age=63072000; includeSubDomains; preload`
  - `Content-Security-Policy` — nonce-based, no `unsafe-inline`
  - `X-Content-Type-Options: nosniff`
  - `X-Frame-Options: DENY`
  - `Referrer-Policy: strict-origin-when-cross-origin`
  - `Permissions-Policy` — camera/mic/geolocation denied by default
  Ref: OWASP Secure Headers Project; ASVS 14.4.
- **CORS**: explicit allow-list of origins. `*` with credentials is forbidden.
  Ref: ASVS 14.5.3.

### Data and logs

- **Encryption at rest** for sensitive columns (OTA OAuth tokens, Stripe
  webhook secrets held client-side, non-trivial PII) using AES-256-GCM via
  `ring` or `aes-gcm`. Key material lives in Docker secrets / SOPS and is
  rotatable. Ref: ASVS 6.2, 6.3.
- **Immutable audit log**: `audit_events` table with triggers that reject
  `UPDATE` and `DELETE`, plus a chained hash (`hash = H(prev_hash || row)`)
  to detect tampering. Logged events: login/logout, role changes, data
  export, admin actions, payment operations. Ref: ASVS 7.1.1-4, 10.2.1.
- **No PII in application logs**: structured `tracing` filter masks emails
  (`a***@example.com`), never logs IBAN/PAN/CVV. Ref: ASVS 7.3.
- **Secrets management**: `.env` gitignored; production uses Docker secrets
  or SOPS+age. `gitleaks` runs pre-commit and in CI. Ref: ASVS 14.1.4.

### Validation and injection

- **Input validation on every route** using typed Axum extractors plus the
  `validator` crate (backend) and Zod (frontend). Reject before business
  logic runs. Ref: ASVS 5.1, OWASP Top 10 A03.
- **Parameterized SQL only** via SQLx `query!`/`query_as!`. String
  concatenation into SQL is a bug. Ref: ASVS 5.3.4.
- **Strict deserialization**: every input DTO uses
  `#[serde(deny_unknown_fields)]`. Ref: ASVS 5.1.5.

### Payments (PCI-DSS scope reduction)

- PAN, CVV, and magnetic-stripe data are **never** stored, logged, or even
  received raw. All card input flows through Stripe Elements or Mollie
  Components which tokenize client-side. Ref: PCI-DSS SAQ-A eligibility.
- Webhooks verified with constant-time signature compare. Idempotency-key
  on every mutating payment call. Ref: Stripe webhook signing docs.

### GDPR

- Export endpoint (JSON) and erasure endpoint for user data.
  Ref: RGPD Art. 17, 20.
- Minimization by default — only collect what each feature requires.
  Retention durations documented per table.
  Ref: RGPD Art. 5(1)(c), 5(1)(e).
- Processing register in `/docs/rgpd/`. DPIA for high-risk processing
  recorded as an ADR. Ref: RGPD Art. 30, 35.

## Validation

Each control is validated when the relevant feature lands. This ADR is the
checklist; the `security-review` skill is run before merging any PR that
touches `auth/`, `billing/`, or `infra/`.

## Related

- ADR 0001 — Stack technique
- `CLAUDE.md` §3 (sécurité non-négociable), §10 (anti-patterns interdits)
