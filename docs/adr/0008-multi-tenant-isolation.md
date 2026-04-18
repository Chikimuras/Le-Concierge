# ADR 0008 — Multi-tenant isolation

- Status: Accepted
- Date: 2026-04-18
- Deciders: Alexandre Velia (solo)
- Tags: security, architecture, tenancy

## Context and Problem Statement

Phase 5a ships the first **tenant-scoped** domain: properties (biens).
Every downstream feature (bookings, guests, payments, calendars,
cleaners) will live under the same organization, so the boundary we
put in place here is copy-pasted into everything that follows. The
question: how do we guarantee that data belonging to organization A
never leaks to a caller authenticated only for organization B?

ADR 0005 committed us to **application-layer isolation** for v1 —
Postgres RLS deferred until the surface area justifies the operational
cost. This ADR translates that commitment into three concrete rules:
URL shape, enforcement mechanism, and test obligations.

## Decision Drivers

- OWASP ASVS v4.0.3 §4 (access control), §13 (API-specific).
- CLAUDE.md §3.1 (security non-negotiable), §7.1 (repo organised by
  domain, `OrgId` newtype), §10 (no shortcuts).
- Solo developer: the rule must be impossible to forget — either a
  compile error or a loud test failure, never a silent miss.
- Minimum cognitive overhead: one pattern, applied uniformly.

## Decisions

### URL shape: `/orgs/:slug/…`

All tenant-scoped endpoints sit under the path prefix `/orgs/{slug}/`.
The slug is the human-readable tenant identifier already validated by
`Slug::parse` (migration 0001). Examples:

- `GET /orgs/acme/properties`
- `POST /orgs/acme/properties`
- `PATCH /orgs/acme/properties/{id}`
- (later) `GET /orgs/acme/bookings`, `/orgs/acme/calendars`, …

Rejected alternatives:

- `?org=<slug>` query parameter: easy to omit, easy to inject from
  another tab's URL, harder to bookmark. No gains.
- `X-Org-Slug` header: same issues as query param, plus mobile clients
  / curl users have to remember it.
- `/orgs/:id/` with UUID: hostile to humans, and the UUID is less
  user-facing information than the slug (which the user chose).
- Implicit-from-session: a user can belong to multiple organizations
  (managers running several properties, cross-tenant support). The
  slug in the path is the only unambiguous signal.

The anonymous surface (`/auth/*`, `/healthz`, `/docs`, `/openapi.json`)
stays at the root — there is no org yet at login time.

### Enforcement: `Membership` extractor

A single Axum extractor is the chokepoint for every tenant-scoped
route. It runs after `AuthenticatedUser` and produces an `Access`
value carrying `{ org_id, role, user_id }`.

Responsibilities:

1. Pull `slug` from the path segment (`axum::extract::Path`).
2. Resolve the org through `AuthRepo::find_org_by_slug`. Return
   **404 Not Found** if no such org exists.
3. Check that the session's user has a membership in that org with at
   least the handler-required role. Return **404 Not Found** on
   mismatch — _never 403_. A caller without access to an org must not
   learn whether it exists.
4. Hand the resolved `OrgId` down to the handler. The handler must
   pass it through to every SQL query; the `OrgId` newtype makes this
   a compile error to skip.

Rejected alternatives:

- Per-handler `WHERE user_has_membership(...)`: easy to forget on a
  new route. Centralising in one extractor makes "did you wire it up?"
  a review checklist item.
- Postgres RLS via `SET LOCAL app.current_org`: real defense in depth,
  but adds transaction scoping, policy bugs on every new table, and
  doubles the mental model. Deferred — a future ADR triggers it.

### The 404-over-403 rule

When a caller authenticates successfully but has no membership in the
target org, the response is `404 Not Found`, never `403 Forbidden` or
`401 Unauthorized`. Same reasoning as ASVS 2.1.1 for user enumeration:
the client cannot distinguish "this org does not exist" from "this
org exists and you have no access". This is consistent across:

- Unknown slug.
- Known slug, no membership.
- Known slug, membership with insufficient role.

The audit log still records attempts — `access.denied` with the
attempted slug + masked IP, flagged so security-review can spot
enumeration probing.

### `org_id` in every query

Every `sqlx::query!` that touches a tenant table must bind the
resolved `OrgId` and include it in the `WHERE` clause. The `OrgId`
newtype prevents accidentally passing a `UserId` through. A code
search for `FROM properties` (or any other tenant table) must return
zero queries without an `org_id = $1` predicate.

Deletes and updates follow the same rule: `WHERE id = $1 AND org_id =
$2`. Even if `id` were guessed, the row belongs to another tenant
and the `UPDATE` affects zero rows — the handler treats
`rows_affected == 0` as `AppError::NotFound`.

### Soft delete

Tenant tables default to a `deleted_at timestamptz` column plus a
partial index on active rows:

```sql
CREATE INDEX idx_<table>_org_active
    ON <table> (org_id) WHERE deleted_at IS NULL;
```

Every read filters `deleted_at IS NULL` by default. Mutations on a
soft-deleted row return 404. Historical references (bookings pointing
to a closed property) still resolve through direct joins without
violating the filter — the join target is fetched separately.

A later ADR can add a `deleted_rows_retention` job; for now rows stay
in place.

### Tests we must ship

Every PR that introduces a new tenant-scoped endpoint must include an
**isolation test** in `apps/api/tests/tenant_isolation.rs`. Shape:

1. User A signs up (creates org A).
2. User A creates the resource R_A in org A.
3. User B signs up (creates org B).
4. User B tries `GET /orgs/acme/<resource>/R_A.id` — expect 404.
5. User B tries `PATCH /orgs/acme/<resource>/R_A.id` — expect 404.
6. User B tries `DELETE /orgs/acme/<resource>/R_A.id` — expect 404.
7. Repeat with user B trying to target org A's slug directly
   (`GET /orgs/<A.slug>/<resource>`) — expect 404.

This is not optional. A PR lacking the isolation test for its new
tenant endpoint is rejected.

### Frontend mirror

The frontend surfaces the same shape: routes are
`/orgs/:slug/<feature>`; a `useActiveOrg` composable reads the slug
from the route and matches it against `session.memberships` to resolve
the active role. The router guard extends `requiresMembership` as new
meta that verifies the user has a membership with the required role
*before* navigating — a defense-in-depth against deep-link
enumeration (the server 404 is still the authority).

## Validation

- Every tenant-scoped PR ships an isolation test (see above).
- `cargo clippy -D warnings` catches misuse of the `OrgId` / `UserId`
  newtypes.
- The `Membership` extractor is unit-tested with every combination
  (unknown slug, no membership, wrong role, happy path).
- Periodic grep in CI: `rg '^\\s*FROM [a-z_]+' apps/api/src/**` must
  show every tenant table accompanied by an `org_id = $` nearby. If
  this becomes noisy we promote it to a cargo-deny custom lint.

## Related

- ADR 0002 — Security baseline
- ADR 0005 — Auth scheme (multi-tenancy isolation line item)
- CLAUDE.md §1 (actors), §3.1 (sécurité), §7.1 (repo organisé par
  domaine)
