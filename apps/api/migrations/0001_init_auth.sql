-- Initial auth schema.
--
-- Tables: organizations, users, organization_members, platform_admins,
--         audit_events.
--
-- Forward-only per CLAUDE.md §10 (no .down.sql). A future migration adds
-- new objects; never modify this file once merged.
--
-- References:
--   - CLAUDE.md §1 (actors), §3.1 (auth), §3.3 (audit log immutability)
--   - ADR 0002 — Security baseline
--   - ADR 0005 — Auth scheme

-- ---- Extensions ------------------------------------------------------------

CREATE EXTENSION IF NOT EXISTS pgcrypto;   -- gen_random_uuid()
CREATE EXTENSION IF NOT EXISTS citext;     -- case-insensitive text for email

-- ---- Role enum (per-organization) -----------------------------------------
-- `admin` is NOT in this enum: platform admins are a separate scope (see
-- `platform_admins` below) rather than a per-org role.

CREATE TYPE role AS ENUM ('owner', 'manager', 'cleaner', 'guest');

-- ---- Generic triggers -----------------------------------------------------

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS trigger AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ---- Organizations (tenants) ----------------------------------------------

CREATE TABLE organizations (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    slug        text        NOT NULL UNIQUE
                            CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$'),
    name        text        NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    timezone    text        NOT NULL DEFAULT 'Europe/Paris',
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER organizations_set_updated_at
    BEFORE UPDATE ON organizations
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---- Users ----------------------------------------------------------------
-- `password_hash` stores the full PHC-format Argon2id string (parameters are
-- embedded in the hash — no separate columns needed).

CREATE TABLE users (
    id                    uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    email                 citext      NOT NULL UNIQUE,
    password_hash         text        NOT NULL,
    email_verified_at     timestamptz,
    failed_login_attempts integer     NOT NULL DEFAULT 0
                                      CHECK (failed_login_attempts >= 0),
    locked_until          timestamptz,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER users_set_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---- Organization memberships ---------------------------------------------

CREATE TABLE organization_members (
    user_id     uuid        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    org_id      uuid        NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    role        role        NOT NULL,
    invited_by  uuid        REFERENCES users(id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, org_id)
);

CREATE INDEX idx_organization_members_user ON organization_members (user_id);
CREATE INDEX idx_organization_members_org  ON organization_members (org_id);

-- ---- Platform admins ------------------------------------------------------
-- Kept separate from `role` enum because platform admin is cross-tenant,
-- not a per-org role. Grants access to the `/admin` routes that ship later.

CREATE TABLE platform_admins (
    user_id     uuid        PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    granted_by  uuid        REFERENCES users(id) ON DELETE SET NULL,
    granted_at  timestamptz NOT NULL DEFAULT now()
);

-- ---- Audit events (append-only, hash-chained) -----------------------------
-- CLAUDE.md §3.3 / ADR 0002 §Data-and-logs.
--
-- Integrity model:
--   - UPDATE and DELETE are blocked by triggers (table is append-only).
--   - `hash = SHA-256(prev_hash || canonical_row_bytes)`. prev_hash is the
--     previous row's hash, so tampering with any row invalidates the chain.
--   - Hash computation happens in Rust under a Postgres advisory lock so
--     concurrent writers serialise (see `audit::record_event` in a later
--     phase). This keeps the chain correct across multiple processes.

CREATE TABLE audit_events (
    id            bigserial   PRIMARY KEY,
    occurred_at   timestamptz NOT NULL DEFAULT now(),
    actor_user_id uuid        REFERENCES users(id) ON DELETE SET NULL,
    org_id        uuid        REFERENCES organizations(id) ON DELETE SET NULL,
    kind          text        NOT NULL CHECK (length(kind) BETWEEN 1 AND 64),
    payload       jsonb       NOT NULL DEFAULT '{}'::jsonb,
    prev_hash     bytea,
    hash          bytea       NOT NULL CHECK (octet_length(hash) = 32)
);

CREATE INDEX idx_audit_events_occurred_at
    ON audit_events (occurred_at DESC);
CREATE INDEX idx_audit_events_org
    ON audit_events (org_id, occurred_at DESC)
    WHERE org_id IS NOT NULL;
CREATE INDEX idx_audit_events_actor
    ON audit_events (actor_user_id, occurred_at DESC)
    WHERE actor_user_id IS NOT NULL;

CREATE OR REPLACE FUNCTION audit_events_immutable()
RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'audit_events is append-only; UPDATE/DELETE forbidden';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_events_no_update
    BEFORE UPDATE ON audit_events
    FOR EACH STATEMENT EXECUTE FUNCTION audit_events_immutable();

CREATE TRIGGER audit_events_no_delete
    BEFORE DELETE ON audit_events
    FOR EACH STATEMENT EXECUTE FUNCTION audit_events_immutable();

-- TRUNCATE bypasses row-level triggers; block it explicitly too.
CREATE TRIGGER audit_events_no_truncate
    BEFORE TRUNCATE ON audit_events
    FOR EACH STATEMENT EXECUTE FUNCTION audit_events_immutable();
