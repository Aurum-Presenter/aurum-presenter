//! The local database, and the reactivity that makes a write show up on the screen.

pub mod idb;
pub mod live;
pub mod schema;

use js_sys::Array;
use thiserror::Error;
use wasm_bindgen::prelude::*;
use web_sys::{IdbDatabase, IdbObjectStoreParameters, IdbTransactionMode};

use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Clone, Debug, Error)]
pub enum DbError {
    #[error("this browser has no IndexedDB")]
    Unavailable,
    #[error("the database could not be opened: {0}")]
    Open(String),
    #[error("{0}")]
    Request(String),
    #[error("a record could not be read: {0}")]
    Shape(String),
}

/// One workspace's database.
#[derive(Clone)]
pub struct Database {
    inner: IdbDatabase,
    workspace_id: String,
}

impl Database {
    pub async fn open(workspace_id: &str) -> Result<Database, DbError> {
        let factory = web_sys::window()
            .ok_or(DbError::Unavailable)?
            .indexed_db()
            .map_err(|_| DbError::Unavailable)?
            .ok_or(DbError::Unavailable)?;

        let request = factory
            .open_with_u32(&schema::database_name(workspace_id), schema::VERSION)
            .map_err(|_| DbError::Unavailable)?;

        let opened = idb::open_request(request, create_missing_stores).await?;

        Ok(Database {
            inner: opened.unchecked_into(),
            workspace_id: workspace_id.to_owned(),
        })
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        store: &str,
        key: &str,
    ) -> Result<Option<T>, DbError> {
        let value = idb::request(
            self.store(store, IdbTransactionMode::Readonly)?
                .get(&JsValue::from_str(key))
                .map_err(|_| DbError::Request(format!("reading from {store}")))?,
        )
        .await?;

        from_js(value)
    }

    pub async fn all<T: DeserializeOwned>(&self, store: &str) -> Result<Vec<T>, DbError> {
        let value = idb::request(
            self.store(store, IdbTransactionMode::Readonly)?
                .get_all()
                .map_err(|_| DbError::Request(format!("listing {store}")))?,
        )
        .await?;

        collect(value)
    }

    /// Everything in a store whose index equals a value — the shape of nearly every read the
    /// app makes: the arrangements of a song, the items of a set.
    pub async fn by_index<T: DeserializeOwned>(
        &self,
        store: &str,
        index: &str,
        value: &JsValue,
    ) -> Result<Vec<T>, DbError> {
        let store = self.store(store, IdbTransactionMode::Readonly)?;
        let index = store
            .index(index)
            .map_err(|_| DbError::Request(format!("no index {index}")))?;
        let found = idb::request(
            index
                .get_all_with_key(value)
                .map_err(|_| DbError::Request("querying an index".to_owned()))?,
        )
        .await?;

        collect(found)
    }

    pub async fn put<T: Serialize>(&self, store: &str, record: &T) -> Result<JsValue, DbError> {
        let value = serde_wasm_bindgen::to_value(record)
            .map_err(|error| DbError::Shape(error.to_string()))?;
        let key = idb::request(
            self.store(store, IdbTransactionMode::Readwrite)?
                .put(&value)
                .map_err(|_| DbError::Request(format!("writing to {store}")))?,
        )
        .await?;

        live::changed(&self.workspace_id, store);

        Ok(key)
    }

    /// Writes many records in one transaction, and announces the store once.
    ///
    /// A pull that brought four hundred rows must not wake every list four hundred times.
    pub async fn put_all<T: Serialize>(&self, store: &str, records: &[T]) -> Result<(), DbError> {
        if records.is_empty() {
            return Ok(());
        }

        let handle = self.store(store, IdbTransactionMode::Readwrite)?;
        let mut last = None;

        for record in records {
            let value = serde_wasm_bindgen::to_value(record)
                .map_err(|error| DbError::Shape(error.to_string()))?;

            last = Some(
                handle
                    .put(&value)
                    .map_err(|_| DbError::Request(format!("writing to {store}")))?,
            );
        }

        if let Some(request) = last {
            idb::request(request).await?;
        }

        live::changed(&self.workspace_id, store);

        Ok(())
    }

    pub async fn delete(&self, store: &str, key: &JsValue) -> Result<(), DbError> {
        idb::request(
            self.store(store, IdbTransactionMode::Readwrite)?
                .delete(key)
                .map_err(|_| DbError::Request(format!("deleting from {store}")))?,
        )
        .await?;

        live::changed(&self.workspace_id, store);

        Ok(())
    }

    pub async fn clear(&self, store: &str) -> Result<(), DbError> {
        idb::request(
            self.store(store, IdbTransactionMode::Readwrite)?
                .clear()
                .map_err(|_| DbError::Request(format!("clearing {store}")))?,
        )
        .await?;

        live::changed(&self.workspace_id, store);

        Ok(())
    }

    pub async fn count(&self, store: &str) -> Result<u32, DbError> {
        let value = idb::request(
            self.store(store, IdbTransactionMode::Readonly)?
                .count()
                .map_err(|_| DbError::Request(format!("counting {store}")))?,
        )
        .await?;

        Ok(value.as_f64().unwrap_or_default() as u32)
    }

    fn store(
        &self,
        name: &str,
        mode: IdbTransactionMode,
    ) -> Result<web_sys::IdbObjectStore, DbError> {
        self.inner
            .transaction_with_str_and_mode(name, mode)
            .and_then(|transaction| transaction.object_store(name))
            .map_err(|_| DbError::Request(format!("no store named {name}")))
    }
}

/// Creates whatever is missing rather than rebuilding.
///
/// The version chain in the TypeScript added stores one release at a time; a device that has
/// been through it already holds them. Asking for what is absent is the same outcome for a
/// device that has everything, a device that has some of it, and a device that has none.
fn create_missing_stores(database: &IdbDatabase) {
    let existing = database.object_store_names();

    for store in schema::STORES {
        if (0..existing.length()).any(|index| existing.get(index).as_deref() == Some(store.name)) {
            continue;
        }

        let parameters = IdbObjectStoreParameters::new();
        parameters.set_key_path(&JsValue::from_str(store.key_path));
        parameters.set_auto_increment(store.auto_increment);

        let Ok(created) =
            database.create_object_store_with_optional_parameters(store.name, &parameters)
        else {
            continue;
        };

        for index in store.indexes {
            let key_path = match index.key_path {
                [] => JsValue::from_str(index.name),
                paths => Array::from_iter(paths.iter().map(|path| JsValue::from_str(path))).into(),
            };

            let _ = created.create_index_with_str_sequence(index.name, &key_path.unchecked_into());
        }
    }
}

fn from_js<T: DeserializeOwned>(value: JsValue) -> Result<Option<T>, DbError> {
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }

    serde_wasm_bindgen::from_value(value)
        .map(Some)
        .map_err(|error| DbError::Shape(error.to_string()))
}

fn collect<T: DeserializeOwned>(value: JsValue) -> Result<Vec<T>, DbError> {
    serde_wasm_bindgen::from_value(value).map_err(|error| DbError::Shape(error.to_string()))
}
