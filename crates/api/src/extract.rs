//! What a handler is handed, and what it had to prove to be handed it.
//!
//! The PHP declared authorization in a route attribute and enforced it in middleware that read
//! the attribute back reflectively. Here the permission is a *type*: a handler that wants to
//! write asks for `Workspace<Write>`, and a handler that forgot to ask cannot touch a workspace
//! at all, because opening one needs a value only the extractor can make. The check moves from
//! runtime to the signature.

use std::marker::PhantomData;

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::db::{blocking, now_ms};
use crate::error::{ApiError, ApiResult};
use crate::repo::accounts::User;
use crate::repo::workspaces::Role;
use crate::repo::{accounts, workspaces};
use crate::state::AppState;

/// The caller, resolved from the bearer access token.
#[derive(Clone, Debug)]
pub struct Caller {
    pub user: User,
    pub session_id: String,
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> ApiResult<Caller> {
        let token = bearer_token(parts)
            .ok_or_else(|| {
                ApiError::unauthorized("A bearer access token is required.")
                    .with_code("missing_token")
            })?
            .to_owned();

        // Distinguished from a missing token so the client knows to attempt a refresh rather
        // than send the user back to the sign-in screen.
        let claims = state
            .tokens
            .verify_access_token(&token, now_ms())
            .ok_or_else(|| {
                ApiError::unauthorized("Access token is invalid or expired.")
                    .with_code("token_expired")
            })?;

        let state = state.clone();

        blocking(move || {
            let db = state.db.open_control()?;

            let session = crate::repo::sessions::find_by_id(&db, &claims.sid)?
                .filter(crate::repo::sessions::Session::carries_access_tokens)
                .ok_or_else(|| {
                    ApiError::unauthorized("This session has been revoked.")
                        .with_code("session_revoked")
                })?;

            let user = accounts::find_by_id(&db, &claims.sub)?.ok_or_else(|| {
                ApiError::unauthorized("Account no longer exists.").with_code("account_missing")
            })?;

            Ok(Caller {
                user,
                session_id: session.id,
            })
        })
        .await
    }
}

fn bearer_token(parts: &Parts) -> Option<&str> {
    let header = parts.headers.get("authorization")?.to_str().ok()?;
    let (scheme, token) = header.split_once(' ')?;

    scheme.eq_ignore_ascii_case("Bearer").then(|| token.trim())
}

/// What a route needs of the caller's role in the workspace it names.
pub trait Permission: Send + Sync + 'static {
    fn granted(role: Role) -> bool;
}

/// Anyone who belongs to the workspace.
pub struct Read;
/// An editor or an owner: everything that changes shared content.
pub struct Write;
/// The owner alone: members, invites, roles.
pub struct Manage;

impl Permission for Read {
    fn granted(role: Role) -> bool {
        role.can_read()
    }
}

impl Permission for Write {
    fn granted(role: Role) -> bool {
        role.can_write()
    }
}

impl Permission for Manage {
    fn granted(role: Role) -> bool {
        role.can_manage()
    }
}

/// A workspace the caller has been verified to hold `P` in.
///
/// This is the structural half of what replaced row-level security. RLS made every query carry a
/// predicate that could be forgotten; here a handler that was not granted the workspace has no
/// value with which to open one, so there is no query to get wrong.
pub struct Workspace<P: Permission> {
    pub id: String,
    pub record: workspaces::Workspace,
    pub role: Role,
    pub caller: Caller,
    permission: PhantomData<P>,
}

impl<P: Permission> Workspace<P> {
    /// The content tier for this workspace. Reachable only from a value the extractor made.
    pub fn open(&self, state: &AppState) -> ApiResult<Connection> {
        state.db.open_workspace(&self.id)
    }
}

impl<P: Permission> FromRequestParts<AppState> for Workspace<P> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> ApiResult<Workspace<P>> {
        let caller = Caller::from_request_parts(parts, state).await?;
        let Path(params) =
            Path::<std::collections::HashMap<String, String>>::from_request_parts(parts, state)
                .await
                .map_err(|_| {
                    ApiError::not_found("No such workspace.").with_code("workspace_not_found")
                })?;

        let id = params
            .get("workspace")
            .filter(|id| aurum_core::ids::is_uuid(id))
            .cloned()
            // Deliberately 404, not 400: whether an id names a workspace is itself information.
            .ok_or_else(|| {
                ApiError::not_found("No such workspace.").with_code("workspace_not_found")
            })?;

        let state = state.clone();

        blocking(move || {
            let db = state.db.open_control()?;

            // Deliberately 404, not 403: whether a workspace exists is something a non-member
            // must not be able to probe for.
            let missing =
                || ApiError::not_found("No such workspace.").with_code("workspace_not_found");

            let role = workspaces::role_of(&db, &caller.user.id, &id)?.ok_or_else(missing)?;
            let record = workspaces::find(&db, &id)?.ok_or_else(missing)?;

            if !P::granted(role) {
                return Err(ApiError::forbidden(format!(
                    "Your role ({}) does not permit this action.",
                    role.as_str()
                ))
                .with_code("insufficient_role"));
            }

            Ok(Workspace {
                id,
                record,
                role,
                caller,
                permission: PhantomData,
            })
        })
        .await
    }
}

/// A JSON body, rejected as a 422 rather than axum's default 400-with-prose.
pub struct Body<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> axum::extract::FromRequest<S> for Body<T> {
    type Rejection = ApiError;

    async fn from_request(request: axum::extract::Request, state: &S) -> ApiResult<Body<T>> {
        let bytes = axum::body::Bytes::from_request(request, state)
            .await
            .map_err(|_| ApiError::bad_request("Could not read the request body."))?;

        // An absent body is an empty object, so a handler whose fields are all optional does not
        // need the client to send `{}`.
        let bytes = if bytes.is_empty() { "{}".into() } else { bytes };

        serde_json::from_slice(&bytes)
            .map(Body)
            .map_err(|error| ApiError::unprocessable(format!("Malformed request body: {error}")))
    }
}

/// The half-typed shape of a sync payload, which is a bag of columns rather than a struct.
pub fn object(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// Trims, and refuses empty or over-long. The same message the PHP produced, because the client
/// shows it to a person.
pub fn require_string(
    body: &Map<String, Value>,
    key: &str,
    max_length: usize,
) -> ApiResult<String> {
    let value = body
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| ApiError::validation(key, format!("\"{key}\" is required.")))?;

    if value.chars().count() > max_length {
        return Err(ApiError::validation(
            key,
            format!("\"{key}\" must be at most {max_length} characters."),
        ));
    }

    Ok(value.to_owned())
}

pub fn optional_string(body: &Map<String, Value>, key: &str) -> Option<String> {
    body.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
