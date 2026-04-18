-- Properties (biens): the first tenant-scoped domain.
--
-- Forward-only per CLAUDE.md §10. Soft-deleted rows stay in place so
-- historical bookings (later phase) can still resolve their property,
-- but every read filters `deleted_at IS NULL`.
--
-- References:
--   - CLAUDE.md §1 (actors), §3.4 (validation)
--   - ADR 0005 — Auth scheme (multi-tenancy via application-layer)
--   - ADR 0008 — Multi-tenant isolation

CREATE TABLE properties (
    id             uuid         PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id         uuid         NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    slug           text         NOT NULL
                                CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$'),
    name           text         NOT NULL
                                CHECK (length(name) BETWEEN 1 AND 200),
    timezone       text         NOT NULL DEFAULT 'Europe/Paris',
    address_line1  text         CHECK (address_line1 IS NULL OR length(address_line1) <= 200),
    address_line2  text         CHECK (address_line2 IS NULL OR length(address_line2) <= 200),
    city           text         CHECK (city IS NULL OR length(city) <= 100),
    postal_code    text         CHECK (postal_code IS NULL OR length(postal_code) <= 20),
    country        text         NOT NULL DEFAULT 'FR'
                                CHECK (length(country) = 2),
    bedrooms       smallint     CHECK (bedrooms IS NULL OR bedrooms BETWEEN 0 AND 50),
    max_guests     smallint     CHECK (max_guests IS NULL OR max_guests BETWEEN 1 AND 100),
    notes          text         CHECK (notes IS NULL OR length(notes) <= 2000),
    created_at     timestamptz  NOT NULL DEFAULT now(),
    updated_at     timestamptz  NOT NULL DEFAULT now(),
    deleted_at     timestamptz
);

-- Slug uniqueness is scoped to the owning organisation and only
-- enforced for active rows. Two orgs can both have a "chez-alex" slug,
-- and a soft-deleted "chez-alex" does not block re-creation.
CREATE UNIQUE INDEX idx_properties_org_slug_active
    ON properties (org_id, slug)
    WHERE deleted_at IS NULL;

-- Hot path for `/orgs/:slug/properties` listings.
CREATE INDEX idx_properties_org_active
    ON properties (org_id)
    WHERE deleted_at IS NULL;

CREATE TRIGGER properties_set_updated_at
    BEFORE UPDATE ON properties
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
