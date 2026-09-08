//! Session rotation, and the things a stolen cookie must not buy.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{refresh_cookie, server};

#[tokio::test]
async fn a_refresh_rotates_the_cookie_and_keeps_the_session_working() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    let refreshed = server.refresh(&ada.refresh).await;

    assert_eq!(refreshed.status, StatusCode::OK, "{:?}", refreshed.body);

    let rotated = refresh_cookie(&refreshed.cookies).expect("a new cookie");

    assert_ne!(rotated, ada.refresh, "the token is single-use");

    let token = refreshed.body["access_token"].as_str().expect("a token");

    assert_eq!(
        server.get("/api/v1/account", token).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_replaced_session_cannot_be_refreshed() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    server.refresh(&ada.refresh).await;

    let replayed = server.refresh(&ada.refresh).await;

    assert_eq!(replayed.status, StatusCode::UNAUTHORIZED);
    assert_eq!(replayed.code(), "refresh_token_reused");
}

/// Presenting a token twice means somebody has a copy, so the whole rotating chain dies — both
/// the legitimate user and whoever stole the cookie must sign in again.
#[tokio::test]
async fn reuse_detection_kills_the_whole_chain_immediately() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    let honest = refresh_cookie(&server.refresh(&ada.refresh).await.cookies).expect("a cookie");

    // The thief replays the one they copied.
    server.refresh(&ada.refresh).await;

    // The honest device's current cookie is dead too.
    let after = server.refresh(&honest).await;

    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    assert!(
        ["refresh_token_reused", "session_expired"].contains(&after.code()),
        "got {}",
        after.code()
    );
}

/// A session that was replaced by rotation is revoked but not dead: the access token it issued
/// has minutes left, and killing it the instant the client rotates would sign people out
/// mid-request.
#[tokio::test]
async fn a_window_keeps_working_after_another_window_refreshes() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    server.refresh(&ada.refresh).await;

    assert_eq!(
        server.get("/api/v1/account", &ada.token).await.status,
        StatusCode::OK,
        "the older access token still works"
    );
}

/// A logout is not a rotation. It revokes without a replacement, so the access token stops at
/// once rather than living out its fifteen minutes.
#[tokio::test]
async fn a_sign_out_stops_the_access_token_at_once() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    assert_eq!(
        server
            .post("/api/v1/auth/logout", &ada.token, json!({}))
            .await
            .status,
        StatusCode::OK
    );

    let after = server.get("/api/v1/account", &ada.token).await;

    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    assert_eq!(after.code(), "session_revoked");
}

#[tokio::test]
async fn a_missing_or_forged_token_is_told_which_it_is() {
    let server = server().await;

    let missing = server.request("GET", "/api/v1/account", None, None).await;

    assert_eq!(missing.status, StatusCode::UNAUTHORIZED);
    assert_eq!(missing.code(), "missing_token");

    // Distinguished so the client knows to attempt a refresh rather than send the user back to
    // the sign-in screen.
    let forged = server.get("/api/v1/account", "v1.bm90.aGVyZQ").await;

    assert_eq!(forged.code(), "token_expired");
}

#[tokio::test]
async fn registering_twice_with_the_same_address_is_refused() {
    let server = server().await;
    server.account("ada@example.com").await;

    let again = server
        .request(
            "POST",
            "/api/v1/auth/register",
            None,
            Some(json!({
                "email": "ada@example.com",
                "display_name": "Ada",
                "password": "correct horse battery staple",
            })),
        )
        .await;

    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.code(), "email_taken");
}

#[tokio::test]
async fn a_password_shorter_than_the_minimum_is_refused() {
    let server = server().await;

    let answer = server
        .request(
            "POST",
            "/api/v1/auth/register",
            None,
            Some(json!({ "email": "ada@example.com", "display_name": "Ada", "password": "short" })),
        )
        .await;

    assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(answer.body["error"]["details"]["field"], "password");
}

/// The same message either way, so response content does not disclose whether an address has an
/// account.
#[tokio::test]
async fn a_wrong_password_and_an_unknown_address_answer_the_same() {
    let server = server().await;
    server.account("ada@example.com").await;

    let wrong = server
        .request(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": "ada@example.com", "password": "not the password" })),
        )
        .await;
    let unknown = server
        .request(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": "nobody@example.com", "password": "not the password" })),
        )
        .await;

    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.code(), "invalid_credentials");
    assert_eq!(
        wrong.body["error"]["message"],
        unknown.body["error"]["message"]
    );
    assert_eq!(wrong.code(), unknown.code());
}

/// A correct password alone never returns a session once a second factor is enrolled.
#[tokio::test]
async fn a_second_factor_turns_a_sign_in_into_a_challenge() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    let enrolled = server
        .post("/api/v1/account/totp/enrol", &ada.token, json!({}))
        .await;
    let secret = enrolled.body["secret"]
        .as_str()
        .expect("a secret")
        .to_owned();

    let confirmed = server
        .post(
            "/api/v1/account/totp/confirm",
            &ada.token,
            json!({ "code": code_for(&secret) }),
        )
        .await;

    assert_eq!(confirmed.status, StatusCode::OK, "{:?}", confirmed.body);
    assert_eq!(
        confirmed.body["recovery_codes"].as_array().unwrap().len(),
        10
    );

    let signed_in = server
        .request(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({
                "email": "ada@example.com",
                "password": "correct horse battery staple",
            })),
        )
        .await;

    assert_eq!(signed_in.body["totp_required"], true);
    assert!(signed_in.body["access_token"].is_null(), "no session yet");
    assert!(signed_in.body["challenge_id"].is_string());
}

/// A recovery code stands in for the authenticator, exactly once.
#[tokio::test]
async fn a_recovery_code_answers_the_challenge_and_then_stops_working() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    let secret = server
        .post("/api/v1/account/totp/enrol", &ada.token, json!({}))
        .await
        .body["secret"]
        .as_str()
        .expect("a secret")
        .to_owned();
    let codes = server
        .post(
            "/api/v1/account/totp/confirm",
            &ada.token,
            json!({ "code": code_for(&secret) }),
        )
        .await
        .body["recovery_codes"]
        .clone();
    let code = codes[0].as_str().expect("a code").to_owned();

    for (attempt, expected) in [(1, StatusCode::OK), (2, StatusCode::UNAUTHORIZED)] {
        let challenge = server
            .request(
                "POST",
                "/api/v1/auth/login",
                None,
                Some(json!({
                    "email": "ada@example.com",
                    "password": "correct horse battery staple",
                })),
            )
            .await;

        let answered = server
            .request(
                "POST",
                "/api/v1/auth/login/totp",
                None,
                Some(json!({
                    "challenge_id": challenge.body["challenge_id"],
                    "code": code,
                })),
            )
            .await;

        assert_eq!(
            answered.status, expected,
            "attempt {attempt}: {:?}",
            answered.body
        );

        if attempt == 1 {
            assert_eq!(answered.body["recovery_codes_remaining"], 9);
        }
    }
}

/// The authenticator's side, so the test can be the phone.
fn code_for(secret: &str) -> String {
    use totp_rs::{Algorithm, TOTP};

    let bytes = base32::decode(base32::Alphabet::Rfc4648 { padding: false }, secret)
        .expect("a base32 secret");

    TOTP::new(Algorithm::SHA1, 6, 1, 30, bytes, None, String::new())
        .expect("a totp")
        .generate_current()
        .expect("a code")
}
