//! Jar and note geometry, in sprite space.
//!
//! Everything is expressed against a 200x230 box so the SVG sprite, the pile
//! layout, and the canvas all agree on where the inside of a jar is. The canvas
//! scales this box; it never redefines it.

/// Sprite box the jar is authored in.
pub const VIEW_W: f32 = 200.0;
pub const VIEW_H: f32 = 230.0;

/// The jar body outline. Shared by the SVG sprite and the note clip region.
pub const BODY_PATH: &str = "M58 78 Q58 64 72 62 L128 62 Q142 64 142 78 L147 178 Q149 206 122 208 L78 208 Q51 206 53 178 Z";

/// The watercolour bleed sits slightly outside the ink line, on purpose.
pub const BLEED_PATH: &str = "M54 74 Q54 60 70 58 L130 58 Q146 60 146 74 L151 180 Q153 210 122 212 L78 212 Q47 210 49 180 Z";

/// The lid band across the jar's neck.
pub const LID_PATH: &str = "M54 50 Q54 44 62 44 L138 44 Q146 44 146 50 L146 62 Q146 68 138 68 L62 68 Q54 68 54 62 Z";

/// Where notes are allowed to come to rest.
pub const FLOOR_Y: f32 = 184.0;
pub const CEILING_Y: f32 = 76.0;
pub const LEFT_X: f32 = 62.0;
/// Horizontal gap between the two notes in a row.
pub const COLUMN_GAP: f32 = 24.0;

/// Nominal folded-note size before per-note jitter.
pub const NOTE_W: f32 = 42.0;
pub const NOTE_H: f32 = 20.0;

/// Vertical pitch between pile rows when the jar is not crowded.
pub const ROW_PITCH: f32 = 12.5;
/// Notes per pile row.
pub const PER_ROW: usize = 2;
