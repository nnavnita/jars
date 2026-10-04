//! jars — a todo list you don't get to choose from.

mod backup;
mod palette;
mod platform;
mod scene;
mod sprites;
mod state;
mod store;
mod ui;

fn main() {
    // Without this a Rust panic surfaces as "unreachable executed" with no
    // stack, which is unhelpful in a canvas app.
    console_error_panic_hook::set_once();
    palette::publish_to_css();
    register_service_worker();
    leptos::mount::mount_to_body(ui::Root);
}

/// Registers the offline worker, so an installed jars works with no network.
///
/// Skipped on localhost: a cached shell during development serves stale wasm
/// and makes every change look like it didn't apply.
fn register_service_worker() {
    let host = platform::window().location().hostname().unwrap_or_default();
    if host == "localhost" || host == "127.0.0.1" {
        return;
    }
    let _ = platform::window().navigator().service_worker().register("sw.js");
}
