//! The client's assets, served by the same binary as the API.
//!
//! Acceptance criterion 7 of the Rust rewrite: one binary plus a directory of static assets. The
//! interesting part is not that a file is served — it is what happens to a path that is not a
//! file, because every route in this app is client-side.

mod support;

use axum::http::StatusCode;

/// Writes a stand-in distribution: the shell, and one hashed asset beside it.
fn dist() -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!("aurum-dist-{}", std::process::id()));

    std::fs::create_dir_all(&directory).expect("a temporary directory");
    std::fs::write(directory.join("index.html"), "<!doctype html>the shell").expect("the shell");
    std::fs::write(directory.join("app-abc123.js"), "// the glue").expect("an asset");

    directory
}

#[tokio::test]
async fn serves_the_shell_and_the_assets_beside_it() {
    let server = support::server_serving(dist()).await;

    let (status, body) = server.raw("/index.html").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("the shell"));

    let (status, body) = server.raw("/app-abc123.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "// the glue");
}

/// A reload on `/sets/x` is a path the server has never heard of, and it must answer with the
/// shell rather than a 404 — the route is resolved in the browser.
#[tokio::test]
async fn an_unknown_path_is_the_shell_rather_than_a_404() {
    let server = support::server_serving(dist()).await;

    let (status, body) = server
        .raw("/sets/01890000-0000-7000-8000-000000000000")
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("the shell"));
}

/// The sync engine classifies failures by status and parses the body, so an unknown endpoint
/// must not become an HTML page just because the shell is being served from the same origin.
#[tokio::test]
async fn an_unknown_endpoint_is_still_json() {
    let server = support::server_serving(dist()).await;

    let answer = server.request("GET", "/api/v1/nope", None, None).await;

    assert_eq!(answer.status, StatusCode::NOT_FOUND);
    assert_eq!(answer.code(), "not_found");
}

/// Without a distribution the binary is the API and nothing else, which is what development
/// looks like: Trunk serves the client and proxies the API back here.
#[tokio::test]
async fn without_a_distribution_nothing_but_the_api_is_served() {
    let server = support::server().await;

    let answer = server.request("GET", "/index.html", None, None).await;

    assert_eq!(answer.status, StatusCode::NOT_FOUND);
}
