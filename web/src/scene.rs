//! The jar on the canvas.
//!
//! Composition order is what makes the jar read as glass with paper inside:
//! body wash, then the note pile clipped to the jar's outline, then the front
//! layer whose inner shadow settles over the pile, then anything in flight —
//! which is drawn last because a note leaving the jar passes in front of it.

use std::cell::RefCell;
use std::rc::Rc;

use jars_core::{geom, pile};
use wasm_bindgen::JsCast;

use crate::sprites::{self, Cache};

/// One bitmap set, shared by every jar on screen. Without this each jar on the
/// shelf would rasterize its own copy of identical art.
pub type SharedCache = Rc<RefCell<Cache>>;

const TILT_MS: f64 = 560.0;
const FLY_MS: f64 = 430.0;

/// How far the jar leans before a note comes out, in degrees. One slow lean and
/// back, pivoting on the jar's base — a fast oscillation reads as a buzzing
/// error state rather than someone tipping a jar.
const TILT_DEGREES: f64 = 4.5;

/// How far above the jar a note travels before the reveal takes over.
const EXIT_Y: f64 = -14.0;
/// A note in the air is bigger than one in the pile — it's coming toward you.
const EXIT_SCALE: f64 = 1.55;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Tilt { t0: f64 },
    Fly { t0: f64, out: bool },
}

/// What just completed, so the UI can advance the ritual.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Finished {
    Tilt,
    FlyOut,
    FlyIn,
}

pub struct Tick {
    pub finished: Option<Finished>,
}

pub struct Scene {
    canvas: web_sys::HtmlCanvasElement,
    ctx: web_sys::CanvasRenderingContext2d,
    cache: SharedCache,
    body: web_sys::Path2d,
    hue: u8,
    seed: u64,
    /// Notes the jar holds. A note in flight or in hand is still counted here,
    /// so a reload mid-reveal can't lose it.
    count: usize,
    /// True while a drawn note is being shown to the user, so the pile renders
    /// one short without the model having to give the note up.
    held: bool,
    /// Written on the jar's label.
    name: String,
    phase: Phase,
    /// Set while a sprite is still decoding, so the first paint isn't blank.
    dirty: bool,
}

impl Scene {
    pub fn new(canvas: web_sys::HtmlCanvasElement, cache: SharedCache) -> Option<Scene> {
        let ctx = canvas
            .get_context("2d")
            .ok()??
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .ok()?;
        let body = web_sys::Path2d::new_with_path_string(geom::BODY_PATH).ok()?;
        Some(Scene {
            canvas,
            ctx,
            cache,
            body,
            hue: 0,
            seed: 1,
            count: 0,
            held: false,
            name: String::new(),
            phase: Phase::Idle,
            dirty: true,
        })
    }

    pub fn set_jar(&mut self, hue: u8, seed: u64, count: usize, held: bool, name: &str) {
        if (self.hue, self.seed, self.count, self.held) != (hue, seed, count, held)
            || self.name != name
        {
            self.hue = hue;
            self.seed = seed;
            self.count = count;
            self.held = held;
            self.name = name.to_string();
            self.dirty = true;
        }
    }

    pub fn tilt(&mut self, now: f64) {
        self.phase = Phase::Tilt { t0: now };
    }

    /// Sends the top note up out of the jar.
    pub fn fly_out(&mut self, now: f64) {
        self.phase = Phase::Fly { t0: now, out: true };
    }

    /// Drops a note back in — used both for "back in jar" and for adding one.
    pub fn fly_in(&mut self, now: f64) {
        self.phase = Phase::Fly { t0: now, out: false };
    }

    /// One frame. Returns whether to keep the loop running.
    pub fn tick(&mut self, now: f64) -> Tick {
        let mut finished = None;

        let tilt = match self.phase {
            Phase::Tilt { t0 } => {
                let t = (now - t0) / TILT_MS;
                if t >= 1.0 {
                    self.phase = Phase::Idle;
                    finished = Some(Finished::Tilt);
                    0.0
                } else {
                    // A single half-cycle: lean out and come back, easing at
                    // both ends. No oscillation, so it never reads as a buzz.
                    (t * std::f64::consts::PI).sin() * TILT_DEGREES
                }
            }
            _ => 0.0,
        };

        let flight = match self.phase {
            Phase::Fly { t0, out } => {
                let t = ((now - t0) / FLY_MS).clamp(0.0, 1.0);
                if t >= 1.0 {
                    self.phase = Phase::Idle;
                    finished = Some(if out { Finished::FlyOut } else { Finished::FlyIn });
                    None
                } else {
                    Some(if out { t } else { 1.0 - t })
                }
            }
            _ => None,
        };

        if self.phase != Phase::Idle || self.dirty || finished.is_some() {
            self.render(tilt, flight);
        }

        Tick { finished }
    }

    /// `tilt_deg` leans the whole jar; `flight` is progress from pile (0.0) to
    /// out of the jar (1.0).
    fn render(&mut self, tilt_deg: f64, flight: Option<f64>) {
        let dpr = crate::platform::window().device_pixel_ratio().max(1.0);
        let cw = self.canvas.client_width().max(1) as f64;
        let ch = self.canvas.client_height().max(1) as f64;

        // Resizing clears the canvas, so only touch it when the size changed.
        let (want_w, want_h) = ((cw * dpr) as u32, (ch * dpr) as u32);
        if self.canvas.width() != want_w || self.canvas.height() != want_h {
            self.canvas.set_width(want_w);
            self.canvas.set_height(want_h);
        }

        let k = (cw / geom::VIEW_W as f64).min(ch / geom::VIEW_H as f64);
        let ox = (cw - geom::VIEW_W as f64 * k) / 2.0;
        let oy = ch - geom::VIEW_H as f64 * k;

        let ctx = &self.ctx;
        let _ = ctx.reset_transform();
        ctx.clear_rect(0.0, 0.0, cw * dpr, ch * dpr);
        let _ = ctx.set_transform(k * dpr, 0.0, 0.0, k * dpr, ox * dpr, oy * dpr);
        ctx.set_image_smoothing_enabled(true);

        // Lean the jar about the centre of its base, so it pivots where it
        // would actually rest rather than spinning about its middle.
        if tilt_deg != 0.0 {
            let _ = ctx.translate(geom::VIEW_W as f64 / 2.0, geom::FLOOR_Y as f64 + 24.0);
            let _ = ctx.rotate(tilt_deg.to_radians());
            let _ = ctx.translate(-(geom::VIEW_W as f64) / 2.0, -(geom::FLOOR_Y as f64 + 24.0));
        }

        // Clone the handles out so the cache borrow ends before drawing.
        let (back, front, note, note_alt) = {
            let mut cache = self.cache.borrow_mut();
            (
                cache.jar_back(self.hue),
                cache.jar_front(self.hue),
                cache.note(false),
                cache.note(true),
            )
        };

        // A note in the air, or in the user's hand mid-reveal, has left the
        // pile but still belongs to the jar — so the pile draws one short.
        // These two states overlap while a note flies back in, hence `||`
        // rather than counting each separately.
        let one_out = flight.is_some() || self.held;
        let in_pile = if one_out { self.count.saturating_sub(1) } else { self.count };
        let placements = pile::layout(self.count, self.seed);

        sprites::draw_jar_layer(ctx, &back);

        ctx.save();
        ctx.clip_with_path_2d(&self.body);
        for (i, p) in placements.iter().take(in_pile).enumerate() {
            let s = if i % 2 == 0 { &note } else { &note_alt };
            sprites::draw_note(ctx, s, p.x as f64, p.y as f64, p.w as f64, p.rot as f64);
        }
        ctx.restore();

        sprites::draw_jar_layer(ctx, &front);
        self.draw_label();

        if let Some(t) = flight {
            if let Some(from) = placements.get(self.count.saturating_sub(1)) {
                let e = ease_out_cubic(t);
                let w = from.w as f64 * (1.0 + (EXIT_SCALE - 1.0) * e);
                let x = from.x as f64 + (geom::VIEW_W as f64 / 2.0 - w / 2.0 - from.x as f64) * e;
                let y = from.y as f64 + (EXIT_Y - from.y as f64) * e;
                // Rotation unwinds toward flat as the note comes toward you.
                let rot = from.rot as f64 * (1.0 - e);
                let sprite = if self.count % 2 == 1 { &note } else { &note_alt };
                sprites::draw_note(ctx, sprite, x, y, w, rot);
            }
        }

        // Sprites decode asynchronously; stay dirty until they've all landed.
        self.dirty = [&back, &front, &note, &note_alt].iter().any(|s| s.image().is_none());
    }
}

impl Scene {
    /// The paper label stuck on the jar, with the jar's name on it.
    ///
    /// Drawn straight to the canvas rather than baked into the jar sprite: an
    /// SVG rendered through an `<img>` can't reach the page's fonts, so the
    /// handwriting would silently fall back to a default face. Canvas text uses
    /// the document's loaded fonts.
    fn draw_label(&self) {
        if self.name.trim().is_empty() {
            return;
        }
        let ctx = &self.ctx;
        let (w, h) = (74.0, 30.0);

        ctx.save();
        let _ = ctx.translate(100.0, 112.0);
        let _ = ctx.rotate(-2.5_f64.to_radians());

        ctx.set_fill_style_str("#FDFAF2");
        ctx.fill_rect(-w / 2.0, -h / 2.0, w, h);
        ctx.set_line_width(1.0);
        ctx.set_stroke_style_str("rgba(100, 112, 140, 0.38)");
        ctx.stroke_rect(-w / 2.0, -h / 2.0, w, h);

        // Shrink to fit rather than spilling over the label's edges.
        let mut size = 15.0_f64;
        let fits = |ctx: &web_sys::CanvasRenderingContext2d, size: f64| {
            ctx.set_font(&format!("{size}px \"Reenie Beanie\", cursive"));
            ctx.measure_text(&self.name).map(|m| m.width()).unwrap_or(0.0)
        };
        while size > 8.0 && fits(ctx, size) > w - 8.0 {
            size -= 0.5;
        }

        ctx.set_fill_style_str("#4f5a74");
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text(&self.name, 0.0, 1.0);
        ctx.restore();
    }
}

fn ease_out_cubic(t: f64) -> f64 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}
