//! Backups, export and import.
//!
//! Two paths, because browsers differ on one capability:
//!
//! * **Chrome and Edge** have the File System Access API, so the app can hold a
//!   handle to one file the user picked and silently rewrite *that same file*
//!   after every change. One file, always current, nothing piling up.
//! * **Safari and Firefox** have no such API. A page there cannot overwrite a
//!   file it previously wrote — `<a download>` hands control to the browser,
//!   which appends ` (1)`, ` (2)` rather than replacing. Those browsers get a
//!   one-tap manual export plus a nudge when a backup is overdue.
//!
//! The API calls go through `js_sys::Reflect` rather than `web-sys` because
//! these bindings are still gated behind an unstable-API flag upstream, and
//! this keeps the build on stable defaults.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::platform::{self, call, get, window};

/// Days without a backup before the nudge appears.
pub const NUDGE_AFTER_DAYS: i64 = 30;
const MS_PER_DAY: i64 = 86_400_000;

pub const BACKUP_FILENAME: &str = "jars-backup.json";

/// Whether this browser can silently overwrite a chosen file.
pub fn silent_overwrite_supported() -> bool {
    platform::has_fn("showSaveFilePicker")
}

/// Should the "back up your jars" nudge show?
///
/// Only asked on browsers without silent overwrite — elsewhere the file is
/// already current, so a nudge would be noise.
pub fn nudge_due(last_backup_at: Option<i64>, now: i64) -> bool {
    if silent_overwrite_supported() {
        return false;
    }
    match last_backup_at {
        None => true,
        Some(then) => now.saturating_sub(then) > NUDGE_AFTER_DAYS * MS_PER_DAY,
    }
}

/// Opens the save dialog so the user can nominate their backup file.
///
/// Must be called from a click: browsers reject a file picker that no gesture
/// asked for.
pub async fn choose_backup_file() -> Result<JsValue, JsValue> {
    let accept = js_sys::Object::new();
    js_sys::Reflect::set(
        &accept,
        &"application/json".into(),
        &js_sys::Array::of1(&".json".into()),
    )?;

    let kind = js_sys::Object::new();
    js_sys::Reflect::set(&kind, &"description".into(), &"jars backup".into())?;
    js_sys::Reflect::set(&kind, &"accept".into(), &accept)?;

    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"suggestedName".into(), &BACKUP_FILENAME.into())?;
    js_sys::Reflect::set(&options, &"types".into(), &js_sys::Array::of1(&kind))?;

    let promise = call(&window().into(), "showSaveFilePicker", &[options.into()])?;
    JsFuture::from(js_sys::Promise::from(promise)).await
}

/// Confirms we may still write to the handle.
///
/// Permission can lapse between sessions, and an installed PWA is likelier to
/// keep it than a plain tab.
async fn ensure_writable(handle: &JsValue) -> bool {
    let descriptor = js_sys::Object::new();
    if js_sys::Reflect::set(&descriptor, &"mode".into(), &"readwrite".into()).is_err() {
        return false;
    }

    let granted = |v: JsValue| v.as_string().as_deref() == Some("granted");

    if let Ok(p) = call(handle, "queryPermission", &[descriptor.clone().into()]) {
        if let Ok(v) = JsFuture::from(js_sys::Promise::from(p)).await {
            if granted(v) {
                return true;
            }
        }
    }
    if let Ok(p) = call(handle, "requestPermission", &[descriptor.into()]) {
        if let Ok(v) = JsFuture::from(js_sys::Promise::from(p)).await {
            return granted(v);
        }
    }
    false
}

/// Overwrites the chosen backup file in place.
pub async fn write_to_handle(handle: &JsValue, contents: &str) -> Result<(), JsValue> {
    if !ensure_writable(handle).await {
        return Err(JsValue::from_str("permission to write the backup file was declined"));
    }

    let writable = JsFuture::from(js_sys::Promise::from(call(
        handle,
        "createWritable",
        &[],
    )?))
    .await?;

    // A failed write must still close the stream, or the file is left truncated.
    let wrote = match call(&writable, "write", &[JsValue::from_str(contents)]) {
        Ok(p) => JsFuture::from(js_sys::Promise::from(p)).await.map(|_| ()),
        Err(e) => Err(e),
    };
    let closed = match call(&writable, "close", &[]) {
        Ok(p) => JsFuture::from(js_sys::Promise::from(p)).await.map(|_| ()),
        Err(e) => Err(e),
    };
    wrote.and(closed)
}

/// Hands a file to the browser's downloads, for export and for the manual
/// backup path.
pub fn download(filename: &str, contents: &str) -> Result<(), JsValue> {
    let parts = js_sys::Array::of1(&JsValue::from_str(contents));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/json");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;

    let anchor: web_sys::HtmlAnchorElement =
        platform::document().create_element("a")?.dyn_into()?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();

    // The object URL pins the blob in memory until it's revoked.
    web_sys::Url::revoke_object_url(&url)?;
    Ok(())
}

/// Reads a picked file as text.
pub async fn read_file(file: &web_sys::File) -> Result<String, JsValue> {
    let text = JsFuture::from(file.text()).await?;
    text.as_string().ok_or_else(|| JsValue::from_str("that file isn't text"))
}

/// A filename for an exported jar, safe to put on a filesystem.
pub fn export_filename(jar_name: &str) -> String {
    let slug: String = jar_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').replace("--", "-");
    if slug.is_empty() {
        "jar.json".to_string()
    } else {
        format!("{slug}.json")
    }
}

/// True when a JS error means "the user closed the dialog", which is not a
/// failure worth reporting.
pub fn is_abort(err: &JsValue) -> bool {
    get(err, "name").and_then(|n| n.as_string()).as_deref() == Some("AbortError")
}
