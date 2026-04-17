# Architecture — Le Concierge

> **Status:** skeleton. Diagrams and sequence flows will grow as domains land.
> This document describes *what exists today* plus *what is planned*; it never
> describes aspirational architecture as if it were real. Every substantive
> change is backed by an ADR in [`./adr/`](./adr).

---

## 1. Purpose

*Le Concierge* is a SaaS platform for concierge agencies managing short-term
rentals across OTAs (Airbnb, Booking.com, VRBO). Each concierge agency is an
isolated tenant (organization) with its own users, properties, guests, and
staff. The platform coordinates reservations, cleanings, check-ins, payments,
and GDPR-compliant data handling.

See [ADR 0001 — Stack technique](./adr/0001-stack-choice.md) for the tech
choices and [`CLAUDE.md`](../CLAUDE.md) §1 for business context.

---

## 2. C4 — Level 1 (System context)

```
                    ┌─────────────────┐         ┌──────────────────┐
                    │   Owner         │         │  Guest           │
                    │ (property host) │         │ (short-term      │
                    └────────┬────────┘         │  renter)         │
                             │                  └────────┬─────────┘
                             │                           │
                             │  browser / mobile web     │ email / SMS
                             ▼                           ▼
   ┌────────────┐    ┌───────────────────────────────────────────┐    ┌──────────────┐
   │  Cleaner   ├───►│               Le Concierge                │◄──►│ Stripe /     │
   │ (operator) │    │  (Vue 3 web app + Rust API + Postgres)    │    │ Mollie       │
   └────────────┘    └───────────┬──────────────┬────────────────┘    └──────────────┘
                                 │              │
                                 │              ├────────────► OTAs (Airbnb,
                                 │              │              Booking.com, VRBO)
                                 │              │              via iCal sync
                                 ▼              ▼
                         ┌──────────────┐  ┌─────────────┐
                         │ Resend /     │  │ MinIO (S3-  │
                         │ Postmark     │  │ compatible) │
                         └──────────────┘  └─────────────┘
```

External systems the platform interacts with:

- **Stripe / Mollie** — card processing, webhook-driven payment state.
- **OTA platforms** — inbound iCal feeds for reservation sync.
- **Resend / Postmark** — transactional email (booking confirmations,
  password resets, 2FA recovery reminders).
- **MinIO** — self-hosted S3-compatible object storage for invoices, photos.

Internal actors (see `CLAUDE.md` §1): `owner`, `manager`, `cleaner`, `guest`,
`admin`.

---

## 3. C4 — Level 2 (Containers)

> Placeholder until Phase 1/2/3 actually exist. Intended layout:

```
┌───────────────────────────────────────────────────────────────────────┐
│                         Single VPS (EU)                               │
│                                                                       │
│  ┌──────────┐   ┌──────────┐   ┌──────────────┐                       │
│  │  Caddy   │──►│  web     │   │    api       │                       │
│  │ (TLS +   │──►│  (static │◄─►│  (Axum,      │                       │
│  │ HSTS)    │   │  Vue)    │   │   Rust)      │                       │
│  └──────────┘   └──────────┘   └──────┬───────┘                       │
│                                       │                               │
│        ┌───────────────────┬──────────┼──────────┬──────────────┐     │
│        ▼                   ▼          ▼          ▼              ▼     │
│  ┌───────────┐       ┌──────────┐ ┌──────┐  ┌──────────┐  ┌────────┐  │
│  │ Postgres  │       │  Redis   │ │MinIO │  │ OTA iCal │  │Workers │  │
│  │ (data +   │       │(sessions │ │      │  │  sync    │  │(jobs)  │  │
│  │  audit)   │       │  cache)  │ │      │  │          │  │        │  │
│  └───────────┘       └──────────┘ └──────┘  └──────────┘  └────────┘  │
│                                                                       │
│  ┌───────────────────────────────────────────────────────────────┐    │
│  │  Observability: tracing → OTel → Loki + Tempo + Prometheus    │    │
│  │                 dashboards in Grafana                         │    │
│  └───────────────────────────────────────────────────────────────┘    │
└───────────────────────────────────────────────────────────────────────┘
```

---

## 4. Domain decomposition

Following `CLAUDE.md` §7.1, Rust modules are organized by **domain**, not by
technical layer. Planned domains:

| Domain       | Responsibility                                             |
| ------------ | ---------------------------------------------------------- |
| `auth`       | Signup, login, 2FA, sessions, password reset               |
| `orgs`       | Organization (tenant) lifecycle, members, roles            |
| `properties` | Properties, rooms, amenities, photos                       |
| `bookings`   | Reservations, conflicts, state machine                     |
| `guests`     | Guest profiles, communication, check-in flow               |
| `cleanings`  | Tasks, assignments, schedules, completion proofs           |
| `billing`    | Stripe/Mollie integration, invoices, subscriptions         |
| `sync`       | iCal import/export, conflict detection (worker-heavy)      |
| `messaging`  | In-app messages, notifications, email dispatch             |
| `audit`      | Immutable event log (hash-chained)                         |
| `admin`      | Platform-level operations (cross-tenant, restricted)       |

Each domain follows the layout in `CLAUDE.md` §7.1:
`domain.rs` → `repo.rs` → `service.rs` → `routes.rs` → `dto.rs`.

---

## 5. Cross-cutting concerns

- **Multi-tenancy**: every table that holds tenant data has `org_id uuid not
  null`, indexed; every handler resolves `org_id` from the authenticated
  session before touching data. Row-level security policies in Postgres
  enforce isolation as a second line of defense.
- **Observability**: `tracing` spans propagate `trace_id` (W3C traceparent),
  `user_id`, `org_id`. JSON logs in production.
- **Security**: see [ADR 0002](./adr/0002-security-baseline.md).
- **GDPR**: export/erase endpoints under `/me/*`; retention documented per
  table in `/docs/rgpd/`.

---

## 6. Deployment topology

Phase 0: none (repository only). Phases 3+ will deploy a single-VPS
`docker compose` stack with:

- Caddy reverse proxy (TLS via Let's Encrypt).
- `api` and `web` containers (distroless, non-root).
- Postgres, Redis, MinIO.
- Grafana/Loki/Tempo/Prometheus sidecars.

Backups: nightly `pg_dump` + MinIO object replication to a second region.

---

## 7. Non-goals (for now)

- Kubernetes / multi-node orchestration.
- Multi-region active-active.
- Dedicated message broker — evaluate RabbitMQ vs NATS only when the first
  truly async workload lands (`sync` domain is the likely candidate).
- Mobile apps — web is mobile-responsive; native apps are post-MVP.

---

*This document is maintained alongside the code. Each structural change must
reference (and ideally create) an ADR.*
