//! The HTTP client, and the rules about the access token that only it knows.

use std::cell::RefCell;

use js_sys::Reflect;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestCredentials, RequestInit, Response};

use crate::app::locks::in_turn;

#[derive(Clone, Debug, Error)]
pub enum ApiError {
    #[error("{message}")]
    Server {
        status: u16,
        code: String,
        message: String,
        details: Option<Value>,
    },
    /// The request never reached the server. Says nothing about the session.
    #[error("the network is unreachable")]
    Offline,
    #[error("the server sent something this client could not read: {0}")]
    Malformed(String),
}

impl ApiError {
    /// Transient failures the outbox should retry; anything else is parked.
    pub fn is_retryable(&self) -> bool {
        match self {
            ApiError::Server { status, .. } => *status >= 500 || *status == 429,
            ApiError::Offline => true,
            ApiError::Malformed(_) => false,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            ApiError::Server { code, .. } => code,
            ApiError::Offline => "offline",
            ApiError::Malformed(_) => "malformed",
        }
    }

    pub fn status(&self) -> u16 {
        match self {
            ApiError::Server { status, .. } => *status,
            _ => 0,
        }
    }
}

/// Why a session could not be restored.
///
/// "Offline" and "signed out" look the same to a fetch that fails, and they must not look the
/// same to the app: one is a reason to show a sign-in screen, the other is a reason to show the
/// library that is already on the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreResult {
    Ok,
    SignedOut,
    Offline,
}

thread_local! {
    /// The access token lives in memory only. Persisting it would put a bearer credential
    /// somewhere a script can read it, and it is worth so little — fifteen minutes — that
    /// recovering it from the refresh cookie on reload is cheaper than protecting it.
    static ACCESS_TOKEN: RefCell<Option<String>> = const { RefCell::new(None) };
    static SIGNED_OUT: RefCell<Vec<Box<dyn Fn()>>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Debug)]
pub struct Api {
    base: String,
}

impl Default for Api {
    fn default() -> Api {
        Api::new(default_base())
    }
}

/// Same origin in production, and whatever the build was told in development.
fn default_base() -> String {
    option_env!("AURUM_API_URL")
        .map(str::to_owned)
        .unwrap_or_default()
}

impl Api {
    pub fn new(base: impl Into<String>) -> Api {
        Api {
            base: base.into().trim_end_matches('/').to_owned(),
        }
    }

    pub fn set_access_token(token: Option<String>) {
        ACCESS_TOKEN.with(|held| *held.borrow_mut() = token);
    }

    pub fn has_access_token() -> bool {
        ACCESS_TOKEN.with(|held| held.borrow().is_some())
    }

    /// The current token, for the one caller that cannot use `send`: a WebSocket handshake,
    /// which a browser will not let us add headers to, so the token goes in the query string
    /// instead. It lives fifteen minutes and the socket lives two, which is the trade that makes
    /// that acceptable.
    pub fn access_token() -> Option<String> {
        ACCESS_TOKEN.with(|held| held.borrow().clone())
    }

    /// Called when the server says the session is over — not when the network merely failed.
    pub fn on_signed_out(listener: impl Fn() + 'static) {
        SIGNED_OUT.with(|listeners| listeners.borrow_mut().push(Box::new(listener)));
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        self.send("GET", path, None::<()>, true).await
    }

    pub async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: B,
    ) -> Result<T, ApiError> {
        self.send("POST", path, Some(body), true).await
    }

    pub async fn patch<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: B,
    ) -> Result<T, ApiError> {
        self.send("PATCH", path, Some(body), true).await
    }

    pub async fn delete<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: B,
    ) -> Result<T, ApiError> {
        self.send("DELETE", path, Some(body), true).await
    }

    async fn send<B: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<B>,
        retry_on_expiry: bool,
    ) -> Result<T, ApiError> {
        let payload = match &body {
            Some(body) => Some(
                serde_json::to_string(body)
                    .map_err(|error| ApiError::Malformed(error.to_string()))?,
            ),
            None => None,
        };

        let response = self.fetch(method, path, payload.as_deref()).await?;
        let status = response.status();

        if status == 204 {
            return serde_json::from_value(Value::Null)
                .map_err(|error| ApiError::Malformed(error.to_string()));
        }

        let text = JsFuture::from(response.text().map_err(|_| ApiError::Offline)?)
            .await
            .ok()
            .and_then(|text| text.as_string())
            .unwrap_or_default();
        let parsed: Option<Value> = serde_json::from_str(&text).ok();

        if !(200..300).contains(&status) {
            let error = parsed.as_ref().and_then(|body| body.get("error"));
            let code = error
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();

            // A token that expired, or a device that started offline and has none at all: both
            // are fixed by asking for a new one, and neither should surface as an error.
            if status == 401
                && retry_on_expiry
                && (code == "token_expired" || !Api::has_access_token())
                && self.refresh().await
            {
                return Box::pin(self.send(method, path, body, false)).await;
            }

            return Err(ApiError::Server {
                status,
                code,
                message: error
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("Something went wrong.")
                    .to_owned(),
                details: error.and_then(|error| error.get("details")).cloned(),
            });
        }

        serde_json::from_value(parsed.unwrap_or(Value::Null))
            .map_err(|error| ApiError::Malformed(error.to_string()))
    }

    async fn fetch(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<Response, ApiError> {
        let headers = Headers::new().map_err(|_| ApiError::Offline)?;
        let _ = headers.set("Accept", "application/json");

        if body.is_some() {
            let _ = headers.set("Content-Type", "application/json");
        }

        if let Some(token) = Api::access_token() {
            let _ = headers.set("Authorization", &format!("Bearer {token}"));
        }

        let init = RequestInit::new();
        init.set_method(method);
        init.set_headers(&headers);
        // The refresh cookie has to travel, and it is httpOnly, so the browser must be told to
        // send it even when the API is on another origin in development.
        init.set_credentials(RequestCredentials::Include);

        if let Some(body) = body {
            init.set_body(&JsValue::from_str(body));
        }

        let request = Request::new_with_str_and_init(&format!("{}/api/v1{path}", self.base), &init)
            .map_err(|_| ApiError::Offline)?;

        let window = web_sys::window().ok_or(ApiError::Offline)?;

        JsFuture::from(window.fetch_with_request(&request))
            .await
            .map(Response::unchecked_from_js)
            .map_err(|_| ApiError::Offline)
    }

    /// One refresh at a time, device-wide.
    ///
    /// Without the lock, a burst of parallel requests hitting a just-expired token would each
    /// rotate the refresh cookie — and rotation treats a second use of the same token as theft,
    /// which revokes the whole family. The same is true across windows: opening the library, the
    /// control surface and a stage window together would present one cookie three times and sign
    /// the user out of everything, mid-service.
    async fn refresh(&self) -> bool {
        let base = self.base.clone();

        in_turn("aurum-refresh", async move {
            let api = Api::new(base);

            let Ok(response) = api.fetch("POST", "/auth/refresh", None).await else {
                return false;
            };

            if !response.ok() {
                Api::set_access_token(None);

                // 401 is the server saying this session is over. Anything else — a 500, a proxy
                // page — is a broken connection, and the app must not throw somebody out of a
                // service for it.
                if response.status() == 401 {
                    SIGNED_OUT.with(|listeners| {
                        for listener in listeners.borrow().iter() {
                            listener();
                        }
                    });
                }

                return false;
            }

            let Some(token) = JsFuture::from(response.json().expect("a body"))
                .await
                .ok()
                .and_then(|body| {
                    Reflect::get(&body, &"access_token".into())
                        .ok()
                        .and_then(|token| token.as_string())
                })
            else {
                return false;
            };

            Api::set_access_token(Some(token));

            true
        })
        .await
    }

    /// A refresh that reports which of the two failures happened.
    pub async fn restore(&self) -> RestoreResult {
        // `refresh` folds a network failure into `false`, which is exactly the distinction this
        // has to keep, so it asks the question again itself.
        if self.fetch("POST", "/auth/refresh", None).await.is_err() {
            return RestoreResult::Offline;
        }

        if self.refresh().await {
            RestoreResult::Ok
        } else {
            RestoreResult::SignedOut
        }
    }
}
