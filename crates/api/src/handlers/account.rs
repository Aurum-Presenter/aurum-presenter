//! The signed-in account: who it is, and its second factor.

use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};

use crate::db::{blocking, now_ms};
use crate::error::{ApiError, ApiResult};
use crate::extract::{Body, Caller, object, require_string};
use crate::repo::{accounts, workspaces};
use crate::state::AppState;

pub async fn me(State(state): State<AppState>, caller: Caller) -> ApiResult<Json<Value>> {
    blocking(move || {
        let db = state.db.open_control()?;
        let id = &caller.user.id;

        Ok(Json(json!({
            "id": id,
            "email": caller.user.email,
            "display_name": caller.user.display_name,
            "totp": {
                "enrolled": accounts::confirmed_totp(&db, id)?.is_some(),
                "required": super::auth::requires_totp_for(&db, id)?,
                "recovery_codes_remaining": accounts::count_unused_recovery_codes(&db, id)?,
            },
            "workspaces": workspaces::for_user(&db, id)?,
        })))
    })
    .await
}

pub async fn totp_enrol(State(state): State<AppState>, caller: Caller) -> ApiResult<Json<Value>> {
    let secret = state.totp.generate_secret();
    let uri = state.totp.provisioning_uri(&secret, &caller.user.email)?;
    let encrypted = state.totp.encrypt_secret(&secret)?;

    let staged = secret.clone();

    blocking(move || {
        accounts::stage_totp_secret(&state.db.open_control()?, &caller.user.id, &encrypted)
    })
    .await?;

    Ok(Json(json!({
        "secret": staged,
        "provisioning_uri": uri,
        "digits": crate::auth::totp::DIGITS,
        "period": crate::auth::totp::PERIOD,
    })))
}

pub async fn totp_confirm(
    State(state): State<AppState>,
    caller: Caller,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let code = require_string(&object(&body), "code", 32)?;

    blocking(move || {
        let mut db = state.db.open_control()?;
        let id = &caller.user.id;

        let pending = accounts::pending_totp(&db, id)?.ok_or_else(|| {
            ApiError::conflict("There is no enrolment in progress.").with_code("no_enrolment")
        })?;

        if pending.confirmed_at.is_some() {
            return Err(
                ApiError::conflict("Two-factor is already enabled.").with_code("already_enrolled")
            );
        }

        let step = state
            .totp
            .verify(&pending.secret_encrypted, &code, None, now_ms() / 1000)
            .ok_or_else(|| {
                ApiError::unprocessable("That code is not valid.").with_code("invalid_totp")
            })?;

        accounts::confirm_totp(&db, id, step)?;

        // Shown exactly once. Stored only as hashes, so they cannot be re-displayed later.
        let codes = state
            .recovery_codes
            .generate(crate::auth::totp::RECOVERY_CODE_COUNT);
        let hashes: Vec<String> = codes
            .iter()
            .map(|code| state.recovery_codes.hash(code))
            .collect();

        accounts::replace_recovery_codes(&mut db, id, &hashes)?;

        Ok(Json(json!({
            "enrolled": true,
            "recovery_codes": codes,
            "notice": "These codes are shown once. Store them somewhere safe.",
        })))
    })
    .await
}

/// Turning the second factor off needs both the password and a current code: the two things an
/// attacker who has taken over a live session is least likely to hold.
pub async fn totp_disable(
    State(state): State<AppState>,
    caller: Caller,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let body = object(&body);
    let password = require_string(&body, "password", 512)?;
    let code = require_string(&body, "code", 32)?;

    blocking(move || {
        let db = state.db.open_control()?;
        let id = &caller.user.id;

        if super::auth::requires_totp_for(&db, id)? {
            return Err(ApiError::forbidden(
                "You own a workspace, so two-factor cannot be turned off. Transfer ownership first.",
            )
            .with_code("totp_required_for_owner"));
        }

        verify_password(&state, &db, id, &password)?;

        let secret = accounts::confirmed_totp(&db, id)?.ok_or_else(|| {
            ApiError::conflict("Two-factor is not enabled.").with_code("not_enrolled")
        })?;

        state
            .totp
            .verify(
                &secret.secret_encrypted,
                &code,
                secret.last_accepted_step,
                now_ms() / 1000,
            )
            .ok_or_else(|| {
                ApiError::unprocessable("That code is not valid.").with_code("invalid_totp")
            })?;

        accounts::remove_totp(&db, id)?;

        Ok(Json(json!({ "enrolled": false })))
    })
    .await
}

/// New recovery codes replace the old ones outright: a set that was written down and lost is a
/// set that has to stop working.
pub async fn recovery_codes(
    State(state): State<AppState>,
    caller: Caller,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let body = object(&body);
    let password = require_string(&body, "password", 512)?;
    let code = require_string(&body, "code", 32)?;

    blocking(move || {
        let mut db = state.db.open_control()?;
        let id = &caller.user.id;

        verify_password(&state, &db, id, &password)?;

        let secret = accounts::confirmed_totp(&db, id)?.ok_or_else(|| {
            ApiError::conflict("This account does not have two-factor authentication.")
                .with_code("totp_not_enrolled")
        })?;

        let step = state
            .totp
            .verify(
                &secret.secret_encrypted,
                &code,
                secret.last_accepted_step,
                now_ms() / 1000,
            )
            .ok_or_else(|| {
                ApiError::unauthorized("That code is not right.").with_code("invalid_code")
            })?;

        accounts::record_totp_step(&db, id, step)?;

        let codes = state
            .recovery_codes
            .generate(crate::auth::totp::RECOVERY_CODE_COUNT);
        let hashes: Vec<String> = codes
            .iter()
            .map(|code| state.recovery_codes.hash(code))
            .collect();

        accounts::replace_recovery_codes(&mut db, id, &hashes)?;

        Ok(Json(json!({ "recovery_codes": codes })))
    })
    .await
}

fn verify_password(
    state: &AppState,
    db: &rusqlite::Connection,
    user_id: &str,
    password: &str,
) -> ApiResult<()> {
    let hash = accounts::password_hash(db, user_id)?;

    match hash {
        Some(hash) if state.passwords.verify(password, &hash) => Ok(()),
        _ => {
            Err(ApiError::unauthorized("That password is not right.")
                .with_code("invalid_credentials"))
        }
    }
}
