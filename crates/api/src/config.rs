//! Everything the server reads from its environment, in one place.
//!
//! A single struct rather than a container of seventeen factories: the shape of the
//! configuration is then something you can read in one screen, and a missing variable is a
//! startup failure rather than a surprise on the first request that happens to need it.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub signal_bind: String,
    pub debug: bool,
    pub cors_allowed_origins: Vec<String>,
    /// The client's built assets, served by this binary when it has them. `None` in development,
    /// where Trunk serves them on its own port and proxies the API back here.
    pub web_dir: Option<PathBuf>,
    pub database: Database,
    pub auth: Auth,
    pub storage: Storage,
    pub mail: Mail,
}

#[derive(Clone, Debug)]
pub struct Database {
    pub control_path: PathBuf,
    pub workspace_dir: PathBuf,
    pub busy_timeout_ms: u32,
    pub auto_migrate: bool,
}

#[derive(Clone, Debug)]
pub struct Auth {
    /// HMAC key for access tokens. Not a secret the client ever sees.
    pub signing_key: String,
    /// AES-GCM key for TOTP secrets at rest, base64.
    pub secret_key: String,
    pub access_ttl: i64,
    pub refresh_ttl: i64,
    pub cookie_path: String,
    pub cookie_secure: bool,
    pub min_password_length: usize,
    pub lockout_threshold: i64,
    pub lockout_window: i64,
    pub totp_issuer: String,
    pub argon2: Argon2Params,
}

#[derive(Clone, Copy, Debug)]
pub struct Argon2Params {
    pub memory_cost: u32,
    pub time_cost: u32,
    pub threads: u32,
}

#[derive(Clone, Debug)]
pub struct Storage {
    pub endpoint: Option<String>,
    pub region: String,
    pub bucket: String,
    pub key: String,
    pub secret: String,
    pub use_path_style: bool,
}

#[derive(Clone, Debug)]
pub struct Mail {
    /// `smtp://host:port`, or `null://null` to swallow mail in development.
    pub dsn: String,
    pub from: String,
}

/// Unset and set-to-empty mean the same thing: a variable a deployment left blank has not been
/// configured, and falling back is friendlier than starting with an empty bucket name.
fn string(key: &str, default: &str) -> String {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => value,
        _ => default.to_owned(),
    }
}

fn optional(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn number<T: std::str::FromStr>(key: &str, default: T) -> T {
    optional(key)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn flag(key: &str, default: bool) -> bool {
    match optional(key) {
        Some(value) => ["1", "true", "yes", "on"].contains(&value.to_lowercase().as_str()),
        None => default,
    }
}

impl Config {
    pub fn from_env() -> Config {
        let data_dir = PathBuf::from(string("DATA_DIR", "var/data"));

        Config {
            bind: string("BIND", "127.0.0.1:8080"),
            signal_bind: string("SIGNAL_BIND", "127.0.0.1:8081"),
            debug: flag("APP_DEBUG", false),
            cors_allowed_origins: string(
                "CORS_ALLOWED_ORIGINS",
                "http://localhost:5173,http://127.0.0.1:5173",
            )
            .split(',')
            .map(|origin| origin.trim().to_owned())
            .filter(|origin| !origin.is_empty())
            .collect(),
            web_dir: optional("WEB_DIR").map(PathBuf::from),
            database: Database {
                control_path: optional("CONTROL_DB_PATH")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| data_dir.join("control.sqlite")),
                workspace_dir: optional("WORKSPACE_DB_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| data_dir.join("workspace")),
                busy_timeout_ms: number("SQLITE_BUSY_TIMEOUT_MS", 5000),
                auto_migrate: flag("DB_AUTO_MIGRATE", true),
            },
            auth: Auth {
                signing_key: string("APP_SIGNING_KEY", "insecure-development-key-change-me"),
                secret_key: string("APP_SECRET_KEY", &base64_zero_key()),
                access_ttl: number("AUTH_ACCESS_TTL", 900),
                refresh_ttl: number("AUTH_REFRESH_TTL", 2_592_000),
                cookie_path: "/api/v1/auth".to_owned(),
                cookie_secure: flag("AUTH_COOKIE_SECURE", true),
                min_password_length: 12,
                lockout_threshold: 5,
                lockout_window: 900,
                totp_issuer: string("TOTP_ISSUER", "Aurum Presenter"),
                argon2: Argon2Params {
                    memory_cost: number("ARGON2_MEMORY_COST", 65_536),
                    time_cost: number("ARGON2_TIME_COST", 4),
                    threads: number("ARGON2_THREADS", 1),
                },
            },
            storage: Storage {
                endpoint: optional("S3_ENDPOINT"),
                region: string("S3_REGION", "us-east-1"),
                bucket: string("S3_BUCKET", "aurum-sheets"),
                key: string("S3_KEY", ""),
                secret: string("S3_SECRET", ""),
                use_path_style: flag("S3_PATH_STYLE", true),
            },
            mail: Mail {
                dsn: string("MAIL_DSN", "null://null"),
                from: string("MAIL_FROM", "aurum@localhost"),
            },
        }
    }
}

/// The development default the PHP used: thirty-two ASCII zeroes, base64. Obviously not a key —
/// which is the point, because a deployment that never set one should look wrong.
fn base64_zero_key() -> String {
    use base64::Engine;

    base64::engine::general_purpose::STANDARD.encode([b'0'; 32])
}
