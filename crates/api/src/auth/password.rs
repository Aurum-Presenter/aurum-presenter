//! Argon2id, with the cost parameters the deployment chose.

use argon2::password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

use crate::config::Argon2Params;
use crate::error::{ApiError, ApiResult};

#[derive(Clone, Debug)]
pub struct PasswordHasher {
    params: Argon2Params,
}

impl PasswordHasher {
    pub fn new(params: Argon2Params) -> PasswordHasher {
        PasswordHasher { params }
    }

    pub fn hash(&self, password: &str) -> ApiResult<String> {
        let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);

        Ok(self
            .argon2()?
            .hash_password(password.as_bytes(), &salt)
            .map_err(|error| ApiError::internal("hashing a password", error))?
            .to_string())
    }

    /// A hash we cannot parse is a failed verification, never an error: an account row that was
    /// corrupted must not become a way in.
    pub fn verify(&self, password: &str, hash: &str) -> bool {
        PasswordHash::new(hash).is_ok_and(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
    }

    /// True when the stored hash was made with weaker parameters than the ones now configured,
    /// so a correct sign-in can quietly upgrade it.
    pub fn needs_rehash(&self, hash: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(hash) else {
            return true;
        };

        if parsed.algorithm.as_str() != "argon2id" {
            return true;
        }

        let Ok(params) = Params::try_from(&parsed) else {
            return true;
        };

        params.m_cost() < self.params.memory_cost
            || params.t_cost() < self.params.time_cost
            || params.p_cost() < self.params.threads
    }

    fn argon2(&self) -> ApiResult<Argon2<'static>> {
        let params = Params::new(
            self.params.memory_cost,
            self.params.time_cost,
            self.params.threads,
            None,
        )
        .map_err(|error| ApiError::internal("argon2 parameters", error))?;

        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deliberately cheap: these are tests, not a login.
    fn hasher() -> PasswordHasher {
        PasswordHasher::new(Argon2Params {
            memory_cost: 8,
            time_cost: 1,
            threads: 1,
        })
    }

    #[test]
    fn accepts_the_password_it_hashed_and_nothing_else() {
        let hash = hasher().hash("correct horse battery staple").unwrap();

        assert!(hasher().verify("correct horse battery staple", &hash));
        assert!(!hasher().verify("Correct horse battery staple", &hash));
        assert!(!hasher().verify("", &hash));
    }

    #[test]
    fn salts_every_hash() {
        assert_ne!(
            hasher().hash("same password").unwrap(),
            hasher().hash("same password").unwrap()
        );
    }

    /// A row somebody corrupted is a failed sign-in, not a way in and not a crash.
    #[test]
    fn treats_an_unreadable_hash_as_a_failure() {
        assert!(!hasher().verify("anything", ""));
        assert!(!hasher().verify("anything", "not-a-hash"));
        assert!(!hasher().verify("anything", "$argon2id$v=19$m=8,t=1,p=1$aaaa$bbbb"));
    }

    #[test]
    fn asks_for_a_rehash_when_the_cost_has_been_raised() {
        let weak = hasher().hash("password").unwrap();
        let stronger = PasswordHasher::new(Argon2Params {
            memory_cost: 64,
            time_cost: 2,
            threads: 1,
        });

        assert!(!hasher().needs_rehash(&weak), "as configured");
        assert!(stronger.needs_rehash(&weak));
        assert!(stronger.needs_rehash("nonsense"));
    }
}
