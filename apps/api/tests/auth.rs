//! Integration tests for the `auth` domain.
//!
//! Each test boots a fresh Postgres container (via `testcontainers-modules`),
//! applies every migration, and exercises [`AuthService::signup_organization`]
//! end-to-end. No DB mock, per `CLAUDE.md` §4.2.

#![allow(clippy::expect_used, clippy::unwrap_used)] // test assertions

mod common;

use api::auth::{AuthError, AuthRepo, AuthService, SignupInput};
use secrecy::SecretString;
use sqlx::PgPool;

use crate::common::db::TestDatabase;

async fn count_users(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(r#"SELECT COUNT(*) AS "n!" FROM users"#)
        .fetch_one(pool)
        .await
        .expect("count users")
}

fn service_for(db: &TestDatabase) -> AuthService {
    AuthService::new(
        AuthRepo::new(db.pool.clone()),
        SecretString::from("test-pepper-long-enough-for-integration"),
    )
}

fn valid_signup() -> SignupInput {
    SignupInput {
        email: "owner@example.test".into(),
        password: "correct-horse-battery-staple".into(),
        organization_slug: "acme".into(),
        organization_name: "Acme Concierge".into(),
    }
}

#[tokio::test]
async fn signup_creates_org_user_and_owner_membership() {
    let db = TestDatabase::spawn().await;
    let svc = service_for(&db);

    let outcome = svc
        .signup_organization(valid_signup())
        .await
        .expect("signup ok");

    // Organization exists with the expected slug.
    let org_slug: String = sqlx::query_scalar!(
        "SELECT slug FROM organizations WHERE id = $1",
        outcome.organization_id.into_inner()
    )
    .fetch_one(&db.pool)
    .await
    .expect("org row present");
    assert_eq!(org_slug, "acme");

    // User exists, email is lowercased, hash is a PHC-format Argon2id string.
    let (email, password_hash): (String, String) =
        sqlx::query_as("SELECT email::text, password_hash FROM users WHERE id = $1")
            .bind(outcome.owner_user_id.into_inner())
            .fetch_one(&db.pool)
            .await
            .expect("user row present");
    assert_eq!(email, "owner@example.test");
    assert!(
        password_hash.starts_with("$argon2id$"),
        "expected Argon2id PHC hash, got {password_hash}",
    );

    // Membership is owner.
    let role: String = sqlx::query_scalar!(
        r#"
        SELECT role::text AS "role!"
        FROM organization_members
        WHERE user_id = $1 AND org_id = $2
        "#,
        outcome.owner_user_id.into_inner(),
        outcome.organization_id.into_inner(),
    )
    .fetch_one(&db.pool)
    .await
    .expect("membership row present");
    assert_eq!(role, "owner");
}

#[tokio::test]
async fn signup_is_case_insensitive_on_email() {
    let db = TestDatabase::spawn().await;
    let svc = service_for(&db);

    svc.signup_organization(valid_signup())
        .await
        .expect("first ok");

    let mut second = valid_signup();
    second.email = "OWNER@Example.TEST".into();
    second.organization_slug = "other-org".into();
    second.organization_name = "Other Org".into();

    let err = svc
        .signup_organization(second)
        .await
        .expect_err("duplicate email must fail");
    assert!(matches!(err, AuthError::EmailAlreadyTaken));
}

#[tokio::test]
async fn duplicate_slug_is_rejected_with_specific_error() {
    let db = TestDatabase::spawn().await;
    let svc = service_for(&db);

    svc.signup_organization(valid_signup())
        .await
        .expect("first ok");

    let mut second = valid_signup();
    second.email = "another@example.test".into();

    let err = svc
        .signup_organization(second)
        .await
        .expect_err("duplicate slug must fail");
    assert!(matches!(err, AuthError::SlugAlreadyTaken));
}

#[tokio::test]
async fn invalid_email_does_not_touch_the_db() {
    let db = TestDatabase::spawn().await;
    let svc = service_for(&db);

    let mut input = valid_signup();
    input.email = "not-an-email".into();

    let err = svc
        .signup_organization(input)
        .await
        .expect_err("validation");
    assert!(matches!(err, AuthError::InvalidEmail));

    let users_after = count_users(&db.pool).await;
    assert_eq!(
        users_after, 0,
        "failed validation must not have written rows"
    );
}

#[tokio::test]
async fn weak_password_is_rejected() {
    let db = TestDatabase::spawn().await;
    let svc = service_for(&db);

    let mut input = valid_signup();
    input.password = "short".into();

    let err = svc
        .signup_organization(input)
        .await
        .expect_err("validation");
    assert!(matches!(err, AuthError::WeakPassword));
}

#[tokio::test]
async fn audit_events_cannot_be_updated_or_deleted() {
    // The table is empty at this point, but the triggers still fire when a
    // row is inserted. We exercise them directly to catch accidental trigger
    // removal in a future migration.
    let db = TestDatabase::spawn().await;

    sqlx::query!(
        r#"
        INSERT INTO audit_events (kind, payload, hash)
        VALUES ('test.event', '{}'::jsonb, decode(repeat('ab', 32), 'hex'))
        "#
    )
    .execute(&db.pool)
    .await
    .expect("insert test audit event");

    let update_err = sqlx::query!("UPDATE audit_events SET kind = 'tampered'")
        .execute(&db.pool)
        .await;
    assert!(update_err.is_err(), "UPDATE on audit_events must fail");

    let delete_err = sqlx::query!("DELETE FROM audit_events")
        .execute(&db.pool)
        .await;
    assert!(delete_err.is_err(), "DELETE on audit_events must fail");
}
