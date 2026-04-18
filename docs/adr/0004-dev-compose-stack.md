# ADR 0004 — Development compose stack

- Status: Accepted
- Date: 2026-04-18
- Deciders: Alexandre Velia (solo)
- Tags: infra, dev

## Context and Problem Statement

Phase 1 and Phase 2 boot the API and the web app natively (`cargo run` /
`vite`). Before Phase 2 (`auth`, `properties`, …) introduces real database
work, we need a reproducible local environment for the backing services:
PostgreSQL, Redis, MinIO, and (optionally) a reverse proxy that mirrors the
production topology.

## Decision Drivers

- Solo dev: one `just` command to start/stop the whole stack, no manual
  `brew` installs drifting per machine.
- Parity with prod: same PostgreSQL major version, same Redis major, same
  S3-compatible storage — catch version-specific bugs locally.
- Security by default: nothing listens on non-loopback interfaces; dev
  credentials are clearly dev-only.
- Zero CORS pain: the Vite dev proxy handles `/api/*` routing in the
  normal dev flow, so Caddy is optional and gated behind a profile.

## Considered Options

1. **Docker compose stack with service profiles** — chosen.
2. Skip compose, document `brew install postgresql redis` etc.
3. Kubernetes locally (kind/minikube).
4. Nix shell.

## Decision Outcome

**Chosen: Option 1 — `docker compose` with profiles.**

Layout:

```
infra/docker/
├── compose.yaml       # services + volumes + network
├── Caddyfile          # reverse-proxy config (profile: app)
├── .env.example       # dev credentials template
└── .gitignore         # ignore the local .env
```

### Services (always on)

| Service     | Image                | Host port        | Purpose                        |
| ----------- | -------------------- | ---------------- | ------------------------------ |
| `postgres`  | `postgres:16-alpine` | 127.0.0.1:5432   | Relational DB (CLAUDE.md §2.1) |
| `redis`     | `redis:7-alpine`     | 127.0.0.1:6379   | Sessions + cache + rate limit  |
| `minio`     | `minio/minio:latest` | 127.0.0.1:9000/1 | S3-compatible object storage   |

Redis runs with AOF (`--appendonly yes`), so sessions survive container
restarts — matches the CLAUDE.md §3.1 requirement of durable server-side
sessions.

### Services behind `--profile app`

| Service  | Image                              | Host port      | Purpose                           |
| -------- | ---------------------------------- | -------------- | --------------------------------- |
| `api`    | `le-concierge/api:dev` (built)     | 127.0.0.1:3000 | The distroless API binary         |
| `caddy`  | `caddy:2-alpine`                   | 127.0.0.1:8080 | Reverse proxy (`/api` + `/`)      |

Caddy listens HTTP only. TLS is deferred until a real domain exists; see
CLAUDE.md §3.2 which requires TLS in prod, not in dev.

### Ports, volumes, network

- All ports bind to `127.0.0.1:` — never `0.0.0.0:`. Safe on a portable
  machine in coworking spaces, cafés, etc.
- Named volumes: `le_concierge_pg_data`, `le_concierge_redis_data`,
  `le_concierge_minio_data`, `le_concierge_caddy_{data,config}`.
  Reset with `just compose-reset`.
- Single network `le_concierge_dev`, isolated from other compose stacks.

### Credentials

Dev defaults live in `infra/docker/.env.example` (committed). Real values
go in `infra/docker/.env` (gitignored) for local overrides. Production
credentials come from Docker secrets or SOPS+age per CLAUDE.md §3.3 and
ADR 0002 — **never** from these files.

### Positive Consequences

- `just compose-up && just api-run && just web-dev` is the entire dev flow.
- Integration tests (Phase 2 domains) can reuse the same running services,
  or spin up ephemeral ones via `testcontainers-rs` in CI.
- The Caddy profile lets us smoke-test the reverse-proxy routing without
  a production deploy.

### Negative Consequences

- Docker Desktop / Colima is required. Linux users with rootless podman
  may need `podman-compose` adjustments (small, deferred until relevant).
- MinIO image is not pinned to a semver tag (`latest`). Upstream publishes
  rolling releases; we accept drift in dev and will pin for prod.

## Validation

- `just compose-up` brings up the three data services in < 10s, all
  reporting `healthy`.
- `just compose-up-app` additionally builds and runs the distroless api
  image, with Caddy serving `/api/healthz` correctly.
- `just compose-reset` nukes volumes; the next `just compose-up` starts
  from scratch.

## Related

- ADR 0001 — Stack technique (CLAUDE.md §2)
- ADR 0002 — Security baseline (CLAUDE.md §3)
- CLAUDE.md §2.3 (repo structure), §3.1 (sessions), §3.3 (secrets)
