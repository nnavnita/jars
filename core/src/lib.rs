//! Domain logic for `jars`, with no dependency on the browser.
//!
//! Everything here is pure so it can be tested with `cargo test` on the host.
//! Anything needing the DOM, canvas, or storage lives in the `jars-web` crate.

pub mod draw;
pub mod geom;
pub mod model;
pub mod pile;
pub mod rng;

pub use model::{BinnedNote, Note, Jar, Store, HUES, MAX_NOTE_LEN, SCHEMA_VERSION};
