# ADR 0010 — Dev email: Mailpit + SMTP sender, prod deferred

- Status: Accepted
- Date: 2026-04-19
- Deciders: Alexandre Velia
- Tags: email, dev-ex, infra, compose

## Context and Problem Statement

Phase 5b-1 shipped the invite backend with a single [`EmailSender`][1]
implementation, `LogEmailSender`, that writes the invite URL to
`tracing::warn!` and does nothing else. That is the right default for
CI (no external dependency) and for the first five minutes of a solo
dev (one terminal), but it breaks the moment we want to:

- click the link from a realistic MUA (check rendering, check that the
  HTML body is not obviously broken);
- test the UX at `/accept-invite?token=…` against the exact token that
  a real recipient would receive;
- validate that the `invite_url` we assemble (public base URL + query
  param) actually resolves in a browser.

We need a second transport behind the same trait, selected by config,
that works against a local sink. Production email is **out of scope**
for this ADR — we know we will route through Resend or Postmark (no
raw SMTP relay), but picking the provider belongs in a later ADR once
Phase 6 starts.

## Decision Drivers

- **Zero prod risk**: dev-only changes must not create a footgun that
  an operator might accidentally enable in production.
- **Monorepo self-contained**: one `docker compose` stack, no external
  SaaS signup, no per-dev secrets (CLAUDE.md §3.3).
- **Loopback-only by default** (CLAUDE.md §3.2 / ADR 0004).
- **Narrow dep graph**: the Rust client must not pull in hundreds of
  transitive crates. Async-friendly + rustls, because sqlx/reqwest
  already use rustls.

## Considered Options

1. **Mailpit + `lettre` SMTP sender.** Mailpit is the active successor
   to MailHog (same author, stopped MailHog development), ~15 MB
   image, MIT license, single binary, exposes both SMTP (1025) and a
   web UI (8025). `lettre` is the canonical Rust async SMTP client,
   ~400 KB compiled, MIT, current maintainer activity.
2. **MailHog.** Deprecated by its author; last release 2020. Non-
   starter against CLAUDE.md §9.5 (no librairie non maintenue).
3. **Keep only `LogEmailSender`, log the URL.** Cheapest, but
   requires copy-pasting tokens from terminal to browser and gives no
   visibility into HTML rendering. Sufficient for CI, not for the
   dev loop.
4. **Inbucket / smtp4dev / MailCatcher.** Viable alternatives but
   each either predates Mailpit or has a smaller community; no
   concrete win to picking them over the incumbent.
5. **Hit Resend/Postmark directly in dev via a test sender domain.**
   Real product, real quotas, real risk of accidentally hitting a
   real inbox. Rejected — we want loopback-only.

## Decision Outcome

Chosen: **Option 1 — Mailpit in compose + `lettre` `SmtpEmailSender`
selected by `APP_EMAIL__MODE=smtp`.**

### Mechanics

- `infra/docker/compose.yaml` gains a `mailpit` service bound to
  `127.0.0.1:1025` (SMTP) and `127.0.0.1:8025` (UI). No volume: the
  inbox resets on `docker restart`, which is the expected dev
  workflow. `MP_SMTP_AUTH_ACCEPT_ANY=true` +
  `MP_SMTP_AUTH_ALLOW_INSECURE=true` — Mailpit never relays outside
  itself, so "accept any auth" just means "don't fight the client".
- `config.email` gets a new block:
  ```toml
  [email]
  mode = "log"       # default shipped in default.toml
  ```
  Secrets / host info come from env (`APP_EMAIL__MODE`,
  `APP_EMAIL__SMTP_HOST`, `APP_EMAIL__SMTP_PORT`,
  `APP_EMAIL__FROM_ADDRESS`, `APP_EMAIL__FROM_NAME`).
- `email::build_sender(&config.email)` returns the right
  `SharedEmailSender` at boot. Missing required fields in `smtp`
  mode → boot fails with a clear error. `AppState::new` and
  `AppState::from_parts` both call the factory so tests and prod
  follow the same code path.
- `SmtpEmailSender` uses `builder_dangerous` (plaintext SMTP, no
  STARTTLS). This is **intentional for Mailpit only** — we never
  encode real credentials in any config the sender reads, and the
  sender only ever dials 127.0.0.1 or the compose network. The
  production swap point is a different sender module (provider API
  client), not an extension of this one.
- The invite HTML body lives in a single pure helper
  (`render_html`), all interpolations go through a local HTML-escape
  function, and there is a unit test asserting that both the org
  name (tenant-controlled) and the URL (contains `&`) round-trip
  correctly.

### Positive Consequences

- Invites end up in a real inbox UI during dev, at zero setup cost
  beyond `just compose-up`.
- The same abstraction covers CI (`mode = log`, default), dev
  (`mode = smtp` pointing at Mailpit), and future prod (a third
  sender implementation under `mode = resend` or similar).
- Mailpit is loopback-only; even if a dev leaves it running
  indefinitely, nothing escapes the host.

### Negative Consequences

- One more container on the dev stack (~15 MB image, negligible
  CPU/RAM).
- Two sets of config knobs (`log` vs `smtp`) rather than one —
  acceptable because the log path is what CI uses.
- Plaintext SMTP sender in-repo is a footgun if someone copies it
  into a production sender. Mitigated by: `builder_dangerous`
  clearly visible in the code, explicit comment in `SmtpEmailSender`
  saying "dev only", and this ADR pointing at a future dedicated
  prod sender.

## Validation

Signals that would trigger a revisit:

- **Prod readiness.** As soon as we need real email delivery (Phase 6
  notifications / invoices), a dedicated ADR picks a provider and a
  new sender module lands. `SmtpEmailSender` stays for dev.
- **Credential leakage**. If someone proposes adding SMTP
  username/password support to this sender, push back — that is a
  signal they are trying to repurpose it for prod.
- **Mailpit abandonment.** If the project goes six months without
  commits, replace with smtp4dev or similar.

## Related

- ADR 0004 — Dev compose stack (loopback-only services, `.env`
  conventions).
- ADR 0009 — Team invites (what we actually use email for today).
- `CLAUDE.md` §3.2 / §3.3 / §9.5.

[1]: ../../apps/api/src/email/mod.rs
