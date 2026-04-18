//! HTTP-level integration tests for `/auth/*`.
//!
//! Full stack: Postgres + Redis testcontainers, real Axum server on
//! loopback, real `reqwest` client. Verifies the signup → cookie → /me →
//! logout cycle plus lockout, CSRF, and audit-chain wiring.

#![allow(clippy::expect_used, clippy::unwrap_used)] // test assertions

mod common;

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

use crate::common::spawn_app;

const COOKIE_NAME: &str = "lc_sid";

fn client() -> Client {
    Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest client")
}

fn signup_body() -> Value {
    json!({
        "email": "owner@example.test",
        "password": "correct-horse-battery-staple",
        "organization_slug": "acme",
        "organization_name": "Acme Conciergerie",
    })
}

#[tokio::test]
async fn signup_sets_cookie_and_me_resolves() {
    let app = spawn_app().await;
    let client = client();

    let resp = client
        .post(app.url("/auth/signup"))
        .json(&signup_body())
        .send()
        .await
        .expect("signup");
    assert_eq!(resp.status(), StatusCode::OK);
    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .expect("set-cookie present");
    assert!(set_cookie.starts_with(&format!("{COOKIE_NAME}=")));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));

    let body: Value = resp.json().await.expect("json");
    let csrf = body["session"]["csrf_token"]
        .as_str()
        .expect("csrf_token")
        .to_owned();

    let me = client
        .get(app.url("/auth/me"))
        .send()
        .await
        .expect("me")
        .json::<Value>()
        .await
        .expect("me json");
    assert_eq!(me["memberships"][0]["org_slug"], "acme");
    assert_eq!(me["is_platform_admin"], false);

    // The /me response surfaces the same token the signup returned.
    assert_eq!(me["session"]["csrf_token"], csrf);
}

#[tokio::test]
async fn duplicate_signup_returns_409() {
    let app = spawn_app().await;
    // First signup on one client (which will hang onto its cookies).
    client()
        .post(app.url("/auth/signup"))
        .json(&signup_body())
        .send()
        .await
        .expect("first");

    // Second attempt comes from a *fresh* client so no session cookie is
    // present — otherwise the CSRF middleware would intercept the POST
    // before the handler gets a chance to produce the 409. In practice
    // the collision happens when an anonymous visitor re-tries a slug
    // that is already taken, which this mirrors.
    let mut second = signup_body();
    second["organization_slug"] = json!("acme-two");
    let resp = client()
        .post(app.url("/auth/signup"))
        .json(&second)
        .send()
        .await
        .expect("second");
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn logout_without_csrf_token_is_rejected() {
    let app = spawn_app().await;
    let client = client();
    client
        .post(app.url("/auth/signup"))
        .json(&signup_body())
        .send()
        .await
        .expect("signup");

    let resp = client
        .post(app.url("/auth/logout"))
        .send()
        .await
        .expect("logout");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "CSRF middleware must reject unsafe methods without X-CSRF-Token"
    );
}

#[tokio::test]
async fn logout_with_csrf_destroys_session() {
    let app = spawn_app().await;
    let client = client();
    let signup_resp: Value = client
        .post(app.url("/auth/signup"))
        .json(&signup_body())
        .send()
        .await
        .expect("signup")
        .json()
        .await
        .expect("json");
    let csrf = signup_resp["session"]["csrf_token"]
        .as_str()
        .expect("csrf")
        .to_owned();

    let logout = client
        .post(app.url("/auth/logout"))
        .header("x-csrf-token", &csrf)
        .send()
        .await
        .expect("logout");
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);

    let me = client.get(app.url("/auth/me")).send().await.expect("me");
    assert_eq!(me.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_with_wrong_password_returns_401_opaquely() {
    let app = spawn_app().await;
    let client = client();
    client
        .post(app.url("/auth/signup"))
        .json(&signup_body())
        .send()
        .await
        .expect("signup");

    // Wipe the cookie jar so login runs anonymously.
    let client2 = self::client();
    let resp = client2
        .post(app.url("/auth/login"))
        .json(&json!({
            "email": "owner@example.test",
            "password": "wrong-password-guess",
        }))
        .send()
        .await
        .expect("login");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn five_wrong_passwords_lock_the_account() {
    let app = spawn_app().await;
    client()
        .post(app.url("/auth/signup"))
        .json(&signup_body())
        .send()
        .await
        .expect("signup");

    // `SmartIpKeyExtractor` in the rate-limit layer honours X-Forwarded-For,
    // so cycling through synthetic IPs on each attempt bypasses the
    // IP-based rate limit and isolates the test to the *account lockout*
    // logic we want to verify here (DB-level lockout is also exercised
    // at the repo level in tests/auth.rs).
    let attempts = client();
    for i in 0..5 {
        let _ = attempts
            .post(app.url("/auth/login"))
            .header("x-forwarded-for", format!("10.0.0.{i}"))
            .json(&json!({
                "email": "owner@example.test",
                "password": "nope",
            }))
            .send()
            .await;
    }

    // A 6th attempt from yet another IP still 401s because the lockout
    // now lives on the user row, not on the IP.
    let good = attempts
        .post(app.url("/auth/login"))
        .header("x-forwarded-for", "10.0.0.99")
        .json(&json!({
            "email": "owner@example.test",
            "password": "correct-horse-battery-staple",
        }))
        .send()
        .await
        .expect("good");
    assert_eq!(good.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn audit_chain_is_populated_by_signup() {
    let app = spawn_app().await;
    let client = client();
    client
        .post(app.url("/auth/signup"))
        .json(&signup_body())
        .send()
        .await
        .expect("signup");

    // Check that at least one audit event landed with the expected kind
    // and a non-null hash. Chain integrity across many events is covered
    // by the audit module's unit tests.
    let row: (String, Vec<u8>) =
        sqlx::query_as("SELECT kind, hash FROM audit_events ORDER BY id DESC LIMIT 1")
            .fetch_one(&app.db.pool)
            .await
            .expect("audit row");
    assert_eq!(row.0, "auth.signup");
    assert_eq!(row.1.len(), 32, "hash must be 32 bytes of SHA-256 output");
}
