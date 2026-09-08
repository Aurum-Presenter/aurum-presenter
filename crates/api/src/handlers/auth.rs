//! Registration, sign-in, the second factor, and the rotating refresh cookie.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use rusqlite::Connection;
use serde_json::{Value, json};

use crate::db::{blocking, now_ms};
use crate::error::{ApiError, ApiResult};
use crate::extract::{Body, Caller, object, require_string};
use crate::repo::{accounts, invites, sessions, workspaces};
use crate::state::AppState;

const CHALLENGE_TTL: i64 = 300;
const MAX_TOTP_ATTEMPTS: i64 = 5;
const RESET_TTL: i64 = 3600;
pub const REFRESH_COOKIE: &str = "aurum_refresh";

/// The token pair, and the cookie that carries half of it.
///
/// The split matters: the access token goes in the JSON body because the sync engine sends it as
/// an `Authorization` header from a Web Worker, while the refresh token goes in an httpOnly
/// cookie where no script — including a successful XSS payload — can read it.
struct Issued {
    body: Value,
    cookie: Cookie<'static>,
    session_id: String,
}

fn issue(
    state: &AppState,
    db: &Connection,
    user_id: &str,
    user_agent: Option<&str>,
    family_id: Option<String>,
) -> ApiResult<Issued> {
    let refresh = crate::auth::tokens::random_token();
    let (session_id, _) = sessions::open(
        db,
        user_id,
        &state.tokens.hash_refresh_token(&refresh),
        &crate::repo::in_seconds(state.tokens.refresh_ttl()),
        user_agent,
        family_id,
    )?;

    Ok(Issued {
        body: json!({
            "access_token": state.tokens.issue_access_token(user_id, &session_id, now_ms()),
            "token_type": "Bearer",
            "expires_in": state.tokens.access_ttl(),
        }),
        cookie: refresh_cookie(state, refresh, state.tokens.refresh_ttl()),
        session_id,
    })
}

fn refresh_cookie(state: &AppState, value: String, max_age: i64) -> Cookie<'static> {
    Cookie::build((REFRESH_COOKIE, value))
        .path(state.config.auth.cookie_path.clone())
        .max_age(time::Duration::seconds(max_age))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.config.auth.cookie_secure)
        .build()
}

fn respond(jar: CookieJar, issued: Issued, status: StatusCode, extra: Value) -> Response {
    let mut body = issued.body;

    if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        body.extend(extra.clone());
    }

    (status, jar.add(issued.cookie), Json(body)).into_response()
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get("user-agent")?
        .to_str()
        .ok()
        .map(str::to_owned)
        .filter(|value| !value.is_empty())
}

/// The address the request came from, as the proxy in front of us reports it.
fn client_ip(headers: &HeaderMap) -> Option<String> {
    let forwarded = headers.get("x-forwarded-for")?.to_str().ok()?;

    forwarded
        .split(',')
        .next()
        .map(str::trim)
        .filter(|ip| !ip.is_empty())
        .map(str::to_owned)
}

// -- Registration ----------------------------------------------------------------------------

pub async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Body(body): Body<Value>,
) -> ApiResult<Response> {
    let body = object(&body);
    let email = require_string(&body, "email", 320)?.to_lowercase();
    let display_name = require_string(&body, "display_name", 120)?;
    let password = require_string(&body, "password", 512)?;

    if !looks_like_an_email(&email) {
        return Err(ApiError::validation(
            "email",
            "That does not look like an email address.",
        ));
    }

    assert_password_acceptable(&state, &password)?;

    let agent = user_agent(&headers);
    let hash = state.passwords.hash(&password)?;

    let (issued, jar) = blocking(move || {
        let mut db = state.db.open_control()?;

        if accounts::find_by_email(&db, &email)?.is_some() {
            // One of the few places where disclosing existence is unavoidable, since the account
            // cannot be created either way. It is rate limited instead.
            return Err(
                ApiError::conflict("An account with that email already exists.")
                    .with_code("email_taken"),
            );
        }

        let user_id = accounts::create(&mut db, &email, &display_name, &hash)?;

        // Business rule 1: exactly one personal workspace, created on first sign-in and not
        // deletable. Creating the database file here means a brand-new account can write
        // offline immediately after its first sync.
        let workspace = workspaces::create(&mut db, "My songs", "personal", &user_id)?;
        state.db.open_workspace(&workspace.id)?;

        let issued = issue(&state, &db, &user_id, agent.as_deref(), None)?;

        Ok((issued, jar))
    })
    .await?;

    Ok(respond(jar, issued, StatusCode::CREATED, json!({})))
}

/// A correct password alone never returns a session when a second factor is enrolled — it
/// returns a challenge. That is the property acceptance criterion 1 of the auth change request
/// tests for.
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Body(body): Body<Value>,
) -> ApiResult<Response> {
    let body = object(&body);
    let email = require_string(&body, "email", 320)?.to_lowercase();
    let password = require_string(&body, "password", 512)?;
    let ip = client_ip(&headers);
    let agent = user_agent(&headers);

    let user = authenticate(&state, &email, &password, ip.as_deref()).await?;
    let user_id = user.id.clone();

    let (issued, extra, jar) = blocking(move || {
        let db = state.db.open_control()?;

        if accounts::confirmed_totp(&db, &user_id)?.is_some() {
            return Ok((
                None,
                json!({
                    "totp_required": true,
                    "challenge_id": accounts::create_challenge(&db, &user_id, CHALLENGE_TTL)?,
                    "expires_in": CHALLENGE_TTL,
                }),
                jar,
            ));
        }

        // An owner without a second factor is not refused — they are routed to enrolment, which
        // needs a session to complete. The session is issued with that one purpose, and the
        // client is told to go nowhere else first.
        let extra = if requires_totp_for(&db, &user_id)? {
            json!({ "totp_enrolment_required": true })
        } else {
            json!({})
        };

        Ok((
            Some(issue(&state, &db, &user_id, agent.as_deref(), None)?),
            extra,
            jar,
        ))
    })
    .await?;

    Ok(match issued {
        Some(issued) => respond(jar, issued, StatusCode::OK, extra),
        None => (StatusCode::OK, Json(extra)).into_response(),
    })
}

/// The same message and the same timing whether or not the account exists.
async fn authenticate(
    state: &AppState,
    email: &str,
    password: &str,
    ip: Option<&str>,
) -> ApiResult<accounts::User> {
    let (email, password) = (email.to_owned(), password.to_owned());
    let ip = ip.map(str::to_owned);
    let lockout = state.config.auth.lockout_threshold;
    let window = state.config.auth.lockout_window;

    let (user, stored) = {
        let state = state.clone();
        let (email, ip) = (email.clone(), ip.clone());

        blocking(move || {
            let db = state.db.open_control()?;
            let failures = accounts::recent_failures(&db, &email, ip.as_deref(), window)?;

            if failures >= lockout * 4 {
                return Err(ApiError::too_many_requests(
                    "Too many attempts. Try again shortly.",
                    60,
                ));
            }

            let user = accounts::find_by_email(&db, &email)?;
            let stored = match &user {
                Some(user) => accounts::password_hash(&db, &user.id)?,
                None => None,
            };

            Ok((user, (failures, stored)))
        })
        .await?
    };

    let (failures, stored) = stored;

    // Escalating delay rather than a hard lock, so an attacker cannot lock a known account out
    // of its own sign-in by failing on purpose.
    if failures >= lockout {
        let delay = 2_u64.pow((failures - lockout).min(3) as u32).min(8);
        tokio::time::sleep(std::time::Duration::from_millis(delay * 250)).await;
    }

    // Always run a verify, even with no account, so response time does not disclose whether the
    // email is registered.
    let reference = stored.clone().unwrap_or_else(|| {
        "$argon2id$v=19$m=65536,t=4,p=1$SFdrTUxlVFJqRG5wY0dGSw$\
         0000000000000000000000000000000000000000000"
            .to_owned()
    });
    let valid = state.passwords.verify(&password, &reference) && user.is_some();

    let Some(user) = user.filter(|_| valid) else {
        let state = state.clone();
        let (email, ip) = (email.clone(), ip.clone());

        blocking(move || {
            accounts::record_login_attempt(&state.db.open_control()?, &email, ip.as_deref(), false)
        })
        .await?;

        return Err(ApiError::unauthorized("Email or password is incorrect.")
            .with_code("invalid_credentials"));
    };

    // The only moment the plaintext exists under the current policy.
    let rehashed = match &stored {
        Some(hash) if state.passwords.needs_rehash(hash) => Some(state.passwords.hash(&password)?),
        _ => None,
    };

    let state = state.clone();
    let id = user.id.clone();

    blocking(move || {
        let db = state.db.open_control()?;

        if let Some(hash) = rehashed {
            accounts::update_password_hash(&db, &id, &hash)?;
        }

        accounts::record_login_attempt(&db, &email, ip.as_deref(), true)?;
        accounts::clear_failures(&db, &email)
    })
    .await?;

    Ok(user)
}

/// TOTP is mandatory for owners: an owner can remove members, transfer ownership and delete the
/// whole library, so the account that can do that carries a second factor.
pub fn requires_totp_for(db: &Connection, user_id: &str) -> ApiResult<bool> {
    Ok(workspaces::for_user(db, user_id)?.iter().any(|membership| {
        membership.role == workspaces::Role::Owner && membership.workspace.kind != "personal"
    }))
}

/// Exchanges a pending-2FA challenge for a session, with a TOTP code or a recovery code.
pub async fn login_totp(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Body(body): Body<Value>,
) -> ApiResult<Response> {
    let body = object(&body);
    let challenge_id = require_string(&body, "challenge_id", 64)?;
    let code = require_string(&body, "code", 32)?;
    let agent = user_agent(&headers);

    let (issued, remaining, jar) = blocking(move || {
        let db = state.db.open_control()?;

        let challenge = accounts::find_challenge(&db, &challenge_id)?
            .filter(|challenge| challenge.expires_at > crate::db::now())
            .ok_or_else(|| {
                ApiError::unauthorized("This sign-in attempt has expired. Start again.")
                    .with_code("challenge_expired")
            })?;

        if challenge.attempts >= MAX_TOTP_ATTEMPTS {
            accounts::delete_challenge(&db, &challenge_id)?;

            return Err(
                ApiError::unauthorized("Too many incorrect codes. Start again.")
                    .with_code("challenge_exhausted"),
            );
        }

        let user_id = challenge.user_id;
        let secret = accounts::confirmed_totp(&db, &user_id)?.ok_or_else(|| {
            ApiError::unauthorized("No second factor is enrolled.").with_code("totp_not_enrolled")
        })?;

        match state.totp.verify(
            &secret.secret_encrypted,
            &code,
            secret.last_accepted_step,
            now_ms() / 1000,
        ) {
            Some(step) => accounts::record_totp_step(&db, &user_id, step)?,
            None => {
                let hashed = state.recovery_codes.hash(&code);

                if !accounts::consume_recovery_code(&db, &user_id, &hashed)? {
                    accounts::count_challenge_attempt(&db, &challenge_id)?;

                    return Err(
                        ApiError::unauthorized("That code is not valid.").with_code("invalid_totp")
                    );
                }
            }
        }

        accounts::delete_challenge(&db, &challenge_id)?;

        let issued = issue(&state, &db, &user_id, agent.as_deref(), None)?;
        let remaining = accounts::count_unused_recovery_codes(&db, &user_id)?;

        Ok((issued, remaining, jar))
    })
    .await?;

    Ok(respond(
        jar,
        issued,
        StatusCode::OK,
        json!({ "recovery_codes_remaining": remaining }),
    ))
}

/// Rotating refresh with reuse detection.
///
/// The token is single-use. If one is presented that has already been rotated away, the only
/// safe conclusion is that a copy is circulating — so every session in that rotation family is
/// revoked, and both the legitimate user and whoever stole the cookie must sign in again.
pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let token = jar
        .get(REFRESH_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::unauthorized("No refresh token.").with_code("missing_refresh_token")
        })?;
    let agent = user_agent(&headers);

    let (issued, jar) = blocking(move || {
        let db = state.db.open_control()?;

        let session =
            sessions::find_by_refresh_hash(&db, &state.tokens.hash_refresh_token(&token))?
                .ok_or_else(|| {
                    ApiError::unauthorized("Refresh token is not recognised.")
                        .with_code("invalid_refresh_token")
                })?;

        if session.revoked_at.is_some() {
            sessions::revoke_family(&db, &session.family_id)?;

            return Err(ApiError::unauthorized(
                "This session was already used and has been revoked everywhere.",
            )
            .with_code("refresh_token_reused"));
        }

        if !session.is_usable() {
            return Err(
                ApiError::unauthorized("This session has expired.").with_code("session_expired")
            );
        }

        let issued = issue(
            &state,
            &db,
            &session.user_id,
            agent.as_deref(),
            Some(session.family_id.clone()),
        )?;

        sessions::mark_replaced(&db, &session.id, &issued.session_id)?;

        Ok((issued, jar))
    })
    .await?;

    Ok(respond(jar, issued, StatusCode::OK, json!({})))
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    caller: Caller,
) -> ApiResult<Response> {
    let session_id = caller.session_id;
    let cleared = refresh_cookie(&state, String::new(), 0);

    blocking(move || sessions::revoke(&state.db.open_control()?, &session_id)).await?;

    // Nothing local is touched here. What the device keeps in IndexedDB after sign-out is the
    // client's decision, per the workspaces feature's sign-out rule.
    Ok((jar.add(cleared), Json(json!({ "signed_out": true }))).into_response())
}

// -- Forgotten passwords -----------------------------------------------------------------------

/// Always answers the same way. Whether an address has an account is not something an
/// unauthenticated caller gets to learn by asking.
pub async fn forgot_password(
    State(state): State<AppState>,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let email = require_string(&object(&body), "email", 320)?.to_lowercase();
    let app_url = std::env::var("APP_URL").unwrap_or_else(|_| "http://localhost:5173".to_owned());

    blocking(move || {
        let db = state.db.open_control()?;

        let Some(user) = accounts::find_by_email(&db, &email)? else {
            return Ok(());
        };

        let token = crate::auth::tokens::random_token();

        accounts::create_password_reset(
            &db,
            &user.id,
            &state.tokens.hash_refresh_token(&token),
            RESET_TTL,
        )?;

        let link = format!("{}/auth/reset/{token}", app_url.trim_end_matches('/'));

        invites::enqueue_mail(
            &db,
            &email,
            "Reset your Aurum password",
            &format!(
                "<p>Somebody asked to reset the password for this account.</p>\
                 <p><a href=\"{link}\">Choose a new password</a></p>\
                 <p>The link works once and expires in an hour. \
                 If this was not you, nothing has changed.</p>"
            ),
            &format!(
                "Somebody asked to reset the password for this account.\n\n{link}\n\n\
                 The link works once and expires in an hour. \
                 If this was not you, nothing has changed.\n"
            ),
        )?;

        Ok(())
    })
    .await?;

    Ok(Json(json!({ "sent": true })))
}

pub async fn reset_password(
    State(state): State<AppState>,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let body = object(&body);
    let token = require_string(&body, "token", 128)?;
    let password = require_string(&body, "password", 512)?;

    assert_password_acceptable(&state, &password)?;

    let hash = state.passwords.hash(&password)?;

    blocking(move || {
        let db = state.db.open_control()?;
        let hashed_token = state.tokens.hash_refresh_token(&token);

        let user_id = accounts::consume_password_reset(&db, &hashed_token)?.ok_or_else(|| {
            ApiError::conflict(
                "That reset link has expired or has already been used. Ask for another.",
            )
            .with_code("reset_invalid")
        })?;

        accounts::update_password_hash(&db, &user_id, &hash)?;

        // Whoever knew the old password is signed out everywhere: that is the point of a reset.
        sessions::revoke_all_for_user(&db, &user_id)
    })
    .await?;

    Ok(Json(json!({ "reset": true })))
}

fn assert_password_acceptable(state: &AppState, password: &str) -> ApiResult<()> {
    let minimum = state.config.auth.min_password_length;

    if password.chars().count() < minimum {
        return Err(ApiError::validation(
            "password",
            format!("Password must be at least {minimum} characters."),
        ));
    }

    Ok(())
}

/// Not RFC 5322 — an address with one `@`, something either side, and a dot in the domain. The
/// authoritative check is whether the mail arrives.
pub fn looks_like_an_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };

    !local.is_empty()
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain.contains('.')
        && !domain.contains('@')
        && !email.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_an_address_and_rejects_what_is_not_one() {
        assert!(looks_like_an_email("ada@example.com"));
        assert!(looks_like_an_email("ada+bands@sub.example.co.uk"));
        assert!(!looks_like_an_email("ada"));
        assert!(!looks_like_an_email("ada@example"));
        assert!(!looks_like_an_email("@example.com"));
        assert!(!looks_like_an_email("ada@@example.com"));
        assert!(!looks_like_an_email("ada @example.com"));
        assert!(!looks_like_an_email("ada@.com"));
    }
}
