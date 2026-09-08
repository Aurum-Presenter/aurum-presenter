//! The second factor: a time-based code, and the recovery codes that stand in for one.

use base32::Alphabet;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use totp_rs::{Algorithm, TOTP};

use super::cipher::SecretCipher;
use crate::error::{ApiError, ApiResult};

pub const PERIOD: u64 = 30;
pub const DIGITS: usize = 6;
pub const RECOVERY_CODE_COUNT: usize = 10;

#[derive(Clone)]
pub struct TotpService {
    cipher: SecretCipher,
    issuer: String,
}

impl TotpService {
    pub fn new(cipher: SecretCipher, issuer: String) -> TotpService {
        TotpService { cipher, issuer }
    }

    pub fn generate_secret(&self) -> String {
        let mut bytes = [0_u8; 20];
        rand::rng().fill_bytes(&mut bytes);

        base32::encode(Alphabet::Rfc4648 { padding: false }, &bytes)
    }

    pub fn encrypt_secret(&self, secret: &str) -> ApiResult<Vec<u8>> {
        self.cipher.encrypt(secret)
    }

    pub fn provisioning_uri(&self, secret: &str, account_email: &str) -> ApiResult<String> {
        Ok(self.totp(secret, account_email)?.get_url())
    }

    /// Returns the time step the code matched, or `None`.
    ///
    /// Returning the step rather than a bare boolean is what makes replay detectable: a code is
    /// valid for thirty seconds, and within that window it must work exactly once.
    pub fn verify(
        &self,
        encrypted_secret: &[u8],
        code: &str,
        last_accepted_step: Option<i64>,
        now_seconds: i64,
    ) -> Option<i64> {
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();

        if code.len() != DIGITS || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }

        let secret = self.cipher.decrypt(encrypted_secret).ok()?;
        let totp = self.totp(&secret, "").ok()?;

        // One step either side: a phone whose clock is a few seconds out still gets in.
        for offset in [-1, 0, 1] {
            let at = now_seconds + offset * PERIOD as i64;

            if at < 0 || totp.generate(at as u64) != code {
                continue;
            }

            let step = at / PERIOD as i64;

            return match last_accepted_step {
                Some(last) if step <= last => None,
                _ => Some(step),
            };
        }

        None
    }

    fn totp(&self, secret: &str, account_email: &str) -> ApiResult<TOTP> {
        let bytes = base32::decode(Alphabet::Rfc4648 { padding: false }, secret)
            .ok_or_else(|| ApiError::internal("reading a TOTP secret", "not base32"))?;

        TOTP::new(
            Algorithm::SHA1,
            DIGITS,
            1,
            PERIOD,
            bytes,
            Some(self.issuer.clone()),
            account_email.to_owned(),
        )
        .map_err(|error| ApiError::internal("building a TOTP", error))
    }
}

/// Recovery codes: ten of them, shown once, stored only as keyed hashes.
#[derive(Clone, Debug)]
pub struct RecoveryCodes {
    signing_key: String,
}

impl RecoveryCodes {
    pub fn new(signing_key: String) -> RecoveryCodes {
        RecoveryCodes { signing_key }
    }

    pub fn generate(&self, count: usize) -> Vec<String> {
        (0..count)
            .map(|_| {
                let mut bytes = [0_u8; 5];
                rand::rng().fill_bytes(&mut bytes);

                let raw = hex::encode_upper(bytes);

                format!("{}-{}", &raw[..5], &raw[5..])
            })
            .collect()
    }

    pub fn hash(&self, code: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.signing_key.as_bytes())
            .expect("HMAC takes a key of any length");
        mac.update(self.normalise(code).as_bytes());

        hex::encode(mac.finalize().into_bytes())
    }

    /// A person reading a code off paper types the dash, or does not, or uses lower case.
    pub fn normalise(&self, code: &str) -> String {
        code.chars()
            .filter(char::is_ascii_alphanumeric)
            .flat_map(char::to_uppercase)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn service() -> TotpService {
        TotpService::new(
            SecretCipher::new(&base64::engine::general_purpose::STANDARD.encode([3_u8; 32]))
                .unwrap(),
            "Aurum Presenter".to_owned(),
        )
    }

    fn code_at(service: &TotpService, secret: &str, at: i64) -> String {
        let bytes = base32::decode(Alphabet::Rfc4648 { padding: false }, secret).unwrap();
        let totp = TOTP::new(
            Algorithm::SHA1,
            DIGITS,
            1,
            PERIOD,
            bytes,
            None,
            String::new(),
        )
        .unwrap();

        let _ = service;

        totp.generate(at as u64)
    }

    #[test]
    fn accepts_the_code_an_authenticator_would_show() {
        let service = service();
        let secret = service.generate_secret();
        let sealed = service.encrypt_secret(&secret).unwrap();
        let now = 1_757_332_200;

        let step = service.verify(&sealed, &code_at(&service, &secret, now), None, now);

        assert_eq!(step, Some(now / PERIOD as i64));
    }

    /// Business rule: a code is good for thirty seconds and works exactly once in them.
    #[test]
    fn refuses_the_same_code_a_second_time() {
        let service = service();
        let secret = service.generate_secret();
        let sealed = service.encrypt_secret(&secret).unwrap();
        let now = 1_757_332_200;
        let code = code_at(&service, &secret, now);

        let step = service
            .verify(&sealed, &code, None, now)
            .expect("accepted once");

        assert_eq!(service.verify(&sealed, &code, Some(step), now), None);
    }

    /// A phone whose clock is a few seconds out still gets in.
    #[test]
    fn allows_one_step_either_side() {
        let service = service();
        let secret = service.generate_secret();
        let sealed = service.encrypt_secret(&secret).unwrap();
        let now = 1_757_332_200;

        assert!(
            service
                .verify(&sealed, &code_at(&service, &secret, now - 30), None, now)
                .is_some()
        );
        assert!(
            service
                .verify(&sealed, &code_at(&service, &secret, now + 30), None, now)
                .is_some()
        );
        assert!(
            service
                .verify(&sealed, &code_at(&service, &secret, now - 90), None, now)
                .is_none()
        );
    }

    #[test]
    fn refuses_what_is_not_a_six_digit_code() {
        let service = service();
        let sealed = service.encrypt_secret(&service.generate_secret()).unwrap();
        let now = 1_757_332_200;

        for attempt in ["", "12345", "1234567", "abcdef", "12 34 5"] {
            assert_eq!(
                service.verify(&sealed, attempt, None, now),
                None,
                "{attempt}"
            );
        }
    }

    /// Whitespace a person pasted in is not a wrong code.
    #[test]
    fn ignores_the_spaces_an_authenticator_app_shows() {
        let service = service();
        let secret = service.generate_secret();
        let sealed = service.encrypt_secret(&secret).unwrap();
        let now = 1_757_332_200;
        let code = code_at(&service, &secret, now);
        let spaced = format!("{} {}", &code[..3], &code[3..]);

        assert!(service.verify(&sealed, &spaced, None, now).is_some());
    }

    #[test]
    fn names_the_account_and_the_issuer_in_the_provisioning_uri() {
        let service = service();
        let uri = service
            .provisioning_uri(&service.generate_secret(), "ada@example.com")
            .unwrap();

        assert!(uri.starts_with("otpauth://totp/"), "{uri}");
        assert!(uri.contains("ada%40example.com"), "{uri}");
        assert!(uri.contains("issuer=Aurum%20Presenter"), "{uri}");
    }

    #[test]
    fn recovery_codes_are_readable_off_paper_and_stored_as_hashes() {
        let codes = RecoveryCodes::new("a-key".to_owned());
        let generated = codes.generate(RECOVERY_CODE_COUNT);

        assert_eq!(generated.len(), 10);
        assert_eq!(generated[0].len(), 11, "XXXXX-XXXXX");

        let hashed = codes.hash(&generated[0]);

        assert_ne!(hashed, generated[0]);
        // However the person types it back.
        assert_eq!(codes.hash(&generated[0].to_lowercase()), hashed);
        assert_eq!(codes.hash(&generated[0].replace('-', "")), hashed);
        assert_eq!(codes.hash(&format!(" {} ", generated[0])), hashed);
        assert_ne!(codes.hash(&generated[1]), hashed);
    }
}
