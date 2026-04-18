//! Repo-level integration tests for the `auth` domain.
//!
//! Each test boots a fresh Postgres container, runs every migration, and
//! exercises [`AuthRepo`] directly. The HTTP-facing behaviour of the
//! wider [`AuthService`] (including session creation and audit events)
//! is covered by `tests/auth_http.rs`, which also spins up Redis.

#![allow(clippy::expect_used, clippy::unwrap_used)] // test assertions

mod common;

use api::auth::domain::{Email, PasswordHash, Slug};
use api::auth::{AuthError, AuthRepo};
use secrecy::SecretString;
use sqlx::PgPool;

use crate::common::db::TestDatabase;

async fn count_users(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(r#"SELECT COUNT(*) AS "n!" FROM users"#)
        .fetch_one(pool)
        .await
        .expect("count users")
}

/// Produce a throw-away hash so tests don't pay for a real Argon2 round
/// on every run.
fn stub_hash() -> PasswordHash {
    let pepper = SecretString::from("stub-test-pepper-for-integration-runs");
    api::auth::hash::hash_password("correct-horse-battery-staple", &pepper).expect("hash ok")
}

#[tokio::test]
async fn create_org_persists_org_user_and_owner_membership() {
    let db = TestDatabase::spawn().await;
    let repo = AuthRepo::new(db.pool.clone());
    let email = Email::parse("owner@example.test").expect("valid");
    let slug = Slug::parse("acme").expect("valid");

    let (org_id, user_id) = repo
        .create_organization_with_owner(&email, &stub_hash(), &slug, "Acme Conciergerie")
        .await
        .expect("create ok");

    let (db_slug,): (String,) = sqlx::query_as("SELECT slug FROM organizations WHERE id = $1")
        .bind(org_id.into_inner())
        .fetch_one(&db.pool)
        .await
        .expect("org present");
    assert_eq!(db_slug, "acme");

    let (db_email,): (String,) = sqlx::query_as("SELECT email::text FROM users WHERE id = $1")
        .bind(user_id.into_inner())
        .fetch_one(&db.pool)
        .await
        .expect("user present");
    assert_eq!(db_email, "owner@example.test");

    let role: String = sqlx::query_scalar!(
        r#"
        SELECT role::text AS "role!"
        FROM organization_members
        WHERE user_id = $1 AND org_id = $2
        "#,
        user_id.into_inner(),
        org_id.into_inner(),
    )
    .fetch_one(&db.pool)
    .await
    .expect("membership row");
    assert_eq!(role, "owner");
}

#[tokio::test]
async fn duplicate_email_is_rejected() {
    let db = TestDatabase::spawn().await;
    let repo = AuthRepo::new(db.pool.clone());
    let email = Email::parse("owner@example.test").expect("valid");

    repo.create_organization_with_owner(
        &email,
        &stub_hash(),
        &Slug::parse("one").expect("valid"),
        "One",
    )
    .await
    .expect("first ok");

    let err = repo
        .create_organization_with_owner(
            &Email::parse("OWNER@Example.TEST").expect("valid"),
            &stub_hash(),
            &Slug::parse("two").expect("valid"),
            "Two",
        )
        .await
        .expect_err("must fail");
    assert!(matches!(err, AuthError::EmailAlreadyTaken));
    assert_eq!(count_users(&db.pool).await, 1);
}

#[tokio::test]
async fn duplicate_slug_is_rejected() {
    let db = TestDatabase::spawn().await;
    let repo = AuthRepo::new(db.pool.clone());

    repo.create_organization_with_owner(
        &Email::parse("one@example.test").expect("valid"),
        &stub_hash(),
        &Slug::parse("acme").expect("valid"),
        "Acme",
    )
    .await
    .expect("first ok");

    let err = repo
        .create_organization_with_owner(
            &Email::parse("two@example.test").expect("valid"),
            &stub_hash(),
            &Slug::parse("acme").expect("valid"),
            "Acme Two",
        )
        .await
        .expect_err("must fail");
    assert!(matches!(err, AuthError::SlugAlreadyTaken));
}

#[tokio::test]
async fn progressive_lockout_applies_after_five_failures() {
    let db = TestDatabase::spawn().await;
    let repo = AuthRepo::new(db.pool.clone());
    let email = Email::parse("bob@example.test").expect("valid");

    let (_, user_id) = repo
        .create_organization_with_owner(
            &email,
            &stub_hash(),
            &Slug::parse("acme-lock").expect("valid"),
            "Acme",
        )
        .await
        .expect("create");

    // First four failures — no lockout.
    for _ in 0..4 {
        let window = repo.record_failed_login(user_id).await.expect("inc");
        assert!(window.is_none(), "should not lock before 5");
    }

    let locked_until = repo
        .record_failed_login(user_id)
        .await
        .expect("inc")
        .expect("fifth attempt must lock");
    assert!(locked_until > chrono::Utc::now());

    // Reset clears both counter and lockout.
    repo.reset_failed_logins(user_id).await.expect("reset");
    let user = repo
        .find_user_by_email(&email)
        .await
        .expect("find")
        .expect("present");
    assert_eq!(user.failed_login_attempts, 0);
    assert!(user.locked_until.is_none());
}

#[tokio::test]
async fn memberships_are_listed_for_owner() {
    let db = TestDatabase::spawn().await;
    let repo = AuthRepo::new(db.pool.clone());

    let (_, user_id) = repo
        .create_organization_with_owner(
            &Email::parse("alice@example.test").expect("valid"),
            &stub_hash(),
            &Slug::parse("alice-co").expect("valid"),
            "Alice & Co",
        )
        .await
        .expect("create");

    let memberships = repo.list_memberships(user_id).await.expect("list");
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].org_slug, "alice-co");
    assert_eq!(memberships[0].role, api::auth::Role::Owner);

    assert!(!repo.is_platform_admin(user_id).await.expect("check"));
}

#[tokio::test]
async fn audit_events_cannot_be_updated_or_deleted() {
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
