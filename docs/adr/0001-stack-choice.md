# ADR 0001 — Stack technique

- Status: Accepted
- Date: 2026-04-18
- Deciders: Alexandre Velia (solo)
- Tags: stack, architecture

## Context and Problem Statement

*Le Concierge* is a multi-tenant SaaS for short-term rental concierge
operations (Airbnb, Booking.com, VRBO). Requirements that shaped the stack:

- **Solo developer**: the code must be maintainable by one person indefinitely.
- **Security-critical**: handles payments, PII, and OTA calendar sync.
- **Long-lived**: expected to run for years; prefer stable, conservative tech.
- **EU-hosted, 100% OSS**: no proprietary or US-only managed services in the
  critical path.
- **Real-time**: guest check-ins, payment events, cleaner assignments.

Which backend framework, frontend framework, database, and supporting
infrastructure minimize long-term risk and operational burden while meeting
the security and real-time requirements?

## Decision Drivers

- Memory safety and strong compile-time guarantees (payment + PII code paths).
- Ecosystem maturity and maintainer activity (no abandonware).
- Operational simplicity on a single VPS (Docker Compose, no managed cloud).
- OSS licensing (AGPL or permissive; no SSPL/BSL in the critical path).
- Hiring pool *not* a priority (solo dev).

## Considered Options

1. **Rust (Axum) + Vue 3 + PostgreSQL** — chosen.
2. Go (chi/echo) + Vue 3 + PostgreSQL.
3. Node/TypeScript (Fastify/Nest) + Vue 3 + PostgreSQL.
4. Elixir/Phoenix + LiveView.

## Decision Outcome

**Chosen: Option 1 — Rust (Axum) + Vue 3 + PostgreSQL.**

Rationale:

- Compile-time safety and the `?`/`Result` error model catch most bugs before
  runtime — valuable for payment and auth code paths.
- SQLx's `query!` macros validate SQL against a live schema at build time,
  preventing injection and schema drift (no ORM magic, ref §7.1 of CLAUDE.md).
- Axum is minimal, Tokio-based, widely used, and actively maintained.
- Vue 3 Composition API + TypeScript strict gives fine-grained reactivity
  with a smaller bundle than React equivalents and less boilerplate than
  Svelte's global stores for multi-tenant state.
- PostgreSQL covers relational, JSONB, LISTEN/NOTIFY (cheap pub/sub), and
  full-text search — one DB for everything until scale forces otherwise.
- Docker Compose on a single VPS is enough for the first few hundred tenants;
  defer Kubernetes/managed services until concrete pressure exists (YAGNI).

### Positive Consequences

- Fewer classes of runtime bugs reach production.
- Single deployable binary, small container image (distroless).
- Predictable performance under load (Tokio async runtime).
- Shared Zod schemas between frontend and backend reduce contract drift.

### Negative Consequences

- Compile times slow the inner dev loop; mitigated by `cargo-watch` and
  workspace-aware caching.
- Smaller pool of SaaS boilerplate compared to Node/TS; more glue code
  written by hand.
- `utoipa` + `utoipa-axum` less mature than OpenAPI tooling in Node —
  accepted trade-off.

## Validation

- [ ] Phase 1: API boots, `/healthz` returns 200, OpenAPI served at `/docs`.
- [ ] Phase 2: web app pings the API in dev, strict TS passes.
- [ ] Phase 3: `docker compose up` brings up api + web + postgres + redis + minio.
- [ ] Integration tests run against a real Postgres via `testcontainers-rs`.

## Related

- ADR 0002 — Security baseline
- ADR 0003 — Monorepo layout
- `CLAUDE.md` §2 (stack figée), §7 (conventions de code)
