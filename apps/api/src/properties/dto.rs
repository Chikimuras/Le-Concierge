//! HTTP request / response shapes for `/orgs/:slug/properties*`.
//!
//! Every incoming DTO uses `#[serde(deny_unknown_fields)]` so silent
//! drift in client payloads surfaces as 422, not as "we dropped your
//! field silently". Response DTOs are the only place `utoipa` schemas
//! appear — the domain types stay ignorant of the HTTP layer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    auth::{OrgId, Slug},
    properties::{
        domain::{self, CreatePropertyInput, Property, PropertyId, UpdatePropertyInput},
        error::PropertyError,
    },
};

// ---- Create request ------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePropertyRequest {
    #[schema(example = "chez-alex", min_length = 2, max_length = 64)]
    pub slug: String,
    #[schema(example = "Chez Alex", min_length = 1, max_length = 200)]
    pub name: String,
    /// IANA timezone (e.g. `Europe/Paris`). Defaults server-side when
    /// omitted. Bounded string, not validated against the tz DB here.
    #[serde(default)]
    #[schema(example = "Europe/Paris")]
    pub timezone: Option<String>,
    #[serde(default)]
    pub address_line1: Option<String>,
    #[serde(default)]
    pub address_line2: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub postal_code: Option<String>,
    /// ISO 3166-1 alpha-2 country code. Defaults to `FR`.
    #[serde(default)]
    #[schema(example = "FR", min_length = 2, max_length = 2)]
    pub country: Option<String>,
    #[serde(default)]
    #[schema(example = 2, minimum = 0, maximum = 50)]
    pub bedrooms: Option<i16>,
    #[serde(default)]
    #[schema(example = 4, minimum = 1, maximum = 100)]
    pub max_guests: Option<i16>,
    #[serde(default)]
    pub notes: Option<String>,
}

impl CreatePropertyRequest {
    /// Validate and convert to the service-layer input. Errors bubble
    /// through as `PropertyError::Validation(field)`.
    pub fn into_input(self) -> Result<CreatePropertyInput, PropertyError> {
        Ok(CreatePropertyInput {
            slug: Slug::parse(&self.slug)?,
            name: domain::parse_name(&self.name)?,
            timezone: domain::parse_timezone(self.timezone.as_deref().unwrap_or("Europe/Paris"))?,
            address_line1: domain::check_address(self.address_line1.as_deref(), "address_line1")?,
            address_line2: domain::check_address(self.address_line2.as_deref(), "address_line2")?,
            city: domain::check_city(self.city.as_deref())?,
            postal_code: domain::check_postal_code(self.postal_code.as_deref())?,
            country: domain::parse_country(self.country.as_deref())?,
            bedrooms: domain::check_bedrooms(self.bedrooms)?,
            max_guests: domain::check_max_guests(self.max_guests)?,
            notes: domain::check_notes(self.notes.as_deref())?,
        })
    }
}

// ---- Update request ------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePropertyRequest {
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub address_line1: Option<String>,
    #[serde(default)]
    pub address_line2: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub postal_code: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub bedrooms: Option<i16>,
    #[serde(default)]
    pub max_guests: Option<i16>,
    #[serde(default)]
    pub notes: Option<String>,
}

impl UpdatePropertyRequest {
    pub fn into_input(self) -> Result<UpdatePropertyInput, PropertyError> {
        let slug = self.slug.as_deref().map(Slug::parse).transpose()?;
        let name = self.name.as_deref().map(domain::parse_name).transpose()?;
        let timezone = self
            .timezone
            .as_deref()
            .map(domain::parse_timezone)
            .transpose()?;
        let bedrooms = domain::check_bedrooms(self.bedrooms)?;
        let max_guests = domain::check_max_guests(self.max_guests)?;
        let notes = self
            .notes
            .map(|v| domain::check_notes(Some(&v)))
            .transpose()?
            .flatten();
        let address_line1 = self
            .address_line1
            .map(|v| domain::check_address(Some(&v), "address_line1"))
            .transpose()?
            .flatten();
        let address_line2 = self
            .address_line2
            .map(|v| domain::check_address(Some(&v), "address_line2"))
            .transpose()?
            .flatten();
        let city = self
            .city
            .map(|v| domain::check_city(Some(&v)))
            .transpose()?
            .flatten();
        let postal_code = self
            .postal_code
            .map(|v| domain::check_postal_code(Some(&v)))
            .transpose()?
            .flatten();
        let country = self
            .country
            .map(|v| domain::parse_country(Some(&v)))
            .transpose()?
            .flatten();

        Ok(UpdatePropertyInput {
            slug,
            name,
            timezone,
            address_line1,
            address_line2,
            city,
            postal_code,
            country,
            bedrooms,
            max_guests,
            notes,
        })
    }
}

// ---- Response shape -------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PropertyResponse {
    pub id: PropertyId,
    pub org_id: OrgId,
    #[schema(example = "chez-alex")]
    pub slug: String,
    #[schema(example = "Chez Alex")]
    pub name: String,
    pub timezone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_line1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_line2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    pub country: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bedrooms: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_guests: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Property> for PropertyResponse {
    fn from(p: Property) -> Self {
        Self {
            id: p.id,
            org_id: p.org_id,
            slug: p.slug.as_str().to_owned(),
            name: p.name,
            timezone: p.timezone,
            address_line1: p.address_line1,
            address_line2: p.address_line2,
            city: p.city,
            postal_code: p.postal_code,
            country: p.country,
            bedrooms: p.bedrooms,
            max_guests: p.max_guests,
            notes: p.notes,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PropertyListResponse {
    pub properties: Vec<PropertyResponse>,
}
