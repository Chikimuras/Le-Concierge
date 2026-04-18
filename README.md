# Le Concierge

SaaS concierge platform for short-term rentals (Airbnb, Booking.com, VRBO).
Multi-tenant, security-first, 100% OSS stack. Solo-dev maintainable.

> **Status:** Phase 3 — `apps/api` + `apps/web` + a local `docker compose`
> stack (Postgres 16, Redis 7, MinIO; optionally api + Caddy via the `app`
> profile). No migration, no auth yet. See `docs/adr/` for design decisions
> and [`CLAUDE.md`](./CLAUDE.md) for the full project contract.

---

## Stack (summary)

| Layer        | Tech                                                                   |
| ------------ | ---------------------------------------------------------------------- |
| Backend      | Rust (edition 2024), Axum, Tokio, SQLx (PostgreSQL), Redis              |
| Frontend     | Vue 3 (Composition API), Vite, TypeScript strict, Tailwind, shadcn-vue  |
| Auth         | Server sessions in Redis, Argon2id + pepper, TOTP 2FA                   |
| Observability| `tracing` (JSON) → OpenTelemetry → Loki + Grafana + Tempo               |
| Payments     | Stripe (Mollie fallback), signed webhooks                               |
| Infra        | Docker Compose (dev + VPS), Caddy/Traefik + Let's Encrypt               |
| CI/CD        | Gitea Actions (self-hosted Forgejo)                                     |

Full rationale: [`docs/adr/0001-stack-choice.md`](./docs/adr/0001-stack-choice.md).

---

## Prerequisites

- **Rust** — installed via `rustup`; toolchain is pinned by `rust-toolchain.toml`.
- **Bun** — `>=1.1.38` (`curl -fsSL https://bun.sh/install | bash`).
- **Docker** + `docker compose` plugin.
- **just** — task runner (`cargo install just` or your package manager).
- **lefthook** — git hooks (`cargo install lefthook` or `bun add -g lefthook`).

Optional but recommended:
- **sqlx-cli** (`cargo install sqlx-cli --no-default-features --features postgres,rustls`)
- **gitleaks** for local secret scanning
- **mkcert** for local HTTPS (Caddy dev)

---

## Getting started

```bash
# Clone and enter the repo
git clone <repo-url> le-concierge
cd le-concierge

# Install git hooks
lefthook install

# List available tasks
just
```

### Start the backing services (Phase 3)

```bash
cp infra/docker/.env.example infra/docker/.env   # tweak credentials if needed
just compose-up                                  # postgres + redis + minio
just compose-ps                                  # check that all are healthy
```

Services land on loopback only:

| Service   | Host port           | Notes                                  |
| --------- | ------------------- | -------------------------------------- |
| Postgres  | `127.0.0.1:5432`    | `psql` via `just db-psql`              |
| Redis     | `127.0.0.1:6379`    | `redis-cli` via `just redis-cli`       |
| MinIO     | `127.0.0.1:9000`    | S3 API endpoint                        |
| MinIO UI  | `127.0.0.1:9001`    | Web console, see `just minio-console`  |

To tear down (keeping volumes):

```bash
just compose-down
```

To nuke volumes (⚠️ destroys DB data):

```bash
just compose-reset
```

### Run the API (Phase 1)

```bash
cp apps/api/.env.example apps/api/.env   # tweak as needed
just api-run                             # one-shot debug run
#   or:
just api-dev                             # auto-reload (requires cargo-watch)
```

Then:

```bash
curl http://127.0.0.1:3000/healthz
#   → {"status":"ok","version":"0.1.0","service":"api"}

open http://127.0.0.1:3000/docs          # Scalar API reference
```

### Run the web app (Phase 2)

```bash
just web-install                         # bun install at the repo root
just api-run                             # terminal 1: Axum on :3000
just web-dev                             # terminal 2: Vite on :5173
```

Open http://127.0.0.1:5173. The home page polls the API via the Vite
dev proxy (`/api/*` → `:3000`), so no CORS configuration is needed in dev.
Toggle between system / light / dark theme from the buttons at the bottom.

To add a shadcn-vue component:

```bash
cd apps/web
bunx shadcn-vue@latest add button        # or card, input, …
```

### Build the production image

```bash
just api-docker-build                    # multi-stage, distroless, nonroot
just api-docker-run                      # runs it locally, port 3000
```

### Preview the full stack (api + Caddy reverse proxy)

```bash
just compose-up-app                      # adds api + caddy to the stack
open http://127.0.0.1:8080/api/healthz
open http://127.0.0.1:8080               # Caddy → Vite (host)
```

The `app` profile builds the distroless api image and fronts everything
with Caddy. Web is still run natively via `just web-dev` — Caddy reaches
it via `host.docker.internal:5173`.

---

## Repository layout

```
.
├── apps/
│   ├── api/           # Rust backend (Axum) — Phase 1 ✓
│   └── web/           # Vue 3 frontend — Phase 2 ✓
├── packages/
│   ├── contracts/     # TS types generated from OpenAPI + shared Zod schemas
│   └── ui/            # Shared shadcn-vue components (if needed)
├── infra/
│   ├── docker/        # Dockerfiles, compose files, Caddyfile — Phase 3 ✓
│   └── grafana/       # Provisioned dashboards + datasources
├── docs/
│   ├── adr/           # Architecture Decision Records (MADR format)
│   └── rgpd/          # GDPR register and DPIAs
├── Cargo.toml         # Rust workspace root
├── package.json       # Bun workspaces root
├── justfile           # Task runner (added in task 5)
├── lefthook.yml       # Pre-commit hooks
├── CLAUDE.md          # Project contract for AI-assisted work
└── README.md          # You are here
```

---

## Security posture

Security requirements are non-negotiable and documented in [`CLAUDE.md`](./CLAUDE.md)
§3 and [`docs/adr/0002-security-baseline.md`](./docs/adr/0002-security-baseline.md).
Highlights:

- Argon2id password hashing + out-of-DB pepper (OWASP ASVS 2.4)
- Server-side sessions in Redis, HttpOnly/Secure/SameSite cookies
- TOTP 2FA (mandatory for `admin`/`manager`)
- Per-route input validation, parameterized SQL only, strict deserialization
- Immutable audit log with hash-chained integrity
- Encryption-at-rest for sensitive fields; PII masking in all logs
- Secrets via Docker secrets or SOPS+age — never committed

Report a suspected vulnerability **privately** to the copyright holder (contact
in `LICENSE`); do not open a public issue.

---

## License

Proprietary. See [`LICENSE`](./LICENSE).
