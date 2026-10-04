//! Thin wrappers over the browser globals, so the rest of the crate can stay
//! readable and the `unwrap`s live in one place.

use wasm_bindgen::prelude::*;

pub fn window() -> web_sys::Window {
    web_sys::window().expect("a browser window")
}

pub fn document() -> web_sys::Document {
    window().document().expect("a document")
}

/// Milliseconds since the epoch, for timestamps we store.
pub fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

/// Monotonic milliseconds since page load, for animation timing.
pub fn now_frame() -> f64 {
    window().performance().map(|p| p.now()).unwrap_or(0.0)
}

/// Uniform random in `[0, 1)`.
pub fn random() -> f64 {
    js_sys::Math::random()
}

/// A fresh id.
///
/// `crypto.randomUUID()` needs a secure context, which `localhost` and `https`
/// both are — but a plain-http LAN preview isn't, so fall back rather than
/// crash the app over an id.
pub fn new_id() -> String {
    window()
        .crypto()
        .ok()
        .map(|c| c.random_uuid())
        .unwrap_or_else(|| format!("{:x}-{:x}", now_ms(), (random() * 1e15) as u64))
}

/// A seed for a jar's pile scatter.
pub fn new_seed() -> u64 {
    ((random() * u32::MAX as f64) as u64) << 32 | (random() * u32::MAX as f64) as u64
}

pub fn warn(msg: &str) {
    web_sys::console::warn_1(&JsValue::from_str(msg));
}

/// Reads a JS property, for APIs web-sys doesn't bind.
pub fn get(target: &JsValue, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(target, &JsValue::from_str(key)).ok().filter(|v| !v.is_undefined())
}

/// True if the browser exposes a given global function.
pub fn has_fn(key: &str) -> bool {
    get(&window().into(), key).map(|v| v.is_function()).unwrap_or(false)
}

/// Calls a JS method by name, for APIs web-sys doesn't bind.
pub fn call(target: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let f: js_sys::Function = get(target, method)
        .ok_or_else(|| JsValue::from_str(&format!("{method} is not available")))?
        .dyn_into()?;
    let arr = js_sys::Array::new();
    for a in args {
        arr.push(a);
    }
    js_sys::Reflect::apply(&f, target, &arr)
}
