//! Cross-cutting tenant-access helpers.
//!
//! Every tenant-scoped HTTP route sits behind a [`Membership`] extractor
//! that resolves the org slug in the path to an `OrgId` plus the
//! caller's role — then the handler passes the `OrgId` through to every
//! SQL query. ADR 0008 is the single source of truth for the model.
//!
//! The two rules the extractor enforces:
//!
//! 1. Unknown slug **or** missing membership **or** insufficient role
//!    all return the same `404 Not Found` — the client cannot
//!    enumerate orgs they don't belong to.
//! 2. `AuthenticatedUser` must have resolved successfully first
//!    (anonymous callers get 401 on auth, not a tenant 404).

use std::collections::HashMap;

use axum::{
    extract::{FromRequestParts, Path},
    http::request::Parts,
};

use crate::{
    AppError, AppState,
    auth::{OrgId, Role, UserId, repo::ActiveMembership},
    session::AuthenticatedUser,
};

/// A caller's resolved access to a tenant-scoped resource. Produced by
/// [`Membership::from_request_parts`] and passed into handlers through
/// the Axum extractor mechanism.
#[derive(Debug, Clone, Copy)]
pub struct Membership {
    pub user_id: UserId,
    pub org_id: OrgId,
    pub role: Role,
}

impl Membership {
    /// Reject the handler with 404 if the caller's role is strictly
    /// below `required`. Ordering: `Owner > Manager > Cleaner > Guest`.
    /// 404 over 403 per ADR 0008.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] when the role is below `required`.
    pub fn ensure_role(&self, required: Role) -> Result<(), AppError> {
        if role_level(self.role) >= role_level(required) {
            Ok(())
        } else {
            Err(AppError::NotFound)
        }
    }
}

impl FromRequestParts<AppState> for Membership {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Step 1: authentication. Any failure here is a 401, not a 404.
        let auth = AuthenticatedUser::from_request_parts(parts, state).await?;

        // Step 2: slug from the path. Axum lets us extract `Path` twice
        // with different shapes, so handlers keep their typed `Path<..>`
        // for the full path while we pull just `slug` here.
        let Path(params) = Path::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::NotFound)?;
        let Some(slug) = params.get("slug") else {
            return Err(AppError::NotFound);
        };

        // Step 3: resolve org + membership in one round-trip.
        let membership = state
            .auth
            .resolve_membership(auth.user_id, slug)
            .await?
            .ok_or(AppError::NotFound)?;

        Ok(Self::from_resolved(auth.user_id, membership))
    }
}

impl Membership {
    fn from_resolved(user_id: UserId, m: ActiveMembership) -> Self {
        Self {
            user_id,
            org_id: m.org_id,
            role: m.role,
        }
    }
}

fn role_level(role: Role) -> u8 {
    match role {
        Role::Owner => 3,
        Role::Manager => 2,
        Role::Cleaner => 1,
        Role::Guest => 0,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn membership(role: Role) -> Membership {
        Membership {
            user_id: UserId::new(),
            org_id: OrgId::new(),
            role,
        }
    }

    #[test]
    fn ensure_role_accepts_exact_and_higher() {
        assert!(membership(Role::Owner).ensure_role(Role::Owner).is_ok());
        assert!(membership(Role::Owner).ensure_role(Role::Manager).is_ok());
        assert!(membership(Role::Manager).ensure_role(Role::Manager).is_ok());
        assert!(membership(Role::Manager).ensure_role(Role::Cleaner).is_ok());
        assert!(membership(Role::Cleaner).ensure_role(Role::Cleaner).is_ok());
        assert!(membership(Role::Cleaner).ensure_role(Role::Guest).is_ok());
    }

    #[test]
    fn ensure_role_rejects_lower_as_not_found() {
        // 404 over 403 — ADR 0008 enumeration-safety rule.
        let err = membership(Role::Cleaner)
            .ensure_role(Role::Manager)
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound));

        let err = membership(Role::Guest)
            .ensure_role(Role::Owner)
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn role_ordering_is_strict() {
        assert!(role_level(Role::Owner) > role_level(Role::Manager));
        assert!(role_level(Role::Manager) > role_level(Role::Cleaner));
        assert!(role_level(Role::Cleaner) > role_level(Role::Guest));
    }
}
