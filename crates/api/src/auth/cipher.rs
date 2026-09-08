//! AES-256-GCM for the TOTP shared secret.
//!
//! Authenticated encryption matters here: a tampered secret must fail loudly rather than quietly
//! start accepting a different authenticator's codes.

use aes_gcm::aead::{Aead, KeyInit, OsRng, rand_core::RngCore};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;

use crate::error::{ApiError, ApiResult};

#[derive(Clone)]
pub struct SecretCipher {
    cipher: Aes256Gcm,
}

impl SecretCipher {
    pub fn new(base64_key: &str) -> Result<SecretCipher, String> {
        let key = base64::engine::general_purpose::STANDARD
            .decode(base64_key)
            .map_err(|_| "APP_SECRET_KEY is not valid base64.".to_owned())?;

        if key.len() != 32 {
            return Err("APP_SECRET_KEY must be 32 bytes, base64-encoded. \
                        Generate one with: openssl rand -base64 32"
                .to_owned());
        }

        Ok(SecretCipher {
            cipher: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key)),
        })
    }

    /// Nonce, then tag, then ciphertext — the same layout the PHP wrote, because the same bytes
    /// have to decrypt whichever half of the migration is running.
    pub fn encrypt(&self, plaintext: &str) -> ApiResult<Vec<u8>> {
        let mut nonce = [0_u8; 12];
        OsRng.fill_bytes(&mut nonce);

        let sealed = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
            .map_err(|error| ApiError::internal("encrypting a secret", error))?;

        // aes-gcm appends the tag; the stored layout puts it in front of the ciphertext.
        let (ciphertext, tag) = sealed.split_at(sealed.len() - 16);

        Ok([&nonce[..], tag, ciphertext].concat())
    }

    pub fn decrypt(&self, payload: &[u8]) -> ApiResult<String> {
        if payload.len() < 29 {
            return Err(ApiError::internal(
                "decrypting a secret",
                "payload is truncated",
            ));
        }

        let (nonce, rest) = payload.split_at(12);
        let (tag, ciphertext) = rest.split_at(16);

        let plaintext = self
            .cipher
            .decrypt(
                Nonce::from_slice(nonce),
                [ciphertext, tag].concat().as_slice(),
            )
            .map_err(|_| {
                ApiError::internal("decrypting a secret", "wrong key or tampered payload")
            })?;

        String::from_utf8(plaintext)
            .map_err(|error| ApiError::internal("decrypting a secret", error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher() -> SecretCipher {
        SecretCipher::new(&base64::engine::general_purpose::STANDARD.encode([7_u8; 32])).unwrap()
    }

    #[test]
    fn round_trips_a_secret() {
        let sealed = cipher().encrypt("JBSWY3DPEHPK3PXP").unwrap();

        assert_ne!(sealed, b"JBSWY3DPEHPK3PXP");
        assert_eq!(cipher().decrypt(&sealed).unwrap(), "JBSWY3DPEHPK3PXP");
    }

    /// Encrypting twice must not produce the same bytes, or the database leaks which two users
    /// share a secret.
    #[test]
    fn never_writes_the_same_ciphertext_twice() {
        assert_ne!(
            cipher().encrypt("same").unwrap(),
            cipher().encrypt("same").unwrap()
        );
    }

    /// The point of an authenticated cipher: a flipped bit is an error, not a different secret.
    #[test]
    fn refuses_a_payload_somebody_edited() {
        let mut sealed = cipher().encrypt("JBSWY3DPEHPK3PXP").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;

        assert!(cipher().decrypt(&sealed).is_err());
        assert!(cipher().decrypt(&[0; 8]).is_err(), "truncated");
    }

    #[test]
    fn refuses_a_secret_sealed_with_another_key() {
        let sealed = cipher().encrypt("JBSWY3DPEHPK3PXP").unwrap();
        let other =
            SecretCipher::new(&base64::engine::general_purpose::STANDARD.encode([9_u8; 32]))
                .unwrap();

        assert!(other.decrypt(&sealed).is_err());
    }

    #[test]
    fn refuses_a_key_that_is_not_one() {
        assert!(SecretCipher::new("not base64!").is_err());
        assert!(
            SecretCipher::new(&base64::engine::general_purpose::STANDARD.encode([1_u8; 16]))
                .is_err()
        );
    }
}
