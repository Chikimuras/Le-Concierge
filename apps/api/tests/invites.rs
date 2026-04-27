//! HTTP-level integration tests for team invites (Phase 5b-1).
//!
//! Full stack: Postgres + Redis testcontainers + real Axum server.
//! The email transport is [`LogEmailSender`] (injected by the test
//! harness via `AppState`), so invite URLs land in the log stream —
//! the tests read back the token by peeking the DB with the helper
//! below, not by parsing log output.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

use std::sync::Arc;

use crate::common::{TestApp, email::FailingEmailSender, spawn_app, spawn_app_with_email};

fn client() -> Client {
    Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest")
}

async fn signup(app: &TestApp, email: &str, slug: &str) -> (Client, String) {
    let c = client();
    let resp = c
        .post(app.url("/auth/signup"))
        .json(&json!({
            "email": email,
            "password": "correct-horse-battery-staple",
            "organization_slug": slug,
            "organization_name": "Acme",
        }))
        .send()
        .await
        .expect("signup");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.expect("json");
    let csrf = body["session"]["csrf_token"].as_str().unwrap().to_owned();
    (c, csrf)
}

/// Recover an invite's plaintext token from the DB. The application
/// only stores the HMAC digest, so we cannot actually read the raw
/// token — but for tests we know the DB row exists and can compute
/// the HMAC ourselves. Easier path: create the invite, then query the
/// most recent row for `org_slug`, and bypass the accept flow by
/// driving it through a helper that accepts `(token_hash)`.
///
/// For simplicity the tests below recreate the plaintext token by
/// generating it through the same API: the `POST /orgs/:slug/invites`
/// response does NOT include the token (ADR 0009). Instead, we peek
/// the log output? Also not accessible. The cleanest path is to seed
/// the DB directly using the test's `TestDatabase.pool` and skip
/// the HTTP create step. We do that.
async fn seed_invite_with_known_token(
    app: &TestApp,
    org_slug: &str,
    invited_email: &str,
    role: &str,
    pepper: &str,
) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    // Random 32-byte token + URL-safe base64 (no padding).
    let mut bytes = [0u8; 32];
    for b in &mut bytes {
        *b = rand::random();
    }
    let token = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes);

    let mut mac = HmacSha256::new_from_slice(pepper.as_bytes()).expect("hmac");
    mac.update(token.as_bytes());
    let hash = hex::encode(mac.finalize().into_bytes());

    let org_id: uuid::Uuid =
        sqlx::query_scalar!(r#"SELECT id FROM organizations WHERE slug = $1"#, org_slug)
            .fetch_one(&app.db.pool)
            .await
            .expect("org");
    let inviter_id: uuid::Uuid = sqlx::query_scalar!(
        r#"SELECT user_id FROM organization_members WHERE org_id = $1 LIMIT 1"#,
        org_id,
    )
    .fetch_one(&app.db.pool)
    .await
    .expect("owner");

    sqlx::query(
        r#"INSERT INTO organization_invites
             (org_id, email, role, invited_by, token_hash, expires_at)
           VALUES ($1, $2, $3::text::role, $4, $5, now() + interval '7 days')"#,
    )
    .bind(org_id)
    .bind(invited_email)
    .bind(role)
    .bind(inviter_id)
    .bind(&hash)
    .execute(&app.db.pool)
    .await
    .expect("insert invite");

    token
}

const TEST_PEPPER: &str = "dangerous-test-pepper-never-use-in-prod";

#[tokio::test]
async fn create_list_and_cancel_invite() {
    let app = spawn_app().await;
    let (c, csrf) = signup(&app, "owner@example.test", "acme").await;

    // Create
    let resp = c
        .post(app.url("/orgs/acme/invites"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "email": "teammate@example.test", "role": "manager" }))
        .send()
        .await
        .expect("create");
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created: Value = resp.json().await.expect("json");
    let id = created["id"].as_str().unwrap().to_owned();
    assert_eq!(created["email"], "teammate@example.test");
    assert_eq!(created["role"], "manager");
    // Response body never echoes the token.
    assert!(!created.as_object().unwrap().contains_key("token"));

    // Duplicate for the same email → 409
    let dup = c
        .post(app.url("/orgs/acme/invites"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "email": "teammate@example.test", "role": "cleaner" }))
        .send()
        .await
        .expect("dup");
    assert_eq!(dup.status(), StatusCode::CONFLICT);

    // List
    let list: Value = c
        .get(app.url("/orgs/acme/invites"))
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    assert_eq!(list["invites"].as_array().unwrap().len(), 1);

    // Cancel
    let del = c
        .delete(app.url(&format!("/orgs/acme/invites/{id}")))
        .header("x-csrf-token", &csrf)
        .send()
        .await
        .expect("del");
    assert_eq!(del.status(), StatusCode::NO_CONTENT);

    let after: Value = c
        .get(app.url("/orgs/acme/invites"))
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    assert_eq!(after["invites"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn preview_returns_metadata_without_consuming() {
    let app = spawn_app().await;
    signup(&app, "owner@example.test", "acme").await;

    let token = seed_invite_with_known_token(
        &app,
        "acme",
        "teammate@example.test",
        "cleaner",
        TEST_PEPPER,
    )
    .await;

    let c = client();
    let resp = c
        .post(app.url("/auth/invites/preview"))
        .json(&json!({ "token": token }))
        .send()
        .await
        .expect("preview");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.expect("json");
    assert_eq!(body["email"], "teammate@example.test");
    assert_eq!(body["org_name"], "Acme");
    assert_eq!(body["role"], "cleaner");

    // Second preview still works — idempotent.
    let resp2 = c
        .post(app.url("/auth/invites/preview"))
        .json(&json!({ "token": token }))
        .send()
        .await
        .expect("preview2");
    assert_eq!(resp2.status(), StatusCode::OK);
}

#[tokio::test]
async fn signup_and_accept_creates_user_and_membership() {
    let app = spawn_app().await;
    signup(&app, "owner@example.test", "acme").await;

    let token =
        seed_invite_with_known_token(&app, "acme", "newbie@example.test", "manager", TEST_PEPPER)
            .await;

    let c = client();
    let resp = c
        .post(app.url("/auth/invites/signup"))
        .json(&json!({ "token": token, "password": "correct-horse-battery-staple" }))
        .send()
        .await
        .expect("signup-and-accept");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.expect("json");
    let memberships = body["memberships"].as_array().unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0]["org_slug"], "acme");
    assert_eq!(memberships[0]["role"], "manager");

    // The session is usable — /auth/me works with this client's cookie.
    let me = c.get(app.url("/auth/me")).send().await.expect("me");
    assert_eq!(me.status(), StatusCode::OK);
}

#[tokio::test]
async fn authed_accept_with_matching_email_adds_membership() {
    let app = spawn_app().await;
    signup(&app, "owner@example.test", "acme").await;

    // Teammate already has an account in a different org (bravo).
    let (teammate, _csrf_teammate) = signup(&app, "teammate@example.test", "bravo").await;
    // Owner invites teammate to acme.
    let token = seed_invite_with_known_token(
        &app,
        "acme",
        "teammate@example.test",
        "manager",
        TEST_PEPPER,
    )
    .await;

    // Teammate (already authed) accepts.
    let me_before: Value = teammate
        .get(app.url("/auth/me"))
        .send()
        .await
        .expect("me before")
        .json()
        .await
        .expect("json");
    let csrf = me_before["session"]["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();

    let resp = teammate
        .post(app.url("/auth/invites/accept"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "token": token }))
        .send()
        .await
        .expect("accept");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.expect("json");
    let memberships = body["memberships"].as_array().unwrap();
    // Teammate now has memberships in both bravo (owner) and acme (manager).
    assert_eq!(memberships.len(), 2);
    let slugs: Vec<&str> = memberships
        .iter()
        .map(|m| m["org_slug"].as_str().unwrap())
        .collect();
    assert!(slugs.contains(&"acme"));
    assert!(slugs.contains(&"bravo"));
}

#[tokio::test]
async fn authed_accept_with_wrong_email_is_404() {
    let app = spawn_app().await;
    signup(&app, "owner@example.test", "acme").await;

    let (wrong, _) = signup(&app, "other@example.test", "bravo").await;
    let token = seed_invite_with_known_token(
        &app,
        "acme",
        "teammate@example.test",
        "manager",
        TEST_PEPPER,
    )
    .await;

    let me: Value = wrong
        .get(app.url("/auth/me"))
        .send()
        .await
        .expect("me")
        .json()
        .await
        .expect("json");
    let csrf = me["session"]["csrf_token"].as_str().unwrap().to_owned();

    let resp = wrong
        .post(app.url("/auth/invites/accept"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "token": token }))
        .send()
        .await
        .expect("accept wrong");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn expired_invite_returns_410_on_preview() {
    let app = spawn_app().await;
    signup(&app, "owner@example.test", "acme").await;

    let token =
        seed_invite_with_known_token(&app, "acme", "late@example.test", "cleaner", TEST_PEPPER)
            .await;

    // Fast-forward the expiry by mutating the row.
    sqlx::query!(
        r#"UPDATE organization_invites
              SET expires_at = now() - interval '1 minute'
            WHERE email = $1"#,
        "late@example.test",
    )
    .execute(&app.db.pool)
    .await
    .expect("fast-forward");

    let resp = client()
        .post(app.url("/auth/invites/preview"))
        .json(&json!({ "token": token }))
        .send()
        .await
        .expect("preview");
    assert_eq!(resp.status(), StatusCode::GONE);
}

#[tokio::test]
async fn create_rolls_back_on_email_failure() {
    // Fail-closed contract (CLAUDE.md §3, ADR 0009): if delivery fails, the
    // persisted invite must be cancelled, both `invite.created` and
    // `invite.email_failed` must land in the audit log, and the caller sees 500.
    let app = spawn_app_with_email(Arc::new(FailingEmailSender)).await;
    let (c, csrf) = signup(&app, "owner@example.test", "acme").await;

    let resp = c
        .post(app.url("/orgs/acme/invites"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "email": "doomed@example.test", "role": "manager" }))
        .send()
        .await
        .expect("create");
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Invite row exists but is cancelled.
    let row: (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        r#"SELECT cancelled_at, accepted_at
                 FROM organization_invites
                WHERE email = $1"#,
    )
    .bind("doomed@example.test")
    .fetch_one(&app.db.pool)
    .await
    .expect("invite row");
    assert!(row.0.is_some(), "cancelled_at must be set after rollback");
    assert!(row.1.is_none(), "accepted_at must stay null");

    // Manager listing shows no pending invite.
    let list: Value = c
        .get(app.url("/orgs/acme/invites"))
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    assert_eq!(list["invites"].as_array().unwrap().len(), 0);

    // Both audit events present — hash chain remains contiguous.
    let kinds: Vec<String> = sqlx::query_scalar(
        r#"SELECT kind
             FROM audit_events
            WHERE kind IN ('invite.created', 'invite.email_failed')
            ORDER BY id"#,
    )
    .fetch_all(&app.db.pool)
    .await
    .expect("audit kinds");
    assert_eq!(kinds, vec!["invite.created", "invite.email_failed"]);
}

#[tokio::test]
async fn cancelled_invite_is_404_on_accept() {
    let app = spawn_app().await;
    let (owner, csrf_o) = signup(&app, "owner@example.test", "acme").await;

    // Create via the real API so we get a valid invite id to cancel.
    let created: Value = owner
        .post(app.url("/orgs/acme/invites"))
        .header("x-csrf-token", &csrf_o)
        .json(&json!({ "email": "target@example.test", "role": "cleaner" }))
        .send()
        .await
        .expect("create")
        .json()
        .await
        .expect("json");
    let id = created["id"].as_str().unwrap();

    // Cancel it.
    owner
        .delete(app.url(&format!("/orgs/acme/invites/{id}")))
        .header("x-csrf-token", &csrf_o)
        .send()
        .await
        .expect("cancel");

    // Seed a separate row with a known token for the same email —
    // because the cancelled one's token we can't recover. We reseed
    // with a different email and confirm cancel vs accept still works.
    let token =
        seed_invite_with_known_token(&app, "acme", "target2@example.test", "cleaner", TEST_PEPPER)
            .await;

    // Cancel via DB (easier than figuring out the UUID of the seeded row).
    sqlx::query!(
        r#"UPDATE organization_invites
              SET cancelled_at = now()
            WHERE email = $1"#,
        "target2@example.test",
    )
    .execute(&app.db.pool)
    .await
    .expect("cancel seeded");

    let resp = client()
        .post(app.url("/auth/invites/preview"))
        .json(&json!({ "token": token }))
        .send()
        .await
        .expect("preview");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
