# ADR 0003 — Monorepo layout

- Status: Accepted
- Date: 2026-04-18
- Deciders: Alexandre Velia (solo)
- Tags: repo, tooling

## Context and Problem Statement

The project has a Rust backend, a Vue 3 frontend, shared TypeScript contracts
generated from OpenAPI, infrastructure manifests, and design documents.
Should these live in one repository or be split across multiple?

## Decision Drivers

- Solo developer — minimize context-switching overhead.
- Shared artifacts (OpenAPI schema → TypeScript types → Zod schemas) need
  atomic cross-language changes.
- Versioning: every deploy ships api + web + contracts together.
- CI/CD simplicity — one pipeline, one deploy script.

## Considered Options

1. **Monorepo** (one repo, multiple workspaces) — chosen.
2. Polyrepo (one repo per app/package, linked via versioned packages).
3. Monorepo with heavy tooling (Nx, Turborepo, Bazel).

## Decision Outcome

**Chosen: Option 1 — plain monorepo with native workspace features.**

- Cargo workspace (`[workspace] members = ["apps/*", "packages/*"]`).
- Bun workspaces (`"workspaces": ["apps/*", "packages/*"]`).
- `just` for cross-language task orchestration.
- **No** Nx/Turborepo/Bazel layer: they solve problems we do not have at
  single-dev scale. Re-evaluate if build times exceed ~2 min locally.

### Structure

```
/apps
  /api            # Rust backend (cargo workspace member)
  /web            # Vue 3 frontend (bun workspace member)
/packages
  /contracts      # TS types from OpenAPI + shared Zod schemas
  /ui             # Shared shadcn-vue components (optional)
/infra
  /docker         # Dockerfiles, compose files, reverse-proxy configs
  /grafana        # Provisioned dashboards + datasources
/docs
  /adr            # Architecture Decision Records (this directory)
  /rgpd           # GDPR register, DPIAs
```

### Positive Consequences

- A single commit can evolve an API handler, regenerate the OpenAPI schema,
  refresh the TS contract, and update the Vue caller — atomic and reviewable.
- One CI pipeline, one PR flow, one set of branch protection rules.
- Shared tooling (lefthook, gitleaks, editorconfig) configured once.

### Negative Consequences

- `git log` is noisier across domains; mitigated by `scope` in Conventional
  Commits (`feat(api): …`, `feat(web): …`).
- A full clone is bigger than any single app; acceptable.
- Open-sourcing a specific package later requires a `git filter-repo`
  extraction; acceptable future cost.

## Validation

- Cargo workspace resolves with zero members during Phase 0 (glob accepts
  empty expansion) and will absorb `apps/api` in Phase 1 without config
  changes.
- Bun workspaces likewise pick up `apps/web` in Phase 2.

## Related

- ADR 0001 — Stack technique
- `CLAUDE.md` §2.3 (structure repo)
