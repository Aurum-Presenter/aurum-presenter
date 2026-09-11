//! One explicit router.
//!
//! The PHP discovered these thirty-two routes reflectively from attributes at build time, and
//! read their authorization back out of route options in middleware. Here the list is a list,
//! and authorization is the extractor each handler asks for — `Workspace<Manage>` rather than
//! `options: [PERMISSION => WorkspaceManage]`. A route whose permission is wrong no longer
//! compiles into something that runs; it does not compile.

use std::path::Path;

use axum::Json;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{any, delete, get, patch, post};
use axum::{Router, response::IntoResponse};
use serde_json::json;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::handlers::{account, auth, files, sync, workspaces};
use crate::state::AppState;

/// The API, plus the client's assets when this deployment carries them.
pub fn router(state: AppState) -> Router {
    match state.config.web_dir.clone() {
        // Acceptance criterion 7: one binary and a directory of static assets. An unknown path
        // under `/api/v1` still answers in JSON — only everything else becomes the shell.
        Some(dir) => api_router(state)
            .route("/api/v1/{*rest}", any(not_found))
            .fallback_service(shell(&dir)),
        None => api_router(state),
    }
}

/// The shell for every client-side route.
///
/// The fallback has to be `index.html` itself rather than a 404: every route in the app is
/// client-side — `/sets`, `/song/x`, `/present/y` — and a reload on any of them is a request the
/// server has never heard of.
fn shell(dir: &Path) -> ServeDir<ServeFile> {
    ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))
}

fn api_router(state: AppState) -> Router {
    let cors = cors_layer(&state);

    Router::new()
        .route("/api/v1/health", get(health))
        // Public: reachable without a session.
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/login/totp", post(auth::login_totp))
        .route("/api/v1/auth/refresh", post(auth::refresh))
        .route("/api/v1/auth/password/forgot", post(auth::forgot_password))
        .route("/api/v1/auth/password/reset", post(auth::reset_password))
        .route("/api/v1/invites/{token}", get(workspaces::preview_invite))
        // Signed in.
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/account", get(account::me))
        .route("/api/v1/account/totp/enrol", post(account::totp_enrol))
        .route("/api/v1/account/totp/confirm", post(account::totp_confirm))
        .route("/api/v1/account/totp", delete(account::totp_disable))
        .route(
            "/api/v1/account/totp/recovery-codes",
            post(account::recovery_codes),
        )
        .route("/api/v1/invites/accept", post(workspaces::accept_invite))
        .route(
            "/api/v1/workspaces",
            get(workspaces::list).post(workspaces::create),
        )
        // Workspace-scoped. The permission each needs is in the handler's own signature.
        .route(
            "/api/v1/workspaces/{workspace}/members",
            get(workspaces::members),
        )
        .route(
            "/api/v1/workspaces/{workspace}/members/{user}",
            patch(workspaces::update_member).delete(workspaces::remove_member),
        )
        .route(
            "/api/v1/workspaces/{workspace}/invites",
            get(workspaces::list_invites).post(workspaces::create_invite),
        )
        .route(
            "/api/v1/workspaces/{workspace}/invites/{invite}",
            delete(workspaces::revoke_invite),
        )
        .route("/api/v1/workspaces/{workspace}/sync/pull", get(sync::pull))
        .route("/api/v1/workspaces/{workspace}/sync/push", post(sync::push))
        .route(
            "/api/v1/workspaces/{workspace}/sync/conflicts",
            get(sync::conflicts),
        )
        .route(
            "/api/v1/workspaces/{workspace}/sheets/{sheet}/url",
            get(files::sheet_download_url),
        )
        .route(
            "/api/v1/workspaces/{workspace}/sheets/{sheet}/upload-url",
            post(files::sheet_upload_url),
        )
        .route(
            "/api/v1/workspaces/{workspace}/sheets/{sheet}/complete",
            post(files::sheet_complete),
        )
        .route(
            "/api/v1/workspaces/{workspace}/assets/{asset}/url",
            get(files::asset_url),
        )
        .route(
            "/api/v1/workspaces/{workspace}/assets/upload-url",
            post(files::asset_upload_url),
        )
        .route(
            "/api/v1/workspaces/{workspace}/assets/complete",
            post(files::asset_complete),
        )
        .fallback(not_found)
        // Outermost, above everything: an *error* response has to carry the CORS headers too.
        // Inside, it would only ever decorate a response that came back normally, and the
        // browser would then refuse to let the app read the 401 it was given — which is exactly
        // the response the client needs most, because it is the one that triggers a refresh.
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Credentials are allowed because the refresh token travels as an httpOnly cookie, which in
/// turn forbids a wildcard origin — so the origin is echoed only when it is on the list.
fn cors_layer(state: &AppState) -> CorsLayer {
    let origins: Vec<HeaderValue> = state
        .config
        .cors_allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::IF_MATCH,
        ])
        .max_age(std::time::Duration::from_secs(600))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// Even "no such route" is a JSON envelope: an API that answers with an HTML error page breaks
/// the sync engine's retry logic, which classifies by status and expects a parseable body.
async fn not_found() -> impl IntoResponse {
    crate::error::ApiError::not_found("No such endpoint.")
}
