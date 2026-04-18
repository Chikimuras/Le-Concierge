# Task runner for Le Concierge. Invoke with `just <recipe>`.
#
# Recipes that do not exist yet (compose-*) will be added in phase 3.
# Update this file in lockstep with the CI workflow (phase 4).
#
# Ref: https://just.systems

set shell := ["bash", "-cu"]
set dotenv-load := false

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
api-dev:
    cargo watch -x 'run -p api'

# One-shot debug run of the api.
api-run:
    cargo run -p api

# Release build, single binary.
api-build:
    cargo build --release -p api

# Focused tests for the api crate.
api-test:
    cargo test -p api --locked

# Build the production Docker image.
api-docker-build:
    docker build -f apps/api/Dockerfile -t le-concierge/api:dev .

# Run the production Docker image, exposing port 3000 locally.
api-docker-run: api-docker-build
    docker run --rm -it --name le-concierge-api -p 3000:3000 \
        --env-file apps/api/.env.example \
        le-concierge/api:dev

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
    @echo "Run in two terminals:"
    @echo "  1) just api-run"
    @echo "  2) just web-dev"

# --- CI shortcut -------------------------------------------------------------

# Run the same checks the CI pipeline runs, in order. Keep this recipe in
# sync with .gitea/workflows/ci.yml (Phase 4).
ci: fmt lint test audit secrets-scan
    @echo "[ci] OK"
