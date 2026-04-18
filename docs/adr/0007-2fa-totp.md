# ADR 0007 — TOTP 2FA

- Status: Accepted
- Date: 2026-04-18
- Deciders: Alexandre Velia (solo)
- Tags: security, auth

## Context and Problem Statement

Phase 4c adds the second factor to the auth stack. CLAUDE.md §3.1 makes
TOTP mandatory for `admin` and `manager`, optional for `owner`, and
silent for `cleaner` / `guest`. Recovery codes must be hashed at rest.
ADR 0005 already commits us to `totp-rs`; ADR 0006 wired
`session.mfa_verified` through the payload but never flipped it. This
ADR freezes the remaining choices — TOTP parameters, recovery-code
shape, secret-at-rest encryption, enforcement model, and the
login → verify rotation — so the implementation can land without
re-litigating them.

References: OWASP ASVS v4.0.3 §2.7 (MFA), §3.2.1 (SID rotation on
privilege elevation), §6.2 (cryptography at rest); RFC 6238 (TOTP); NIST
SP 800-63B §5.1.4; CLAUDE.md §3.1; ADR 0002 (security baseline); ADR
0005 (auth scheme); ADR 0006 (sessions / CSRF).

## Decision Drivers

- Solo dev: minimise the number of primitives. `totp-rs` handles RFC
  6238; `aes-gcm` handles the symmetric wrap; `argon2` already hashes
  both passwords and recovery codes.
- No custom crypto. Secret storage must resist a DB-only compromise.
- UX: the flow has to accommodate an authenticator switch (lost phone
  → recovery code) without shipping a second channel (SMS forbidden per
  NIST SP 800-63B §5.1.3.3).
- Enumeration safety carries over from ADR 0005: responses must not
  disclose whether an account has 2FA enrolled before the password is
  verified.

## Considered Options

### Session model after login for a 2FA-enabled user

1. **Single session, `mfa_verified` flag + SID rotation on verify** — chosen.
2. Separate "pre-MFA" session cookie (`lc_sid_pre`) that is exchanged
   for the real cookie on verify — more state, more cookie surface,
   doesn't add real security over a flag + rotation.
3. Short-lived bearer token returned in the login body, POSTed to
   `/auth/2fa/verify`, server issues the cookie on success — an extra
   moving part with no win on the Lax+CSRF baseline we already have.

### TOTP secret storage

1. **AES-256-GCM with a dedicated key via `APP_AUTH__TOTP_KEY`** — chosen.
2. Store with the Argon2 pepper — conflates two different key roles;
   rotating the pepper would break every enrolled 2FA at once.
3. Plain `bytea` (defence only via DB permissions) — fails CLAUDE.md
   §3.3 on encryption-at-rest for sensitive columns.

### Recovery code format

1. **10 codes of 8 alphanumeric chars, rendered `XXXX-XXXX`, hashed
   Argon2id + pepper, single-use** — chosen.
2. 6-digit numeric — too low entropy (~20 bits), trivially brute-forced
   without lockout.
3. Long UUIDs — copy-paste hostile, users will skip copying them.

### Enrollment enforcement

1. **Role-driven `mfa_required` flag in `/auth/me`; client forces
   enrollment before other protected actions; server middleware rejects
   the affected routes with `403 { kind: "mfa_required" }`** — chosen.
2. Lock the session until 2FA is enrolled — breaks the flow where a
   brand-new manager has to copy the otpauth URL from `/auth/me` itself.
3. Server-side redirect — REST handlers don't redirect; problem-details
   lets the SPA choose the route.

## Decision Outcome

### TOTP parameters

- Algorithm `SHA1`, 6 digits, 30-second step. This is the RFC 6238
  default and the only combination that every authenticator app
  (Google Authenticator, 1Password, Bitwarden, Aegis…) reads without
  configuration. Stronger variants (SHA256/SHA512, 8 digits) drop
  compatibility with Google Authenticator silently.
- Verification window `±1 step` (30 s grace either side of the
  server-computed code) — absorbs modest clock drift without widening
  the attack window dangerously.
- Secret length: 20 bytes from `OsRng`, base32-encoded when rendered in
  the `otpauth://` URL per the de-facto Key URI spec.
- `otpauth://totp/Le%20Concierge:<email>?secret=<base32>&issuer=Le%20Concierge&algorithm=SHA1&digits=6&period=30`

### Single session with SID rotation on `verify`

- `POST /auth/login` on a user with active 2FA returns `200` with the
  normal `AuthenticatedResponse`, but `session.mfa_verified = false`.
- The session is **usable only** to call `/auth/2fa/verify`,
  `/auth/logout`, and `/auth/me`. Every other authenticated route must
  be gated by a `require_mfa` extractor that responds
  `403 { kind: "mfa_required" }` when `!mfa_verified`. For Phase 4c-1
  the extractor is wired on `/auth/2fa/disable` and nothing else —
  tenant-scoped routes pick it up as they land.
- On successful `POST /auth/2fa/verify`, the server **rotates the SID**
  (destroy old + create new) and sets `mfa_verified = true`.
  Recovery-code consumption follows the same rotation path. This is
  ASVS 3.2.1 for privilege elevation.
- The new session inherits the old session's `user_id`, `ip_masked`,
  `user_agent_fingerprint`, and recomputes `created_at` /
  `absolute_expires_at` from scratch — a fresh 30-day absolute
  lifetime after MFA.

### TOTP secret encryption at rest

- `aes-gcm` crate (pure Rust, constant-time primitives).
- Key: 32 bytes random, loaded from `APP_AUTH__TOTP_KEY` (hex-encoded
  in env / Docker secret). API fails closed at boot if the variable is
  missing or cannot decode to 32 bytes — same posture as the pepper
  (ADR 0005).
- Per-row random 12-byte nonce from `OsRng`. Ciphertext column layout:
  `nonce (12 bytes) || ciphertext (20 bytes) || tag (16 bytes)` = 48
  bytes. Authenticated with no associated data (the user-id is the
  primary key, and using it as AAD would forbid row moves on backfills
  without extra work).
- Rotation: a future migration adds a `key_version` column and a
  background job that decrypts-and-reencrypts. Documented as debt;
  acceptable for phase 4c.

### Recovery codes

- Generated once, on `POST /auth/2fa/enroll/verify`, after the TOTP
  code proves the authenticator is correctly paired. Returned in the
  response body **once** (the UI is responsible for making the user
  save them) — never queryable again.
- Content: 10 codes of 8 chars each, sampled uniformly from a
  `base32` alphabet without the confusable pair `0` / `O` and `1` / `I`
  (so the effective alphabet is 30 symbols — ~40 bits per code). Each
  rendered to the user as `XXXX-XXXX`; the dash is stripped before
  hashing so users can retype with or without it.
- Each code is hashed with the same Argon2id configuration as
  passwords (parameters, pepper). Stored in
  `user_totp_recovery_codes` alongside `used_at`. Verification
  iterates the user's **unused** rows, runs `verify_password` against
  each; on a single match, marks that row as used in the same
  transaction as the SID rotation.
- Using a recovery code consumes it. The user is advised (via i18n copy
  shipped with 4c-2) to re-enroll and regenerate codes afterwards.
- The timing of the verify loop is acceptable: at most 10 Argon2
  operations per call. Rate limiting (`5 req / 1 min / IP`) is enough
  to keep the wall-clock cost bounded.

### `otpauth://` URL and QR rendering

- The API returns the raw `otpauth://` URL *and* the base32 secret in
  the enrollment start response. The frontend is responsible for
  rendering the QR (e.g. `qrcode` npm package) — the backend stays
  headless and ships no pixel-rendering code.
- The URL is generated once per pending enrollment; it is **not**
  regenerated on each GET, because the secret is already committed to
  `user_totp` at that point. Cancelling an enrollment (implicit by
  starting another one, or explicit via `disable`) drops the row.

### Audit events

Six new `kind` values emitted through `AuditRepo::record`:

- `auth.2fa.enroll.start`
- `auth.2fa.enroll.success`
- `auth.2fa.verify.success`
- `auth.2fa.verify.failure`
- `auth.2fa.recovery.used`
- `auth.2fa.disable`

Payloads never include the secret, the codes, or the raw TOTP
submission. They record the masked IP, the user-agent fingerprint, and
the reason on failures (`"wrong_code"`, `"window_miss"`,
`"recovery_consumed"`).

### `/auth/me` additions

The response body gains three fields:

- `mfa_enrolled: bool` — true when `user_totp.enrolled_at IS NOT NULL`
  and `disabled_at IS NULL`.
- `mfa_required: bool` — true when the user holds a membership with
  role `manager`, or is a row in `platform_admins`. Computed from the
  same query that already builds `memberships` + `is_platform_admin`.
- `mfa_verified: bool` — mirrors `session.mfa_verified`.

The frontend uses `(mfa_required && !mfa_enrolled) || (mfa_enrolled
&& !mfa_verified)` to decide whether to route to the enrollment page
or the step-up challenge.

## Validation

- Unit: AES-GCM round-trip (including tamper detection — flipping a
  tag byte must fail), TOTP code generation against a fixed-time
  clock, recovery-code alphabet excludes confusable chars, verify
  loop rejects a reused code.
- Integration: enroll-pending → verify → /auth/me (`mfa_verified:
  true`, **new** SID, old SID gone from Redis); recovery code consumed
  on first use and rejected on reuse; disable clears both tables.
- HTTP: E2E login → 200 with `mfa_verified: false`; protected
  2fa/disable route returns 403 `{kind: "mfa_required"}`; verify
  then 204 on disable; rate-limit on `/auth/2fa/verify` at
  5-req burst.
- Security-review: run `security-review` on the full diff before merge
  per CLAUDE.md §7.3.

## Related

- ADR 0002 — Security baseline (§Authentication)
- ADR 0005 — Auth scheme (Argon2id, pepper, audit chain)
- ADR 0006 — Sessions, CSRF, lockout
- CLAUDE.md §3.1 (2FA), §3.3 (encryption at rest), §9.8 (i18n)
