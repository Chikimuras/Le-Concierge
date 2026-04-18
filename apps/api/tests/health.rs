//! Integration tests for the `health` domain and the global middleware
//! stack. Lifting the whole app gives us the same code path the binary
//! exercises in production, with the single difference that the DB pool is
//! lazy and no test in this file issues a query.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test assertions

mod common;

use common::spawn_app;

#[tokio::test]
async fn healthz_returns_ok_with_service_metadata() {
    let app = spawn_app().await;
    let resp = reqwest::get(app.url("/healthz")).await.expect("send");

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert!(
        resp.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.starts_with("application/json")),
        "/healthz must advertise JSON",
    );

    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "api");
    assert!(
        body["version"].as_str().is_some_and(|v| !v.is_empty()),
        "version must be a non-empty string",
    );
}

#[tokio::test]
async fn healthz_emits_security_response_headers() {
    let app = spawn_app().await;
    let resp = reqwest::get(app.url("/healthz")).await.expect("send");
    let headers = resp.headers();

    // CLAUDE.md §3.2 / ADR 0002 — OWASP ASVS 14.4.
    let hsts = headers
        .get("strict-transport-security")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(hsts.contains("max-age=63072000"), "HSTS missing max-age");
    assert!(
        hsts.contains("includeSubDomains"),
        "HSTS missing subdomains"
    );
    assert!(hsts.contains("preload"), "HSTS missing preload");

    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(
        headers.get("referrer-policy").unwrap(),
        "strict-origin-when-cross-origin"
    );
    assert!(
        headers.contains_key("permissions-policy"),
        "Permissions-Policy missing",
    );
    assert!(
        headers.contains_key("content-security-policy"),
        "CSP missing on JSON response",
    );
    // JSON response should be locked down.
    let csp = headers
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        csp.contains("default-src 'none'"),
        "CSP too permissive: {csp}"
    );
}

#[tokio::test]
async fn request_id_is_generated_when_absent() {
    let app = spawn_app().await;
    let resp = reqwest::get(app.url("/healthz")).await.expect("send");

    let id = resp
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .expect("request-id echoed");
    assert!(!id.is_empty(), "generated request-id must not be empty");
}

#[tokio::test]
async fn request_id_is_preserved_when_supplied() {
    let app = spawn_app().await;
    let supplied = "11111111-2222-3333-4444-555555555555";

    let resp = reqwest::Client::new()
        .get(app.url("/healthz"))
        .header("x-request-id", supplied)
        .send()
        .await
        .expect("send");

    assert_eq!(
        resp.headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok()),
        Some(supplied),
        "caller-supplied x-request-id must be echoed verbatim",
    );
}

#[tokio::test]
async fn openapi_document_is_served_and_mentions_health() {
    let app = spawn_app().await;
    let resp = reqwest::get(app.url("/openapi.json")).await.expect("send");

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let doc: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(doc["info"]["title"], "Le Concierge API");
    assert!(
        doc["paths"]["/healthz"].is_object(),
        "OpenAPI must advertise /healthz (got: {doc})",
    );
}

#[tokio::test]
async fn docs_endpoint_relaxes_csp_for_scalar_assets() {
    let app = spawn_app().await;
    let resp = reqwest::get(app.url("/docs")).await.expect("send");

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let csp = resp
        .headers()
        .get("content-security-policy")
        .and_then(|v| v.to_str().ok())
        .expect("CSP present on /docs");
    // /docs needs the CDN, but must not fall back to `default-src 'none'`.
    assert!(
        csp.contains("cdn.jsdelivr.net"),
        "/docs CSP must allow Scalar CDN (got: {csp})",
    );
    assert!(
        !csp.contains("default-src 'none'"),
        "/docs must NOT inherit the strict JSON CSP (got: {csp})",
    );
}
