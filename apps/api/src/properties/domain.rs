//! Property domain types.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{OrgId, Slug},
    properties::error::PropertyError,
};

// ---- PropertyId ------------------------------------------------------------

/// Identifier of a property. UUIDv4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[serde(transparent)]
#[sqlx(transparent)]
#[schema(value_type = String, format = Uuid, example = "00000000-0000-4000-8000-000000000000")]
pub struct PropertyId(pub Uuid);

impl PropertyId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub fn into_inner(self) -> Uuid {
        self.0
    }
}

impl Default for PropertyId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PropertyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<Uuid> for PropertyId {
    fn from(u: Uuid) -> Self {
        Self(u)
    }
}

impl From<PropertyId> for Uuid {
    fn from(n: PropertyId) -> Self {
        n.0
    }
}

// ---- Name / attribute validation ------------------------------------------

const NAME_MIN: usize = 1;
const NAME_MAX: usize = 200;
const NOTES_MAX: usize = 2000;
const TIMEZONE_MIN: usize = 1;
const TIMEZONE_MAX: usize = 64;
const ADDRESS_MAX: usize = 200;
const CITY_MAX: usize = 100;
const POSTAL_CODE_MAX: usize = 20;
const COUNTRY_LEN: usize = 2;
const BEDROOMS_MIN: i16 = 0;
const BEDROOMS_MAX: i16 = 50;
const GUESTS_MIN: i16 = 1;
const GUESTS_MAX: i16 = 100;

/// Normalise and validate a property name. Trims surrounding whitespace.
pub fn parse_name(raw: &str) -> Result<String, PropertyError> {
    let trimmed = raw.trim();
    if !(NAME_MIN..=NAME_MAX).contains(&trimmed.len()) {
        return Err(PropertyError::Validation("name"));
    }
    Ok(trimmed.to_owned())
}

/// Trim and bound-check the timezone string. We do not validate
/// against the IANA tz database here — the server uses `chrono-tz` if
/// it ever needs to compute local times; an unknown timezone surfaces
/// as a booking-level error later.
pub fn parse_timezone(raw: &str) -> Result<String, PropertyError> {
    let trimmed = raw.trim();
    if !(TIMEZONE_MIN..=TIMEZONE_MAX).contains(&trimmed.len()) {
        return Err(PropertyError::Validation("timezone"));
    }
    Ok(trimmed.to_owned())
}

/// Parse an optional text field with a max length, collapsing empty
/// strings to `None` so the caller gets a clean null downstream.
pub fn parse_optional_text(
    raw: Option<&str>,
    max: usize,
    field: &'static str,
) -> Result<Option<String>, PropertyError> {
    let Some(value) = raw else { return Ok(None) };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > max {
        return Err(PropertyError::Validation(field));
    }
    Ok(Some(trimmed.to_owned()))
}

pub fn parse_country(raw: Option<&str>) -> Result<Option<String>, PropertyError> {
    let Some(value) = raw else { return Ok(None) };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() != COUNTRY_LEN || !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(PropertyError::Validation("country"));
    }
    Ok(Some(trimmed.to_ascii_uppercase()))
}

pub fn check_bedrooms(v: Option<i16>) -> Result<Option<i16>, PropertyError> {
    match v {
        None => Ok(None),
        Some(n) if (BEDROOMS_MIN..=BEDROOMS_MAX).contains(&n) => Ok(Some(n)),
        _ => Err(PropertyError::Validation("bedrooms")),
    }
}

pub fn check_max_guests(v: Option<i16>) -> Result<Option<i16>, PropertyError> {
    match v {
        None => Ok(None),
        Some(n) if (GUESTS_MIN..=GUESTS_MAX).contains(&n) => Ok(Some(n)),
        _ => Err(PropertyError::Validation("max_guests")),
    }
}

pub fn check_notes(raw: Option<&str>) -> Result<Option<String>, PropertyError> {
    parse_optional_text(raw, NOTES_MAX, "notes")
}

pub fn check_address(
    raw: Option<&str>,
    field: &'static str,
) -> Result<Option<String>, PropertyError> {
    parse_optional_text(raw, ADDRESS_MAX, field)
}

pub fn check_city(raw: Option<&str>) -> Result<Option<String>, PropertyError> {
    parse_optional_text(raw, CITY_MAX, "city")
}

pub fn check_postal_code(raw: Option<&str>) -> Result<Option<String>, PropertyError> {
    parse_optional_text(raw, POSTAL_CODE_MAX, "postal_code")
}

// ---- Inputs ---------------------------------------------------------------

/// Input to [`crate::properties::service::PropertyService::create`].
#[derive(Debug, Clone)]
pub struct CreatePropertyInput {
    pub slug: Slug,
    pub name: String,
    pub timezone: String,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub bedrooms: Option<i16>,
    pub max_guests: Option<i16>,
    pub notes: Option<String>,
}

/// Input to [`crate::properties::service::PropertyService::update`].
/// `None` on any field means "leave untouched"; a non-null value
/// overwrites. Clearing a nullable field is not supported in v1 —
/// send an empty string through the DTO's `parse_optional_text` path
/// to collapse to `None` and keep the current value instead.
#[derive(Debug, Clone, Default)]
pub struct UpdatePropertyInput {
    pub slug: Option<Slug>,
    pub name: Option<String>,
    pub timezone: Option<String>,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub bedrooms: Option<i16>,
    pub max_guests: Option<i16>,
    pub notes: Option<String>,
}

// ---- Output ---------------------------------------------------------------

/// Full property row as returned by the repo + service to the HTTP
/// layer. `deleted_at` is never emitted: reads filter it out.
#[derive(Debug, Clone)]
pub struct Property {
    pub id: PropertyId,
    pub org_id: OrgId,
    pub slug: Slug,
    pub name: String,
    pub timezone: String,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub postal_code: Option<String>,
    pub country: String,
    pub bedrooms: Option<i16>,
    pub max_guests: Option<i16>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn name_requires_non_empty_trimmed() {
        assert!(parse_name("  ").is_err());
        assert!(parse_name("").is_err());
        assert_eq!(parse_name("  Chez Alex  ").unwrap(), "Chez Alex");
    }

    #[test]
    fn name_rejects_excess_length() {
        let long = "a".repeat(NAME_MAX + 1);
        assert!(parse_name(&long).is_err());
        let edge = "a".repeat(NAME_MAX);
        assert_eq!(parse_name(&edge).unwrap().len(), NAME_MAX);
    }

    #[test]
    fn country_normalises_to_uppercase_iso_2() {
        assert_eq!(parse_country(Some("fr")).unwrap().as_deref(), Some("FR"));
        assert_eq!(parse_country(Some("FR")).unwrap().as_deref(), Some("FR"));
        assert!(parse_country(Some("FRA")).is_err());
        assert!(parse_country(Some("F1")).is_err());
        assert_eq!(parse_country(None).unwrap(), None);
        assert_eq!(parse_country(Some("")).unwrap(), None);
    }

    #[test]
    fn bedrooms_in_range() {
        assert_eq!(check_bedrooms(Some(0)).unwrap(), Some(0));
        assert_eq!(check_bedrooms(Some(50)).unwrap(), Some(50));
        assert!(check_bedrooms(Some(-1)).is_err());
        assert!(check_bedrooms(Some(51)).is_err());
    }

    #[test]
    fn optional_text_trims_empties_to_none() {
        assert_eq!(parse_optional_text(Some("  "), 10, "x").unwrap(), None,);
        assert_eq!(
            parse_optional_text(Some("hello"), 10, "x")
                .unwrap()
                .as_deref(),
            Some("hello"),
        );
    }
}
