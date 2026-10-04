//! The Sky Nursery palette.
//!
//! Six jar hues, so a shelf holds six distinct jars before a colour repeats.
//! Each jar's lid borrows a neighbouring hue, which keeps two jars sitting
//! side by side from reading as the same jar.

pub struct Hue {
    pub name: &'static str,
    /// The jar's watercolour wash.
    pub body: &'static str,
    /// The bleed that escapes past the ink line; a shade deeper than `body`.
    pub bleed: &'static str,
    pub lid: &'static str,
}

pub const HUES: [Hue; 6] = [
    Hue { name: "periwinkle", body: "#D3DEF7", bleed: "#B4C5F0", lid: "#FBDEE6" },
    Hue { name: "blush",      body: "#FBDEE6", bleed: "#F4BFCE", lid: "#FDF2CC" },
    Hue { name: "seafoam",    body: "#D4EDE8", bleed: "#B2DFD6", lid: "#D3DEF7" },
    Hue { name: "butter",     body: "#FDF2CC", bleed: "#F5E3A8", lid: "#D4EDE8" },
    Hue { name: "lilac",      body: "#E6DCF5", bleed: "#CDBCEC", lid: "#FBDEE6" },
    Hue { name: "apricot",    body: "#FBE3D4", bleed: "#F4C8AE", lid: "#E6DCF5" },
];

/// Every line, every letter, every outline.
pub const INK: &str = "#64708C";
/// Warm cream, so folded paper separates from the cool pale jars by temperature
/// as well as by value.
pub const NOTE: &str = "#FDF3DC";
/// A second, slightly deeper paper so a pile doesn't look machine-printed.
pub const NOTE_ALT: &str = "#F8EBCF";
pub const CREASE: &str = "#C3B396";
pub const PAPER: &str = "#FAFBF8";

pub fn hue(i: u8) -> &'static Hue {
    &HUES[(i as usize) % HUES.len()]
}

/// Publishes the palette to CSS as custom properties.
///
/// Without this the page colours would be written twice — once here for the
/// SVG sprites and once in the stylesheet — and the two would drift.
pub fn publish_to_css() {
    let Some(root) = crate::platform::document().document_element() else { return };
    let Ok(el) = wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlElement>(root) else { return };
    let style = el.style();
    for (name, value) in [("--paper", PAPER), ("--ink", INK), ("--note", NOTE)] {
        let _ = style.set_property(name, value);
    }
}
