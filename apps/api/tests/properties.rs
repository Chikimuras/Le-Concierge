//! HTTP-level integration tests for `/orgs/:slug/properties*`.
//!
//! Each test exercises a slice of the CRUD + validation surface against
//! Postgres + Redis testcontainers. Tenant isolation is covered
//! separately in `tests/tenant_isolation.rs`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

use crate::common::{TestApp, spawn_app};

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

/// Signup + return a logged-in client (cookies attached) and the CSRF
/// token for the fresh session.
async fn signed_up(app: &TestApp, email: &str, slug: &str) -> (Client, String) {
    let c = client();
    let resp = c
        .post(app.url("/auth/signup"))
        .json(&signup_body(email, slug))
        .send()
        .await
        .expect("signup");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.expect("json");
    let csrf = body["session"]["csrf_token"]
        .as_str()
        .expect("csrf")
        .to_owned();
    (c, csrf)
}

#[tokio::test]
async fn create_list_get_update_delete_round_trip() {
    let app = spawn_app().await;
    let (c, csrf) = signed_up(&app, "owner@example.test", "acme").await;

    // Empty list on a fresh org.
    let empty: Value = c
        .get(app.url("/orgs/acme/properties"))
        .send()
        .await
        .expect("list 0")
        .json()
        .await
        .expect("json");
    assert_eq!(empty["properties"].as_array().unwrap().len(), 0);

    // Create.
    let create = c
        .post(app.url("/orgs/acme/properties"))
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "slug": "chez-alex",
            "name": "Chez Alex",
            "bedrooms": 2,
            "max_guests": 4,
            "country": "fr",
        }))
        .send()
        .await
        .expect("create");
    assert_eq!(create.status(), StatusCode::CREATED);
    let created: Value = create.json().await.expect("json");
    let id = created["id"].as_str().unwrap().to_owned();
    assert_eq!(created["slug"], "chez-alex");
    assert_eq!(created["country"], "FR", "country should be uppercased");
    assert_eq!(created["timezone"], "Europe/Paris");
    assert_eq!(created["bedrooms"], 2);

    // List picks it up.
    let list: Value = c
        .get(app.url("/orgs/acme/properties"))
        .send()
        .await
        .expect("list 1")
        .json()
        .await
        .expect("json");
    assert_eq!(list["properties"].as_array().unwrap().len(), 1);

    // Get single.
    let got: Value = c
        .get(app.url(&format!("/orgs/acme/properties/{id}")))
        .send()
        .await
        .expect("get")
        .json()
        .await
        .expect("json");
    assert_eq!(got["id"], id);

    // Update (PATCH).
    let updated: Value = c
        .patch(app.url(&format!("/orgs/acme/properties/{id}")))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "name": "Chez Alex - Centre", "bedrooms": 3 }))
        .send()
        .await
        .expect("patch")
        .json()
        .await
        .expect("json");
    assert_eq!(updated["name"], "Chez Alex - Centre");
    assert_eq!(updated["bedrooms"], 3);
    // Unchanged fields stay put.
    assert_eq!(updated["max_guests"], 4);

    // Delete (soft).
    let del = c
        .delete(app.url(&format!("/orgs/acme/properties/{id}")))
        .header("x-csrf-token", &csrf)
        .send()
        .await
        .expect("delete");
    assert_eq!(del.status(), StatusCode::NO_CONTENT);

    // Gone from list.
    let after: Value = c
        .get(app.url("/orgs/acme/properties"))
        .send()
        .await
        .expect("list after")
        .json()
        .await
        .expect("json");
    assert_eq!(after["properties"].as_array().unwrap().len(), 0);

    // Direct get → 404.
    let get_gone = c
        .get(app.url(&format!("/orgs/acme/properties/{id}")))
        .send()
        .await
        .expect("get gone");
    assert_eq!(get_gone.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn duplicate_slug_is_409() {
    let app = spawn_app().await;
    let (c, csrf) = signed_up(&app, "owner@example.test", "acme").await;

    c.post(app.url("/orgs/acme/properties"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "slug": "dup", "name": "First" }))
        .send()
        .await
        .expect("first");

    let second = c
        .post(app.url("/orgs/acme/properties"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "slug": "dup", "name": "Second" }))
        .send()
        .await
        .expect("second");
    assert_eq!(second.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn invalid_slug_is_422() {
    let app = spawn_app().await;
    let (c, csrf) = signed_up(&app, "owner@example.test", "acme").await;

    let resp = c
        .post(app.url("/orgs/acme/properties"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "slug": "NotValid!", "name": "Oops" }))
        .send()
        .await
        .expect("create");
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn bedrooms_out_of_range_is_422() {
    let app = spawn_app().await;
    let (c, csrf) = signed_up(&app, "owner@example.test", "acme").await;

    let resp = c
        .post(app.url("/orgs/acme/properties"))
        .header("x-csrf-token", &csrf)
        .json(&json!({ "slug": "ok", "name": "Ok", "bedrooms": 999 }))
        .send()
        .await
        .expect("create");
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn unauthenticated_list_is_401() {
    let app = spawn_app().await;
    let resp = client()
        .get(app.url("/orgs/acme/properties"))
        .send()
        .await
        .expect("anon");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_slug_for_logged_in_user_is_404() {
    let app = spawn_app().await;
    let (c, _csrf) = signed_up(&app, "owner@example.test", "acme").await;

    let resp = c
        .get(app.url("/orgs/does-not-exist/properties"))
        .send()
        .await
        .expect("list");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
