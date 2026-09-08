//! `localStorage`, and the handful of keys the app keeps there.
//!
//! The names are contract: the end-to-end suite reads them, and a device that already holds
//! them has to find its state where it left it.

use web_sys::Storage;

pub const WORKSPACE: &str = "aurum.workspace";
pub const LOCAL: &str = "aurum.local";
pub const SESSION_ACTIVE: &str = "aurum.session.active";
pub const STAGE_LAST: &str = "aurum.stage.last";

pub fn released(workspace_id: &str) -> String {
    format!("aurum.released.{workspace_id}")
}

fn storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub fn read(key: &str) -> Option<String> {
    storage()?
        .get_item(key)
        .ok()
        .flatten()
        .filter(|value| !value.is_empty())
}

pub fn write(key: &str, value: &str) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(key, value);
    }
}

pub fn remove(key: &str) {
    if let Some(storage) = storage() {
        let _ = storage.remove_item(key);
    }
}

pub fn read_json<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    serde_json::from_str(&read(key)?).ok()
}

pub fn write_json<T: serde::Serialize>(key: &str, value: &T) {
    if let Ok(text) = serde_json::to_string(value) {
        write(key, &text);
    }
}
