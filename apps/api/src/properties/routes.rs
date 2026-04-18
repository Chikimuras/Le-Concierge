//! HTTP handlers for `/orgs/:slug/properties*`.
//!
//! Every handler declares a [`Membership`] extractor — that's the
//! only place the slug-to-org resolution happens. Handlers then pass
//! `access.org_id` down to the service. Per ADR 0008:
//!
//! - Unknown slug / missing membership / insufficient role → 404.
//! - Properties belonging to another org → 404 (the query's `org_id`
//!   predicate skips them, the service maps the empty result to
//!   [`PropertyError::NotFound`]).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    AppError, AppState,
    auth::Role,
    properties::{
        domain::PropertyId,
        dto::{
            CreatePropertyRequest, PropertyListResponse, PropertyResponse, UpdatePropertyRequest,
        },
    },
    tenancy::Membership,
};

#[utoipa::path(
    get,
    path = "/orgs/{slug}/properties",
    tag = "properties",
    params(("slug" = String, Path, description = "Organisation slug")),
    responses(
        (status = 200, description = "List of active properties", body = PropertyListResponse),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Org not found or no membership"),
    )
)]
pub async fn list_properties(
    State(state): State<AppState>,
    access: Membership,
) -> Result<Json<PropertyListResponse>, AppError> {
    access.ensure_role(Role::Cleaner)?;
    let rows = state.properties.list(access.org_id).await?;
    Ok(Json(PropertyListResponse {
        properties: rows.into_iter().map(PropertyResponse::from).collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/orgs/{slug}/properties",
    tag = "properties",
    params(("slug" = String, Path, description = "Organisation slug")),
    request_body = CreatePropertyRequest,
    responses(
        (status = 201, description = "Property created", body = PropertyResponse),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "CSRF token missing / invalid"),
        (status = 404, description = "Org not found or insufficient role"),
        (status = 409, description = "Slug already taken in this org"),
        (status = 422, description = "Validation failed"),
    )
)]
pub async fn create_property(
    State(state): State<AppState>,
    access: Membership,
    Json(body): Json<CreatePropertyRequest>,
) -> Result<(StatusCode, Json<PropertyResponse>), AppError> {
    access.ensure_role(Role::Manager)?;
    let input = body.into_input()?;
    let property = state
        .properties
        .create(access.user_id, access.org_id, input)
        .await?;
    Ok((StatusCode::CREATED, Json(PropertyResponse::from(property))))
}

#[utoipa::path(
    get,
    path = "/orgs/{slug}/properties/{id}",
    tag = "properties",
    params(
        ("slug" = String, Path, description = "Organisation slug"),
        ("id" = Uuid, Path, description = "Property id"),
    ),
    responses(
        (status = 200, description = "Property", body = PropertyResponse),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Org or property not found"),
    )
)]
pub async fn get_property(
    State(state): State<AppState>,
    access: Membership,
    Path((_slug, id)): Path<(String, Uuid)>,
) -> Result<Json<PropertyResponse>, AppError> {
    access.ensure_role(Role::Cleaner)?;
    let property = state
        .properties
        .get(access.org_id, PropertyId::from(id))
        .await?;
    Ok(Json(PropertyResponse::from(property)))
}

#[utoipa::path(
    patch,
    path = "/orgs/{slug}/properties/{id}",
    tag = "properties",
    params(
        ("slug" = String, Path, description = "Organisation slug"),
        ("id" = Uuid, Path, description = "Property id"),
    ),
    request_body = UpdatePropertyRequest,
    responses(
        (status = 200, description = "Property updated", body = PropertyResponse),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "CSRF token missing / invalid"),
        (status = 404, description = "Org or property not found, or insufficient role"),
        (status = 409, description = "New slug collides with another property"),
        (status = 422, description = "Validation failed"),
    )
)]
pub async fn update_property(
    State(state): State<AppState>,
    access: Membership,
    Path((_slug, id)): Path<(String, Uuid)>,
    Json(body): Json<UpdatePropertyRequest>,
) -> Result<Json<PropertyResponse>, AppError> {
    access.ensure_role(Role::Manager)?;
    let patch = body.into_input()?;
    let property = state
        .properties
        .update(access.user_id, access.org_id, PropertyId::from(id), patch)
        .await?;
    Ok(Json(PropertyResponse::from(property)))
}

#[utoipa::path(
    delete,
    path = "/orgs/{slug}/properties/{id}",
    tag = "properties",
    params(
        ("slug" = String, Path, description = "Organisation slug"),
        ("id" = Uuid, Path, description = "Property id"),
    ),
    responses(
        (status = 204, description = "Property soft-deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "CSRF token missing / invalid"),
        (status = 404, description = "Org or property not found, or insufficient role"),
    )
)]
pub async fn delete_property(
    State(state): State<AppState>,
    access: Membership,
    Path((_slug, id)): Path<(String, Uuid)>,
) -> Result<StatusCode, AppError> {
    access.ensure_role(Role::Manager)?;
    state
        .properties
        .delete(access.user_id, access.org_id, PropertyId::from(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[must_use]
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_properties, create_property))
        .routes(routes!(get_property, update_property, delete_property))
}
