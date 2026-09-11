//! A real server, on a real database, in a temporary directory.
//!
//! These tests drive the router the binary serves — no mocks, no in-memory substitute for
//! SQLite — because what is being checked is behaviour that only exists when the pieces are
//! assembled: that a rolled-back transaction does not burn a sequence value, that a denied
//! request never opens a workspace file, that twenty writers all commit.
//!
//! Each test binary compiles this module separately and uses a different part of it, so anything
//! one of them does not call looks dead from inside that binary — hence the allow.
#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};

use aurum_api::config::{Argon2Params, Auth, Config, Database, Mail, Storage};
use aurum_api::state::AppState;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

pub struct Server {
    pub state: AppState,
    router: Router,
    pub directory: std::path::PathBuf,
}

static NEXT: AtomicU32 = AtomicU32::new(0);

pub async fn server() -> Server {
    build(None).await
}

/// A server that also carries the client's assets, as a deployment does.
pub async fn server_serving(web_dir: std::path::PathBuf) -> Server {
    build(Some(web_dir)).await
}

async fn build(web_dir: Option<std::path::PathBuf>) -> Server {
    let directory = std::env::temp_dir().join(format!(
        "aurum-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));

    std::fs::create_dir_all(&directory).expect("a temporary directory");

    let config = Config {
        bind: "127.0.0.1:0".to_owned(),
        signal_bind: "127.0.0.1:0".to_owned(),
        debug: true,
        cors_allowed_origins: vec!["http://localhost:5173".to_owned()],
        web_dir,
        database: Database {
            control_path: directory.join("control.sqlite"),
            workspace_dir: directory.join("workspace"),
            busy_timeout_ms: 5000,
            auto_migrate: true,
        },
        auth: Auth {
            signing_key: "test-signing-key".to_owned(),
            secret_key: "MDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDA=".to_owned(),
            access_ttl: 900,
            refresh_ttl: 2_592_000,
            cookie_path: "/api/v1/auth".to_owned(),
            cookie_secure: false,
            min_password_length: 12,
            lockout_threshold: 5,
            lockout_window: 900,
            totp_issuer: "Aurum Test".to_owned(),
            // Deliberately cheap: these are tests, not a login.
            argon2: Argon2Params {
                memory_cost: 8,
                time_cost: 1,
                threads: 1,
            },
        },
        storage: Storage {
            endpoint: Some("http://127.0.0.1:1".to_owned()),
            region: "us-east-1".to_owned(),
            bucket: "test".to_owned(),
            key: "k".to_owned(),
            secret: "s".to_owned(),
            use_path_style: true,
        },
        mail: Mail {
            dsn: "null://null".to_owned(),
            from: "aurum@localhost".to_owned(),
        },
    };

    let state = AppState::build(config).await.expect("a server");
    state.db.migrate_control().expect("migrations");

    Server {
        router: aurum_api::routes::router(state.clone()),
        state,
        directory,
    }
}

#[derive(Debug)]
pub struct Answer {
    pub status: StatusCode,
    pub body: Value,
    pub cookies: Vec<String>,
}

impl Answer {
    pub fn code(&self) -> &str {
        self.body["error"]["code"].as_str().unwrap_or_default()
    }
}

impl Server {
    /// A request whose answer is not JSON — the client's own assets, which the binary serves
    /// when it has them.
    pub async fn raw(&self, path: &str) -> (StatusCode, String) {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(path)
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response");

        let status = response.status();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        (status, String::from_utf8_lossy(&body).into_owned())
    }

    pub async fn request(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Answer {
        let mut builder = Request::builder().method(method).uri(path);

        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }

        let request = match body {
            Some(body) => builder
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => builder.body(Body::empty()),
        }
        .expect("a request");

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("a response");

        let status = response.status();
        let cookies = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|value| value.to_str().ok().map(str::to_owned))
            .collect();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        Answer {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            cookies,
        }
    }

    pub async fn get(&self, path: &str, token: &str) -> Answer {
        self.request("GET", path, Some(token), None).await
    }

    pub async fn post(&self, path: &str, token: &str, body: Value) -> Answer {
        self.request("POST", path, Some(token), Some(body)).await
    }

    /// An account with its personal workspace, as registration leaves it.
    pub async fn account(&self, email: &str) -> Account {
        let answer = self
            .request(
                "POST",
                "/api/v1/auth/register",
                None,
                Some(json!({
                    "email": email,
                    "display_name": email.split('@').next().unwrap_or("Someone"),
                    "password": "correct horse battery staple",
                })),
            )
            .await;

        assert_eq!(answer.status, StatusCode::CREATED, "{:?}", answer.body);

        let token = answer.body["access_token"]
            .as_str()
            .expect("a token")
            .to_owned();
        let me = self.get("/api/v1/account", &token).await;

        Account {
            id: me.body["id"].as_str().expect("an id").to_owned(),
            email: email.to_owned(),
            token,
            refresh: refresh_cookie(&answer.cookies).expect("a refresh cookie"),
            workspace: me.body["workspaces"][0]["id"]
                .as_str()
                .expect("a personal workspace")
                .to_owned(),
        }
    }

    /// A band workspace owned by this account, which is what member management needs.
    pub async fn band(&self, owner: &Account, name: &str) -> String {
        let answer = self
            .post("/api/v1/workspaces", &owner.token, json!({ "name": name }))
            .await;

        assert_eq!(answer.status, StatusCode::CREATED, "{:?}", answer.body);

        answer.body["workspace"]["id"]
            .as_str()
            .expect("an id")
            .to_owned()
    }

    pub async fn refresh(&self, cookie: &str) -> Answer {
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/refresh")
            .header("cookie", format!("aurum_refresh={cookie}"))
            .body(Body::empty())
            .expect("a request");

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("a response");
        let status = response.status();
        let cookies = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|value| value.to_str().ok().map(str::to_owned))
            .collect();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        Answer {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            cookies,
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[derive(Clone, Debug)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub token: String,
    pub refresh: String,
    pub workspace: String,
}

pub fn refresh_cookie(cookies: &[String]) -> Option<String> {
    cookies
        .iter()
        .find_map(|cookie| cookie.strip_prefix("aurum_refresh="))
        .map(|rest| rest.split(';').next().unwrap_or_default().to_owned())
        .filter(|value| !value.is_empty())
}

/// A client-minted id, the way the client mints them.
pub fn id() -> String {
    use rand::RngCore;

    let mut random = [0_u8; 10];
    rand::rng().fill_bytes(&mut random);

    aurum_core::ids::uuidv7(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock")
            .as_millis() as i64,
        random,
    )
}

pub fn upsert(table: &str, record: &str, payload: Value) -> Value {
    json!({
        "op_id": id(),
        "table": table,
        "record_id": record,
        "op": "upsert",
        "payload": payload,
    })
}
