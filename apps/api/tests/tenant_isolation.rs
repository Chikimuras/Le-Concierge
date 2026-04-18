//! ADR 0008 enforcement — every tenant-scoped endpoint must pass the
//! "user B cannot see / mutate user A's resource" assertion. When a new
//! tenant endpoint ships, its equivalent test lands here.

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

async fn signup(app: &TestApp, email: &str, slug: &str) -> (Client, String) {
    let c = client();
    let body: Value = c
        .post(app.url("/auth/signup"))
        .json(&json!({
            "email": email,
            "password": "correct-horse-battery-staple",
            "organization_slug": slug,
            "organization_name": "Test",
        }))
        .send()
        .await
        .expect("signup")
        .json()
        .await
        .expect("json");
    let csrf = body["session"]["csrf_token"]
        .as_str()
        .expect("csrf")
        .to_owned();
    (c, csrf)
}

#[tokio::test]
async fn user_b_cannot_access_user_a_property() {
    let app = spawn_app().await;

    // User A creates org acme + a property P_A.
    let (a, csrf_a) = signup(&app, "alice@example.test", "acme").await;
    let created: Value = a
        .post(app.url("/orgs/acme/properties"))
        .header("x-csrf-token", &csrf_a)
        .json(&json!({ "slug": "chez-alice", "name": "Chez Alice" }))
        .send()
        .await
        .expect("create")
        .json()
        .await
        .expect("json");
    let id = created["id"].as_str().unwrap().to_owned();

    // User B signs up with a different org. (bob owns `bravo`.)
    let (b, csrf_b) = signup(&app, "bob@example.test", "bravo").await;

    // --- Probing alice's slug directly ---
    // GET list under alice's org → 404, never 403 (ADR 0008).
    let list = b
        .get(app.url("/orgs/acme/properties"))
        .send()
        .await
        .expect("list acme");
    assert_eq!(list.status(), StatusCode::NOT_FOUND);

    // GET the property by id under alice's slug → 404.
    let got = b
        .get(app.url(&format!("/orgs/acme/properties/{id}")))
        .send()
        .await
        .expect("get acme");
    assert_eq!(got.status(), StatusCode::NOT_FOUND);

    // PATCH bounces too.
    let patch = b
        .patch(app.url(&format!("/orgs/acme/properties/{id}")))
        .header("x-csrf-token", &csrf_b)
        .json(&json!({ "name": "Stolen" }))
        .send()
        .await
        .expect("patch acme");
    assert_eq!(patch.status(), StatusCode::NOT_FOUND);

    // DELETE bounces too.
    let del = b
        .delete(app.url(&format!("/orgs/acme/properties/{id}")))
        .header("x-csrf-token", &csrf_b)
        .send()
        .await
        .expect("del acme");
    assert_eq!(del.status(), StatusCode::NOT_FOUND);

    // --- Probing alice's property id under bravo's slug ---
    // Even with his own slug, bob cannot reach the id — the query is
    // scoped by org_id, so P_A is invisible.
    let cross_get = b
        .get(app.url(&format!("/orgs/bravo/properties/{id}")))
        .send()
        .await
        .expect("cross get");
    assert_eq!(cross_get.status(), StatusCode::NOT_FOUND);

    // --- Sanity check: alice still owns her property ---
    let alice_sees: Value = a
        .get(app.url(&format!("/orgs/acme/properties/{id}")))
        .send()
        .await
        .expect("alice get")
        .json()
        .await
        .expect("json");
    assert_eq!(alice_sees["id"], id);
    assert_eq!(alice_sees["name"], "Chez Alice");
}

#[tokio::test]
async fn unknown_slug_returns_404_not_403() {
    let app = spawn_app().await;
    let (a, _csrf) = signup(&app, "alice@example.test", "acme").await;

    let resp = a
        .get(app.url("/orgs/imaginary-org/properties"))
        .send()
        .await
        .expect("list");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
