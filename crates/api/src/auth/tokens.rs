//! Two token shapes, deliberately different.
//!
//! The **access token** is stateless and HMAC-signed, valid for minutes. Verifying it is a
//! constant-time comparison, not a database read, so the sync engine's high-frequency calls do
//! not each cost a row lookup.
//!
//! The **refresh token** is a long opaque random string, stored only as a keyed hash. A database
//! read therefore cannot mint a session, and rotation on every use makes a stolen cookie
//! detectable: presenting a token twice means somebody has a copy, and the whole family dies.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const ACCESS_PREFIX: &str = "v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Claims {
    pub sub: String,
    pub sid: String,
    pub exp: i64,
}

#[derive(Clone, Debug)]
pub struct Tokens {
    signing_key: String,
    access_ttl: i64,
    refresh_ttl: i64,
}

impl Tokens {
    pub fn new(signing_key: String, access_ttl: i64, refresh_ttl: i64) -> Tokens {
        Tokens {
            signing_key,
            access_ttl,
            refresh_ttl,
        }
    }

    pub fn access_ttl(&self) -> i64 {
        self.access_ttl
    }

    pub fn refresh_ttl(&self) -> i64 {
        self.refresh_ttl
    }

    pub fn issue_access_token(&self, user_id: &str, session_id: &str, now_ms: i64) -> String {
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&Claims {
                sub: user_id.to_owned(),
                sid: session_id.to_owned(),
                exp: now_ms / 1000 + self.access_ttl,
            })
            .expect("claims serialise"),
        );

        format!("{ACCESS_PREFIX}.{payload}.{}", self.sign(&payload))
    }

    /// `None` when malformed, tampered with, or expired — the caller cannot tell which, and
    /// does not need to.
    pub fn verify_access_token(&self, token: &str, now_ms: i64) -> Option<Claims> {
        let mut parts = token.split('.');

        if parts.next()? != ACCESS_PREFIX {
            return None;
        }

        let payload = parts.next()?;
        let signature = parts.next()?;

        if parts.next().is_some() || !constant_time_eq(&self.sign(payload), signature) {
            return None;
        }

        let claims: Claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;

        (claims.exp > now_ms / 1000).then_some(claims)
    }

    /// A keyed hash, not a bare one: the stored value is useless to an attacker who reads the
    /// database but not the application key.
    pub fn hash_refresh_token(&self, token: &str) -> String {
        hex::encode(self.mac(token.as_bytes()))
    }

    fn sign(&self, payload: &str) -> String {
        URL_SAFE_NO_PAD.encode(self.mac(payload.as_bytes()))
    }

    fn mac(&self, message: &[u8]) -> Vec<u8> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.signing_key.as_bytes())
            .expect("HMAC takes a key of any length");
        mac.update(message);

        mac.finalize().into_bytes().to_vec()
    }
}

pub fn random_token() -> String {
    use rand::RngCore;

    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);

    hex::encode(bytes)
}

/// Comparison that does not leak where two strings first differ.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());

    a.len() == b.len() && a.iter().zip(b).fold(0, |seen, (x, y)| seen | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_757_332_200_000;

    fn tokens() -> Tokens {
        Tokens::new("a-key".to_owned(), 900, 2_592_000)
    }

    #[test]
    fn round_trips_the_claims_it_signed() {
        let token = tokens().issue_access_token("user-1", "session-1", NOW);
        let claims = tokens().verify_access_token(&token, NOW).expect("valid");

        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.sid, "session-1");
        assert_eq!(claims.exp, NOW / 1000 + 900);
    }

    #[test]
    fn refuses_a_token_signed_with_another_key() {
        let token = tokens().issue_access_token("user-1", "session-1", NOW);
        let other = Tokens::new("a-different-key".to_owned(), 900, 2_592_000);

        assert!(other.verify_access_token(&token, NOW).is_none());
    }

    #[test]
    fn refuses_a_payload_that_was_edited() {
        let token = tokens().issue_access_token("user-1", "session-1", NOW);
        let mut parts: Vec<&str> = token.split('.').collect();
        let forged = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&Claims {
                sub: "somebody-else".to_owned(),
                sid: "session-1".to_owned(),
                exp: NOW / 1000 + 900,
            })
            .unwrap(),
        );
        parts[1] = &forged;

        assert!(
            tokens()
                .verify_access_token(&parts.join("."), NOW)
                .is_none()
        );
    }

    #[test]
    fn refuses_a_token_whose_time_has_passed() {
        let token = tokens().issue_access_token("user-1", "session-1", NOW);

        assert!(
            tokens()
                .verify_access_token(&token, NOW + 901_000)
                .is_none()
        );
        assert!(
            tokens()
                .verify_access_token(&token, NOW + 899_000)
                .is_some()
        );
    }

    #[test]
    fn refuses_what_is_not_a_token_at_all() {
        let tokens = tokens();

        assert!(tokens.verify_access_token("", NOW).is_none());
        assert!(tokens.verify_access_token("v1.only-two", NOW).is_none());
        assert!(
            tokens.verify_access_token("v2.a.b", NOW).is_none(),
            "another version"
        );
        assert!(
            tokens.verify_access_token("v1.a.b.c", NOW).is_none(),
            "a fourth part"
        );
    }

    /// The stored value has to be worthless on its own.
    #[test]
    fn hashes_a_refresh_token_with_the_application_key() {
        let token = random_token();
        let hashed = tokens().hash_refresh_token(&token);

        assert_ne!(hashed, token);
        assert_eq!(hashed.len(), 64);
        assert_eq!(hashed, tokens().hash_refresh_token(&token), "stable");
        assert_ne!(
            hashed,
            Tokens::new("other".to_owned(), 900, 1).hash_refresh_token(&token),
            "keyed"
        );
    }

    #[test]
    fn two_refresh_tokens_are_never_the_same() {
        assert_ne!(random_token(), random_token());
        assert_eq!(random_token().len(), 64);
    }
}
