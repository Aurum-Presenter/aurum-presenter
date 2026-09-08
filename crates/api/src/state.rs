//! Everything a handler can reach, assembled once at startup.
//!
//! An explicit struct rather than a service container: what the server depends on is then a list
//! you can read, and a handler cannot ask for something that was never wired.

use std::sync::Arc;

use crate::auth::cipher::SecretCipher;
use crate::auth::password::PasswordHasher;
use crate::auth::tokens::Tokens;
use crate::auth::totp::{RecoveryCodes, TotpService};
use crate::config::Config;
use crate::db::Databases;
use crate::storage::ObjectStore;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

pub struct Inner {
    pub config: Config,
    pub db: Databases,
    pub tokens: Tokens,
    pub passwords: PasswordHasher,
    pub totp: TotpService,
    pub recovery_codes: RecoveryCodes,
    pub storage: ObjectStore,
}

impl std::ops::Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl AppState {
    pub async fn build(config: Config) -> Result<AppState, String> {
        let cipher = SecretCipher::new(&config.auth.secret_key)?;

        Ok(AppState(Arc::new(Inner {
            db: Databases::new(config.database.clone()),
            tokens: Tokens::new(
                config.auth.signing_key.clone(),
                config.auth.access_ttl,
                config.auth.refresh_ttl,
            ),
            passwords: PasswordHasher::new(config.auth.argon2),
            totp: TotpService::new(cipher, config.auth.totp_issuer.clone()),
            recovery_codes: RecoveryCodes::new(config.auth.signing_key.clone()),
            storage: ObjectStore::new(&config.storage).await,
            config,
        })))
    }
}
