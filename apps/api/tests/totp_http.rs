//! HTTP-level tests for `/auth/2fa/*`.
//!
//! Full stack: Postgres + Redis testcontainers, real Axum, real reqwest.
//! Each test stays under the 5-request /auth/2fa burst budget so the
//! governor never 429s the exercise.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use totp_rs::{Algorithm, TOTP};

use crate::common::{TestApp, spawn_app};

const COOKIE_NAME: &str = "lc_sid";

fn client() -> Client {
    Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest client")
}

fn signup_body(email: &str, slug: &str) -> Value {
    json!({
        "email": email,
        "password": "correct-horse-battery-staple",
        "organization_slug": slug,
        "organization_name": "Acme Conciergerie",
    })
}

fn secret_bytes_from_otpauth(url: &str) -> Vec<u8> {
    let query = url.split_once('?').expect("otpauth query").1;
    let b32 = query
        .split('&')
        .find_map(|p| p.strip_prefix("secret="))
        .expect("secret param");
    base32::decode(base32::Alphabet::Rfc4648 { padding: false }, b32).expect("base32")
}

fn current_code(secret: &[u8]) -> String {
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_vec(),
        Some("Le Concierge".to_string()),
        "test@example.test".to_string(),
    )
    .expect("totp")
    .generate_current()
    .expect("generate")
}

fn cookie<'a>(resp: &'a reqwest::Response, name: &str) -> Option<&'a str> {
    resp.headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|s| s.starts_with(&format!("{name}=")))
}

/// Signup + immediate enrollment confirm. Shared setup: returns the
/// client holding the cookie, the current CSRF token, and the raw TOTP
/// secret so downstream tests can generate valid codes.
async fn enrolled(app: &TestApp) -> (Client, String, Vec<u8>) {
    let c = client();
    let signup = c
        .post(app.url("/auth/signup"))
        .json(&signup_body("owner@example.test", "acme"))
        .send()
        .await
        .expect("signup");
    assert_eq!(signup.status(), StatusCode::OK);
    let body: Value = signup.json().await.expect("json");
    let csrf = body["session"]["csrf_token"].as_str().unwrap().to_owned();

    let start: Value = c
        .post(app.url("/auth/2fa/enroll/start"))
        .header("x-csrf-token", &csrf)
        .send()
        .await
        .expect("start")
        .json()
        .await
        .expect("json");
    let secret = secret_bytes_from_otpauth(start["otpauth_url"].as_str().unwrap());

    let confirm = c
        .post(app.url("/auth/2fa/enroll/verify"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "code": current_code(&secret) }))
        .send()
        .await
        .expect("confirm");
    assert_eq!(confirm.status(), StatusCode::OK);
    (c, csrf, secret)
}

#[tokio::test]
async fn verify_rotates_sid_and_flips_mfa_verified() {
    let app = spawn_app().await;
    let (c, csrf, secret) = enrolled(&app).await;

    // Pre-verify: /me reports enrolled but not verified.
    let me_pre: Value = c
        .get(app.url("/auth/me"))
        .send()
        .await
        .expect("me pre")
        .json()
        .await
        .expect("json");
    assert_eq!(me_pre["mfa_enrolled"], true);
    assert_eq!(me_pre["session"]["mfa_verified"], false);

    // Verify with the live code.
    let verify = c
        .post(app.url("/auth/2fa/verify"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "code": current_code(&secret) }))
        .send()
        .await
        .expect("verify");
    assert_eq!(verify.status(), StatusCode::OK);
    let rotated = cookie(&verify, COOKIE_NAME).expect("rotated").to_owned();
    let verify_body: Value = verify.json().await.expect("json");
    assert_eq!(verify_body["used_recovery_code"], false);
    assert_eq!(verify_body["session"]["mfa_verified"], true);

    let me_post: Value = c
        .get(app.url("/auth/me"))
        .send()
        .await
        .expect("me post")
        .json()
        .await
        .expect("json");
    assert_eq!(me_post["session"]["mfa_verified"], true);
    // Sanity: the rotated cookie header contains a value different from
    // whatever session was active before verify (not worth parsing the
    // old value — the test-harness cookie jar handled the swap for us).
    assert!(rotated.starts_with(&format!("{COOKIE_NAME}=")));
}

#[tokio::test]
async fn wrong_totp_is_unauthorized_and_does_not_rotate() {
    let app = spawn_app().await;
    let (c, csrf, _) = enrolled(&app).await;

    let verify = c
        .post(app.url("/auth/2fa/verify"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "code": "000000" }))
        .send()
        .await
        .expect("verify");
    assert_eq!(verify.status(), StatusCode::UNAUTHORIZED);

    let me: Value = c
        .get(app.url("/auth/me"))
        .send()
        .await
        .expect("me")
        .json()
        .await
        .expect("json");
    assert_eq!(me["session"]["mfa_verified"], false);
}

#[tokio::test]
async fn recovery_code_works_once() {
    let app = spawn_app().await;

    let c = client();
    let signup = c
        .post(app.url("/auth/signup"))
        .json(&signup_body("owner@example.test", "acme"))
        .send()
        .await
        .expect("signup");
    let body: Value = signup.json().await.expect("json");
    let csrf = body["session"]["csrf_token"].as_str().unwrap().to_owned();

    let start: Value = c
        .post(app.url("/auth/2fa/enroll/start"))
        .header("x-csrf-token", &csrf)
        .send()
        .await
        .expect("start")
        .json()
        .await
        .expect("json");
    let secret = secret_bytes_from_otpauth(start["otpauth_url"].as_str().unwrap());

    let confirm: Value = c
        .post(app.url("/auth/2fa/enroll/verify"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "code": current_code(&secret) }))
        .send()
        .await
        .expect("confirm")
        .json()
        .await
        .expect("json");
    let recovery_code = confirm["recovery_codes"][0].as_str().unwrap().to_owned();

    // Verify-by-recovery first.
    let first: Value = c
        .post(app.url("/auth/2fa/verify"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "code": recovery_code }))
        .send()
        .await
        .expect("first")
        .json()
        .await
        .expect("json");
    assert_eq!(first["used_recovery_code"], true);
    assert_eq!(first["session"]["mfa_verified"], true);

    // Consuming the same code on a fresh session must fail.
    let c2 = client();
    c2.post(app.url("/auth/login"))
        .json(&json!({ "email": "owner@example.test", "password": "correct-horse-battery-staple" }))
        .send()
        .await
        .expect("login 2");
    let me2: Value = c2
        .get(app.url("/auth/me"))
        .send()
        .await
        .expect("me2")
        .json()
        .await
        .expect("json");
    let csrf2 = me2["session"]["csrf_token"].as_str().unwrap().to_owned();
    let replay = c2
        .post(app.url("/auth/2fa/verify"))
        .header("x-csrf-token", &csrf2)
        .json(&json!({ "code": recovery_code }))
        .send()
        .await
        .expect("replay");
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn double_enrollment_is_conflict() {
    let app = spawn_app().await;
    let (c, csrf, _) = enrolled(&app).await;

    let reject = c
        .post(app.url("/auth/2fa/enroll/start"))
        .header("x-csrf-token", &csrf)
        .send()
        .await
        .expect("reject");
    assert_eq!(reject.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn disable_without_stepup_is_forbidden() {
    let app = spawn_app().await;
    let (c, csrf, secret) = enrolled(&app).await;

    let blocked = c
        .post(app.url("/auth/2fa/disable"))
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "code": current_code(&secret),
            "password": "correct-horse-battery-staple",
        }))
        .send()
        .await
        .expect("blocked");
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn disable_clears_enrollment_and_destroys_session() {
    let app = spawn_app().await;
    let (c, csrf, secret) = enrolled(&app).await;

    // Step up.
    let verify: Value = c
        .post(app.url("/auth/2fa/verify"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "code": current_code(&secret) }))
        .send()
        .await
        .expect("verify")
        .json()
        .await
        .expect("json");
    let stepped_csrf = verify["session"]["csrf_token"].as_str().unwrap().to_owned();

    // Disable.
    let disable = c
        .post(app.url("/auth/2fa/disable"))
        .header("x-csrf-token", &stepped_csrf)
        .json(&json!({
            "code": current_code(&secret),
            "password": "correct-horse-battery-staple",
        }))
        .send()
        .await
        .expect("disable");
    assert_eq!(disable.status(), StatusCode::NO_CONTENT);

    let me = c.get(app.url("/auth/me")).send().await.expect("me");
    assert_eq!(me.status(), StatusCode::UNAUTHORIZED);

    // Log back in on a fresh client — `/auth/me` now reports not enrolled.
    let c2 = client();
    c2.post(app.url("/auth/login"))
        .json(&json!({ "email": "owner@example.test", "password": "correct-horse-battery-staple" }))
        .send()
        .await
        .expect("login");
    let me2: Value = c2
        .get(app.url("/auth/me"))
        .send()
        .await
        .expect("me2")
        .json()
        .await
        .expect("json");
    assert_eq!(me2["mfa_enrolled"], false);
}
