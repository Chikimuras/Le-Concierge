//! `AuthenticatedUser` Axum extractor.
//!
//! Handlers that should only respond to logged-in callers declare an
//! argument of type [`AuthenticatedUser`] and get automatic 401 handling
//! for free: the extractor reads the `lc_sid` cookie, looks the session
//! up in Redis (which also refreshes its idle TTL), and rejects any
//! request whose cookie is missing, malformed, expired, or evicted.

use axum::{
    extract::{FromRequestParts, OptionalFromRequestParts},
    http::request::Parts,
};
use axum_extra::extract::CookieJar;

use crate::{
    AppError, AppState,
    auth::UserId,
    session::{
        cookie::SESSION_COOKIE_NAME,
        dto::{SessionData, SessionId},
        error::SessionError,
    },
};

/// A live session plus the resolved user identifier. Cheap to clone.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: UserId,
    pub session_id: SessionId,
    pub session: SessionData,
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let raw = jar
            .get(SESSION_COOKIE_NAME)
            .map(|c| c.value().to_owned())
            .ok_or(SessionError::NotFound)?;
        let id = SessionId::parse(&raw)?;
        let session = state.session.lookup(&id).await?;
        Ok(Self {
            user_id: session.user_id,
            session_id: id,
            session,
        })
    }
}

impl OptionalFromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Option<Self>, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let Some(raw) = jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_owned()) else {
            return Ok(None);
        };
        let id = match SessionId::parse(&raw) {
            Ok(id) => id,
            // A malformed cookie on an optional extractor is treated as
            // "not authenticated" rather than 401.
            Err(SessionError::Malformed) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        match state.session.lookup(&id).await {
            Ok(session) => Ok(Some(Self {
                user_id: session.user_id,
                session_id: id,
                session,
            })),
            Err(SessionError::NotFound | SessionError::Expired | SessionError::Malformed) => {
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }
}
