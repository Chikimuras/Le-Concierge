//! SQLx persistence for properties. Every query is scoped by
//! `org_id` — the `Membership` extractor hands the handler an
//! `OrgId` which flows through to every call here. See ADR 0008.

use sqlx::PgPool;

use crate::{
    auth::{OrgId, Slug},
    properties::{
        domain::{CreatePropertyInput, Property, PropertyId, UpdatePropertyInput},
        error::PropertyError,
    },
};

#[derive(Clone)]
pub struct PropertyRepo {
    pool: PgPool,
}

impl PropertyRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List active properties for `org_id`, most-recently-updated first.
    pub async fn list(&self, org_id: OrgId) -> Result<Vec<Property>, PropertyError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, org_id, slug, name, timezone,
                   address_line1, address_line2, city, postal_code, country,
                   bedrooms, max_guests, notes,
                   created_at, updated_at
              FROM properties
             WHERE org_id = $1
               AND deleted_at IS NULL
             ORDER BY updated_at DESC
            "#,
            org_id.into_inner(),
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| {
                Ok(Property {
                    id: PropertyId::from(r.id),
                    org_id: OrgId::from(r.org_id),
                    slug: reparse_slug(&r.slug)?,
                    name: r.name,
                    timezone: r.timezone,
                    address_line1: r.address_line1,
                    address_line2: r.address_line2,
                    city: r.city,
                    postal_code: r.postal_code,
                    country: r.country,
                    bedrooms: r.bedrooms,
                    max_guests: r.max_guests,
                    notes: r.notes,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                })
            })
            .collect()
    }

    /// Fetch one active property by id, scoped by `org_id`. Rows in a
    /// different org or already soft-deleted come back as `None`; the
    /// caller renders 404.
    pub async fn find(
        &self,
        org_id: OrgId,
        id: PropertyId,
    ) -> Result<Option<Property>, PropertyError> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, slug, name, timezone,
                   address_line1, address_line2, city, postal_code, country,
                   bedrooms, max_guests, notes,
                   created_at, updated_at
              FROM properties
             WHERE id = $1
               AND org_id = $2
               AND deleted_at IS NULL
            "#,
            id.into_inner(),
            org_id.into_inner(),
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(r) = row else { return Ok(None) };
        Ok(Some(Property {
            id: PropertyId::from(r.id),
            org_id: OrgId::from(r.org_id),
            slug: reparse_slug(&r.slug)?,
            name: r.name,
            timezone: r.timezone,
            address_line1: r.address_line1,
            address_line2: r.address_line2,
            city: r.city,
            postal_code: r.postal_code,
            country: r.country,
            bedrooms: r.bedrooms,
            max_guests: r.max_guests,
            notes: r.notes,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    /// Insert a new property. A collision on `(org_id, slug)` surfaces
    /// as [`PropertyError::SlugAlreadyTaken`]; anything else bubbles
    /// up as a repository error.
    pub async fn create(
        &self,
        org_id: OrgId,
        input: &CreatePropertyInput,
    ) -> Result<Property, PropertyError> {
        let r = sqlx::query!(
            r#"
            INSERT INTO properties
                (org_id, slug, name, timezone, address_line1, address_line2,
                 city, postal_code, country, bedrooms, max_guests, notes)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, COALESCE($9, 'FR'), $10, $11, $12)
            RETURNING id, org_id, slug, name, timezone,
                      address_line1, address_line2, city, postal_code, country,
                      bedrooms, max_guests, notes, created_at, updated_at
            "#,
            org_id.into_inner(),
            input.slug.as_str(),
            input.name,
            input.timezone,
            input.address_line1,
            input.address_line2,
            input.city,
            input.postal_code,
            input.country,
            input.bedrooms,
            input.max_guests,
            input.notes,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(translate_unique_violation)?;

        Ok(Property {
            id: PropertyId::from(r.id),
            org_id: OrgId::from(r.org_id),
            slug: reparse_slug(&r.slug)?,
            name: r.name,
            timezone: r.timezone,
            address_line1: r.address_line1,
            address_line2: r.address_line2,
            city: r.city,
            postal_code: r.postal_code,
            country: r.country,
            bedrooms: r.bedrooms,
            max_guests: r.max_guests,
            notes: r.notes,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }

    /// PATCH an active property. Fields set to `None` are left
    /// untouched via `COALESCE($n, column)` in SQL. Returns
    /// `NotFound` when no active row matches `(id, org_id)`, and
    /// `SlugAlreadyTaken` when a rename collides.
    pub async fn update(
        &self,
        org_id: OrgId,
        id: PropertyId,
        patch: &UpdatePropertyInput,
    ) -> Result<Property, PropertyError> {
        let r = sqlx::query!(
            r#"
            UPDATE properties
               SET slug          = COALESCE($3, slug),
                   name          = COALESCE($4, name),
                   timezone      = COALESCE($5, timezone),
                   address_line1 = COALESCE($6, address_line1),
                   address_line2 = COALESCE($7, address_line2),
                   city          = COALESCE($8, city),
                   postal_code   = COALESCE($9, postal_code),
                   country       = COALESCE($10, country),
                   bedrooms      = COALESCE($11, bedrooms),
                   max_guests    = COALESCE($12, max_guests),
                   notes         = COALESCE($13, notes)
             WHERE id = $1
               AND org_id = $2
               AND deleted_at IS NULL
            RETURNING id, org_id, slug, name, timezone,
                      address_line1, address_line2, city, postal_code, country,
                      bedrooms, max_guests, notes, created_at, updated_at
            "#,
            id.into_inner(),
            org_id.into_inner(),
            patch.slug.as_ref().map(Slug::as_str),
            patch.name,
            patch.timezone,
            patch.address_line1,
            patch.address_line2,
            patch.city,
            patch.postal_code,
            patch.country,
            patch.bedrooms,
            patch.max_guests,
            patch.notes,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(translate_unique_violation)?
        .ok_or(PropertyError::NotFound)?;

        Ok(Property {
            id: PropertyId::from(r.id),
            org_id: OrgId::from(r.org_id),
            slug: reparse_slug(&r.slug)?,
            name: r.name,
            timezone: r.timezone,
            address_line1: r.address_line1,
            address_line2: r.address_line2,
            city: r.city,
            postal_code: r.postal_code,
            country: r.country,
            bedrooms: r.bedrooms,
            max_guests: r.max_guests,
            notes: r.notes,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }

    /// Soft-delete. Row belongs to another org or was already deleted
    /// → `NotFound`, never a silent no-op.
    pub async fn soft_delete(&self, org_id: OrgId, id: PropertyId) -> Result<(), PropertyError> {
        let rows = sqlx::query!(
            r#"
            UPDATE properties
               SET deleted_at = now()
             WHERE id = $1
               AND org_id = $2
               AND deleted_at IS NULL
            "#,
            id.into_inner(),
            org_id.into_inner(),
        )
        .execute(&self.pool)
        .await?
        .rows_affected();
        if rows == 0 {
            Err(PropertyError::NotFound)
        } else {
            Ok(())
        }
    }
}

// Stored slugs were validated at write time; re-parsing here is
// defensive. If it ever fails, the DB has drifted from the invariant
// and the mapping surfaces a Repository error rather than silently
// masking corruption.
fn reparse_slug(raw: &str) -> Result<Slug, PropertyError> {
    Slug::parse(raw).map_err(|_| {
        tracing::error!(slug = %raw, "stored slug failed re-parse");
        PropertyError::Internal(anyhow::anyhow!("stored slug failed re-parse"))
    })
}

fn translate_unique_violation(err: sqlx::Error) -> PropertyError {
    if let sqlx::Error::Database(db_err) = &err
        && db_err.code().as_deref() == Some("23505")
        && db_err.constraint().is_some_and(|c| c.contains("slug"))
    {
        return PropertyError::SlugAlreadyTaken;
    }
    PropertyError::Repository(err)
}
