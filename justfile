# Task runner for Le Concierge. Invoke with `just <recipe>`.
#
# Recipes that do not exist yet (web-*, compose-*) will be added in phases
# 2 and 3. Update this file in lockstep with the CI workflow (phase 4).
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
    @echo "[fmt] Web (prettier) will run once apps/web lands."

# Run every linter in the repo. Fails on any warning (CI parity).
lint:
    cargo clippy --all-targets --locked -- -D warnings
    @echo "[lint] bun lint (added in Phase 2)"

# --- Tests -------------------------------------------------------------------

# Run the full test suite (unit + integration + front). Slow.
test:
    cargo test --locked --workspace
    @echo "[test] bun test (added in Phase 2)"

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

# --- CI shortcut -------------------------------------------------------------

# Run the same checks the CI pipeline runs, in order. Keep this recipe in
# sync with .gitea/workflows/ci.yml (Phase 4).
ci: fmt lint test audit secrets-scan
    @echo "[ci] OK"
