-- Team invites (Phase 5b). A row in `organization_invites` represents
-- a pending, accepted, or cancelled invitation for someone to join an
-- org with a specific role. See ADR 0009 for the lifecycle.
--
-- Forward-only per CLAUDE.md §10.

CREATE TABLE organization_invites (
    id            uuid         PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id        uuid         NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    email         citext       NOT NULL CHECK (length(email) BETWEEN 3 AND 254),
    role          role         NOT NULL,
    invited_by    uuid         NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    -- HMAC-SHA-256(pepper, token) hex-encoded — 64 chars. Deterministic
    -- per token, UNIQUE so the accept flow can look up the row in O(1)
    -- via the index below. ADR 0009 explains why we do NOT use
    -- Argon2id here (token carries 256 bits of entropy; linear verify
    -- cost would be wasted security budget).
    token_hash    text         NOT NULL
                               CHECK (length(token_hash) = 64),
    expires_at    timestamptz  NOT NULL,
    accepted_at   timestamptz,
    accepted_by   uuid         REFERENCES users(id) ON DELETE SET NULL,
    cancelled_at  timestamptz,
    created_at    timestamptz  NOT NULL DEFAULT now()
);

-- Only one pending invite per (org, email). Resending replaces the
-- previous row (delete+insert, not update) so the token rotates.
CREATE UNIQUE INDEX idx_invites_org_email_pending
    ON organization_invites (org_id, email)
    WHERE accepted_at IS NULL AND cancelled_at IS NULL;

-- Hot path for `/auth/invites/{preview,accept,signup}`: the server
-- HMAC-hashes the submitted token and looks the row up in O(1). The
-- partial UNIQUE prevents the (cryptographically impossible but
-- cheap-to-enforce) collision on two active hashes.
CREATE UNIQUE INDEX idx_invites_active_token
    ON organization_invites (token_hash)
    WHERE accepted_at IS NULL AND cancelled_at IS NULL;

-- Also let the manager list pending invites cheaply.
CREATE INDEX idx_invites_org_pending
    ON organization_invites (org_id)
    WHERE accepted_at IS NULL AND cancelled_at IS NULL;
