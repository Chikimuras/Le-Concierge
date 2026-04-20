# Task runner for Le Concierge. Invoke with `just <recipe>`.
#
# Keep in lockstep with the CI workflow (phase 4).
#
# Ref: https://just.systems

set shell := ["bash", "-cu"]
set dotenv-load := false

# Compose v2 auto-loads `infra/docker/.env` (sibling of the compose file).
# If it does not exist, `${VAR:-default}` fallbacks in compose.yaml kick in.
COMPOSE := "docker compose -f infra/docker/compose.yaml"

# Default recipe: list all available recipes.
default:
    @just --list

# --- Dev environment ---------------------------------------------------------

# Install required git hooks via lefthook.
hooks-install:
    lefthook install

# Remove installed git hooks.
hooks-uninstall:
    lefthook uninstall

# --- Lint & format -----------------------------------------------------------

# Format every language in the repo. Safe to run before committing.
fmt:
    cargo fmt --all
    bun run --filter './apps/web' format

# Run every linter in the repo. Fails on any warning (CI parity).
lint:
    cargo clippy --all-targets --locked -- -D warnings
    bun run --filter './apps/web' lint
    bun run --filter './apps/web' typecheck

# --- Tests -------------------------------------------------------------------

# Run the full test suite (unit + integration + front). Slow.
test:
    cargo test --locked --workspace
    bun run --filter './apps/web' test

# --- Security ---------------------------------------------------------------

# Scan the working tree for leaked secrets.
secrets-scan:
    gitleaks detect --config .gitleaks.toml --verbose --redact

# Audit Rust dependencies for known CVEs and policy violations.
audit:
    cargo audit
    cargo deny check

# --- API (apps/api) ---------------------------------------------------------

# Run the api in watch mode (requires `cargo-watch` — `cargo install cargo-watch`).
# `cd` first so `dotenvy` picks up `apps/api/.env` from the crate dir.
api-dev:
    cd apps/api && cargo watch -x run

# One-shot debug run of the api.
api-run:
    cd apps/api && cargo run

# Release build, single binary.
api-build:
    cd apps/api && cargo build --release

# Focused tests for the api crate.
api-test:
    cd apps/api && cargo test --locked

# Build the production Docker image.
api-docker-build:
    docker build -f apps/api/Dockerfile -t le-concierge/api:dev .

# Run the production Docker image, exposing port 3000 locally.
api-docker-run: api-docker-build
    docker run --rm -it --name le-concierge-api -p 3000:3000 \
        --env-file apps/api/.env.example \
        le-concierge/api:dev

# --- Compose (infra/docker) -------------------------------------------------

# Start the data services (postgres, redis, minio) in the background.
compose-up:
    {{COMPOSE}} up -d postgres redis minio mailpit

# Start everything, including the api container and the Caddy reverse proxy.
# Use this to sanity-check the distroless image + routing before a deploy.
compose-up-app:
    {{COMPOSE}} --profile app up -d --build

# Tail logs from every running service (Ctrl-C to detach, services keep running).
compose-logs:
    {{COMPOSE}} logs -f --tail=100

# Show the status of every compose-managed container.
compose-ps:
    {{COMPOSE}} ps

# Stop and remove the containers. Volumes are preserved — data survives.
compose-down:
    {{COMPOSE}} down

# ⚠️ Destructive: stop, remove, and DELETE named volumes (DB data lost).
compose-reset:
    {{COMPOSE}} down -v

# Interactive `psql` shell on the running postgres container.
db-psql:
    {{COMPOSE}} exec postgres psql \
        -U ${POSTGRES_USER:-le_concierge} \
        -d ${POSTGRES_DB:-le_concierge_dev}

# --- Database migrations (sqlx-cli) ----------------------------------------
# Requires `cargo install sqlx-cli --no-default-features --features postgres,rustls`.
#
# `sqlx-cli` needs `DATABASE_URL`. These recipes read it from the shell
# env first (CI / ops set it explicitly), else fall back to the
# `APP_DATABASE__URL` entry in `apps/api/.env` so local dev "just works"
# without the `export DATABASE_URL=...` dance.
_db_url := '${DATABASE_URL:-$(grep ^APP_DATABASE__URL= apps/api/.env | cut -d= -f2-)}'

# Apply every pending migration under apps/api/migrations/.
db-migrate:
    DATABASE_URL="{{_db_url}}" sqlx migrate run --source apps/api/migrations

# Revert the last migration. Forward-only is the rule (CLAUDE.md §10),
# so this recipe is here only for emergencies on a dev DB you don't mind
# nuking. Never run against production.
db-migrate-revert:
    DATABASE_URL="{{_db_url}}" sqlx migrate revert --source apps/api/migrations

# List applied and pending migrations.
db-migrate-info:
    DATABASE_URL="{{_db_url}}" sqlx migrate info --source apps/api/migrations

# Regenerate the sqlx offline query cache (.sqlx/). Run this after
# editing any `sqlx::query!` / `query_as!` invocation so CI (and fresh
# clones) can build without a live database.
db-sqlx-prepare:
    DATABASE_URL="{{_db_url}}" cargo sqlx prepare --workspace -- --all-targets

# Interactive `redis-cli` shell on the running redis container.
redis-cli:
    {{COMPOSE}} exec redis redis-cli

# Open the MinIO web console. macOS-only `open`; on Linux use xdg-open.
minio-console:
    @echo "MinIO console: http://127.0.0.1:9001"
    @echo "  user:     ${MINIO_ROOT_USER:-minio_dev}"
    @echo "  password: ${MINIO_ROOT_PASSWORD:-minio_dev_secret}"

# Print the Mailpit inbox URL. macOS users can add `| xargs open`.
mailpit:
    @echo "Mailpit inbox: http://127.0.0.1:8025"

# --- Web (apps/web) ---------------------------------------------------------

# Install JS deps for all bun workspaces. Run after cloning.
web-install:
    bun install

# Vite dev server on :5173. Proxies /api → :3000, so run `just api-run` in
# parallel.
web-dev:
    bun run --filter './apps/web' dev

# Type-check + production build.
web-build:
    bun run --filter './apps/web' build

# Vitest run (no watch).
web-test:
    bun run --filter './apps/web' test

# ESLint + vue-tsc type check.
web-lint:
    bun run --filter './apps/web' lint
    bun run --filter './apps/web' typecheck

# Start both api and web in parallel. Requires GNU parallel or similar; kept
# here as documentation until a proper `just` recipe for process groups lands.
dev:
    @echo "Typical dev flow:"
    @echo "  1) just compose-up     # postgres + redis + minio in the background"
    @echo "  2) just api-run        # Axum on 127.0.0.1:3000 (terminal 1)"
    @echo "  3) just web-dev        # Vite on 127.0.0.1:5173 (terminal 2)"

# --- CI shortcut -------------------------------------------------------------

# Run the same checks the CI pipeline runs, in order. Keep this recipe in
# sync with .gitea/workflows/ci.yml (Phase 4).
ci: fmt lint test audit secrets-scan
    @echo "[ci] OK"
