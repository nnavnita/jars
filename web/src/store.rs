//! Persistence in IndexedDB.
//!
//! The whole store is one JSON record. Jars are small and always read and
//! written together, so per-jar rows would buy nothing and cost a schema.

use std::cell::RefCell;
use std::rc::Rc;

use jars_core::model::{LoadError, Store};
use rexie::{ObjectStore, Rexie, TransactionMode};
use wasm_bindgen::prelude::*;

const DB_NAME: &str = "jars";
const DB_VERSION: u32 = 1;
const TABLE: &str = "state";
const KEY_STORE: &str = "store";
/// The handle to the user's chosen backup file, kept here because
/// `FileSystemFileHandle` is structured-cloneable and so survives a reload.
const KEY_BACKUP_HANDLE: &str = "backup-handle";
/// Where an unreadable record is parked. A fixed key, so repeated failed loads
/// overwrite one copy rather than piling up a new one each time.
const KEY_UNREADABLE: &str = "store-unreadable";

#[derive(Debug)]
pub enum StoreError {
    /// IndexedDB is unavailable — private browsing, or storage is blocked.
    Unavailable(String),
    /// The saved record exists but can't be read.
    Corrupt(LoadError),
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StoreError::Unavailable(why) => {
                write!(f, "jars can't be saved in this browser ({why})")
            }
            StoreError::Corrupt(e) => write!(f, "saved jars couldn't be read: {e}"),
        }
    }
}

thread_local! {
    /// The open connection, reused for every operation.
    ///
    /// Opening per operation and dropping the handle leaves the underlying
    /// IndexedDB connection open on the JS side while its Rust callbacks are
    /// freed — so a later `versionchange` or `close` event invokes a dropped
    /// closure and throws. Holding one connection for the page's lifetime
    /// avoids that, and saves an open on every save.
    static DB: RefCell<Option<Rc<Rexie>>> = const { RefCell::new(None) };
}

async fn db() -> Result<Rc<Rexie>, StoreError> {
    if let Some(open) = DB.with(|d| d.borrow().clone()) {
        return Ok(open);
    }
    let opened = Rc::new(open_db().await?);
    DB.with(|d| *d.borrow_mut() = Some(opened.clone()));
    Ok(opened)
}

async fn open_db() -> Result<Rexie, StoreError> {
    Rexie::builder(DB_NAME)
        .version(DB_VERSION)
        // No key path: keys are passed in explicitly, since this table holds a
        // couple of singleton records rather than a collection.
        .add_object_store(ObjectStore::new(TABLE))
        .build()
        .await
        .map_err(|e| StoreError::Unavailable(e.to_string()))
}

async fn read(key: &str) -> Result<Option<JsValue>, StoreError> {
    let db = db().await?;
    let tx = db
        .transaction(&[TABLE], TransactionMode::ReadOnly)
        .map_err(|e| StoreError::Unavailable(e.to_string()))?;
    let table = tx.store(TABLE).map_err(|e| StoreError::Unavailable(e.to_string()))?;
    let got = table
        .get(JsValue::from_str(key))
        .await
        .map_err(|e| StoreError::Unavailable(e.to_string()))?;
    let _ = tx.done().await;
    Ok(got.filter(|v| !v.is_undefined() && !v.is_null()))
}

async fn write(key: &str, value: &JsValue) -> Result<(), StoreError> {
    let db = db().await?;
    let tx = db
        .transaction(&[TABLE], TransactionMode::ReadWrite)
        .map_err(|e| StoreError::Unavailable(e.to_string()))?;
    let table = tx.store(TABLE).map_err(|e| StoreError::Unavailable(e.to_string()))?;
    table
        .put(value, Some(&JsValue::from_str(key)))
        .await
        .map_err(|e| StoreError::Unavailable(e.to_string()))?;
    tx.done().await.map_err(|e| StoreError::Unavailable(e.to_string()))?;
    Ok(())
}

/// Loads the saved jars. A browser with nothing saved yet gets an empty shelf,
/// which is not an error.
pub async fn load() -> Result<Store, StoreError> {
    match read(KEY_STORE).await? {
        None => Ok(Store::default()),
        Some(v) => {
            let raw = v.as_string().ok_or(StoreError::Corrupt(LoadError::Malformed))?;
            match Store::from_json(&raw) {
                Ok(store) => Ok(store),
                Err(e) => {
                    // Set the unreadable record aside under its own key before
                    // carrying on. The app stays usable, and the next save
                    // can't overwrite the only copy of whatever went wrong.
                    let _ = write(KEY_UNREADABLE, &JsValue::from_str(&raw)).await;
                    Err(StoreError::Corrupt(e))
                }
            }
        }
    }
}

pub async fn save(store: &Store) -> Result<(), StoreError> {
    write(KEY_STORE, &JsValue::from_str(&store.to_json())).await
}

pub async fn load_backup_handle() -> Option<JsValue> {
    read(KEY_BACKUP_HANDLE).await.ok().flatten()
}

pub async fn save_backup_handle(handle: &JsValue) -> Result<(), StoreError> {
    write(KEY_BACKUP_HANDLE, handle).await
}

/// Asks the browser to exempt this origin from storage eviction.
///
/// Safari evicts data for sites unvisited for seven days unless the site is
/// installed to the home screen; this is the other half of that defence.
pub async fn request_persistence() -> bool {
    let storage = crate::platform::window().navigator().storage();
    match storage.persist() {
        Ok(promise) => wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map(|v| v.as_bool().unwrap_or(false))
            .unwrap_or(false),
        Err(_) => false,
    }
}
