//! Procedural watercolour, via the browser's own SVG filter engine.
//!
//! Canvas2D has no primitive for paper grain or a hand-drawn wobble, but SVG
//! does: `feTurbulence` and `feDisplacementMap`. So the jar art is authored as
//! an SVG string, handed to the browser as an image, and rasterized **once**
//! into a bitmap the scene then blits every frame. Parsing and filtering SVG is
//! slow; copying a bitmap is not, and the whole renderer rests on that split.
//!
//! The upshot is procedural art with no art pipeline: recolouring a jar means
//! regenerating a string, so a seventh hue is a palette edit, not new artwork.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use jars_core::geom;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::palette;

/// Sprites are rasterized at 3x the sprite box, which covers every phone and
/// laptop DPR in use without caching a variant per display.
const OVERSAMPLE: f64 = 3.0;

/// The note sprite's own box. Wider than the note so the wobble filter has
/// room to displace pixels without clipping them at the edge.
const NOTE_BOX_W: f64 = 52.0;
const NOTE_BOX_H: f64 = 30.0;
/// Where the note art sits inside that box.
const NOTE_INSET: f64 = 3.0;
/// The note's drawn size, before per-note scaling.
const NOTE_ART_W: f64 = 45.0;
const NOTE_ART_H: f64 = 21.0;

/// A rasterized sprite. `ready` flips once the browser has decoded it.
pub struct Sprite {
    img: web_sys::HtmlImageElement,
    ready: Rc<Cell<bool>>,
    /// Kept alive so the load callback isn't dropped before it fires.
    _onload: Closure<dyn FnMut()>,
}

impl Sprite {
    fn new(svg: &str, w: f64, h: f64) -> Sprite {
        let img = web_sys::HtmlImageElement::new().expect("an image element");
        img.set_width((w * OVERSAMPLE) as u32);
        img.set_height((h * OVERSAMPLE) as u32);

        let ready = Rc::new(Cell::new(false));
        let flag = ready.clone();
        let onload = Closure::<dyn FnMut()>::new(move || flag.set(true));
        img.set_onload(Some(onload.as_ref().unchecked_ref()));

        let url = format!(
            "data:image/svg+xml;charset=utf-8,{}",
            String::from(js_sys::encode_uri_component(svg))
        );
        img.set_src(&url);

        Sprite { img, ready, _onload: onload }
    }

    pub fn image(&self) -> Option<&web_sys::HtmlImageElement> {
        // Decoded images can also arrive from cache with complete() already
        // true, before onload ever runs.
        if self.ready.get() || self.img.complete() {
            Some(&self.img)
        } else {
            None
        }
    }
}

/// Keeps one bitmap per distinct bit of art.
#[derive(Default)]
pub struct Cache {
    sprites: HashMap<String, Rc<Sprite>>,
}

impl Cache {
    /// The jar behind the notes: watercolour bleed, then the body wash.
    pub fn jar_back(&mut self, hue: u8) -> Rc<Sprite> {
        self.entry(&format!("back:{hue}"), geom::VIEW_W as f64, geom::VIEW_H as f64, || {
            jar_back_svg(hue)
        })
    }

    /// The jar in front of the notes: inner shadow, ink outline, lid.
    pub fn jar_front(&mut self, hue: u8) -> Rc<Sprite> {
        self.entry(&format!("front:{hue}"), geom::VIEW_W as f64, geom::VIEW_H as f64, || {
            jar_front_svg(hue)
        })
    }

    /// A folded note. `alt` picks the second, slightly deeper paper.
    pub fn note(&mut self, alt: bool) -> Rc<Sprite> {
        self.entry(&format!("note:{alt}"), NOTE_BOX_W, NOTE_BOX_H, || note_svg(alt))
    }

    fn entry(
        &mut self,
        key: &str,
        w: f64,
        h: f64,
        build: impl FnOnce() -> String,
    ) -> Rc<Sprite> {
        if let Some(s) = self.sprites.get(key) {
            return s.clone();
        }
        let sprite = Rc::new(Sprite::new(&build(), w, h));
        self.sprites.insert(key.to_string(), sprite.clone());
        sprite
    }
}

/// The wobble that makes an outline look hand-drawn, and the grain that makes a
/// flat fill look painted. Shared by every sprite.
fn filters() -> &'static str {
    r##"<filter id="wb" x="-14%" y="-14%" width="128%" height="128%">
<feTurbulence type="fractalNoise" baseFrequency="0.035" numOctaves="2" seed="7" result="t"/>
<feDisplacementMap in="SourceGraphic" in2="t" scale="4.5" xChannelSelector="R" yChannelSelector="G"/>
</filter>
<filter id="wa" x="-22%" y="-22%" width="144%" height="144%">
<feTurbulence type="fractalNoise" baseFrequency="0.7" numOctaves="4" result="n"/>
<feColorMatrix in="n" type="saturate" values="0" result="ng"/>
<feComponentTransfer in="ng" result="nl"><feFuncA type="linear" slope="0.3"/></feComponentTransfer>
<feComposite in="nl" in2="SourceGraphic" operator="in" result="sp"/>
<feBlend in="SourceGraphic" in2="sp" mode="multiply"/>
</filter>"##
}

fn svg_open(w: f64, h: f64) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{pw}" height="{ph}">"#,
        pw = w * OVERSAMPLE,
        ph = h * OVERSAMPLE,
    )
}

fn jar_back_svg(hue: u8) -> String {
    let h = palette::hue(hue);
    format!(
        r#"{open}<defs>{filters}</defs>
<g filter="url(#wb)"><path d="{bleed_path}" fill="{bleed}" opacity="0.4"/></g>
<g filter="url(#wa)"><path d="{body_path}" fill="{body}"/></g>
</svg>"#,
        open = svg_open(geom::VIEW_W as f64, geom::VIEW_H as f64),
        filters = filters(),
        bleed_path = geom::BLEED_PATH,
        bleed = h.bleed,
        body_path = geom::BODY_PATH,
        body = h.body,
    )
}

fn jar_front_svg(hue: u8) -> String {
    let h = palette::hue(hue);
    // The inner shadow is the detail that makes a draining jar readable: it
    // gives the pile a floor to sit on, so fill level reads even when a pale
    // jar and pale notes are nearly the same value.
    format!(
        r#"{open}<defs>{filters}
<radialGradient id="sh" cx="50%" cy="88%" r="62%">
<stop offset="0%" stop-color="{ink}" stop-opacity="0.22"/>
<stop offset="100%" stop-color="{ink}" stop-opacity="0"/>
</radialGradient>
<clipPath id="cp"><path d="{body_path}"/></clipPath>
</defs>
<g clip-path="url(#cp)"><path d="{body_path}" fill="url(#sh)"/></g>
<g filter="url(#wb)">
<path d="{body_path}" fill="none" stroke="{ink}" stroke-width="2.3" stroke-linejoin="round"/>
<path d="{lid_path}" fill="{lid}" stroke="{ink}" stroke-width="2.3"/>
</g>
</svg>"#,
        open = svg_open(geom::VIEW_W as f64, geom::VIEW_H as f64),
        filters = filters(),
        ink = palette::INK,
        body_path = geom::BODY_PATH,
        lid_path = geom::LID_PATH,
        lid = h.lid,
    )
}

fn note_svg(alt: bool) -> String {
    let face = if alt { palette::NOTE_ALT } else { palette::NOTE };
    // Slightly uneven corners, so a folded note doesn't read as a rectangle.
    let outline = format!("M0 1 L{w} 0 L{w1} {h} L1 {h1} Z", w = 44, w1 = 45, h = 21, h1 = 22);
    format!(
        r#"{open}<defs>{filters}</defs>
<g filter="url(#wb)" transform="translate({inset} {inset})">
<path d="{outline}" fill="{face}"/>
<path d="M0 11.5 L45 10.5" stroke="{crease}" stroke-width="1.3"/>
<path d="{outline}" fill="none" stroke="{ink}" stroke-width="1.3" opacity="0.85"/>
</g>
</svg>"#,
        open = svg_open(NOTE_BOX_W, NOTE_BOX_H),
        filters = filters(),
        inset = NOTE_INSET,
        crease = palette::CREASE,
        ink = palette::INK,
    )
}

/// Blits one note, rotated about its own centre.
///
/// The sprite box is bigger than the note art, so the draw is offset by the
/// inset — otherwise every note would sit down-right of where the pile put it.
pub fn draw_note(
    ctx: &web_sys::CanvasRenderingContext2d,
    sprite: &Sprite,
    x: f64,
    y: f64,
    w: f64,
    rot_deg: f64,
) {
    let Some(img) = sprite.image() else { return };
    let s = w / NOTE_ART_W;

    ctx.save();
    let _ = ctx.translate(x + (NOTE_ART_W / 2.0) * s, y + (NOTE_ART_H / 2.0) * s);
    let _ = ctx.rotate(rot_deg.to_radians());
    let _ = ctx.draw_image_with_html_image_element_and_dw_and_dh(
        img,
        -(NOTE_ART_W / 2.0 + NOTE_INSET) * s,
        -(NOTE_ART_H / 2.0 + NOTE_INSET) * s,
        NOTE_BOX_W * s,
        NOTE_BOX_H * s,
    );
    ctx.restore();
}

/// Blits a whole-jar sprite at sprite-space origin.
pub fn draw_jar_layer(ctx: &web_sys::CanvasRenderingContext2d, sprite: &Sprite) {
    let Some(img) = sprite.image() else { return };
    let _ = ctx.draw_image_with_html_image_element_and_dw_and_dh(
        img,
        0.0,
        0.0,
        geom::VIEW_W as f64,
        geom::VIEW_H as f64,
    );
}

/// A complete jar, pile and all, as a standalone SVG document.
///
/// The shelf uses this as a plain `<img>`: a thumbnail never animates, so it
/// has no business running a canvas or an animation frame loop. Same geometry
/// and same palette as the canvas renderer, so a jar looks identical in both.
pub fn jar_svg_full(hue: u8, count: usize, seed: u64) -> String {
    let h = palette::hue(hue);
    let notes = jars_core::pile::layout(count, seed)
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let face = if i % 2 == 0 { palette::NOTE } else { palette::NOTE_ALT };
            let s = p.w as f64 / NOTE_ART_W;
            format!(
                r#"<g transform="translate({x} {y}) rotate({rot} {cx} {cy}) scale({s})">
<path d="M0 1 L44 0 L45 21 L1 22 Z" fill="{face}"/>
<path d="M0 11.5 L45 10.5" stroke="{crease}" stroke-width="1.3"/>
<path d="M0 1 L44 0 L45 21 L1 22 Z" fill="none" stroke="{ink}" stroke-width="1.3" opacity="0.85"/>
</g>"#,
                x = p.x,
                y = p.y,
                rot = p.rot,
                cx = NOTE_ART_W / 2.0,
                cy = NOTE_ART_H / 2.0,
                crease = palette::CREASE,
                ink = palette::INK,
            )
        })
        .collect::<String>();

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {hh}"><defs>{filters}
<radialGradient id="sh" cx="50%" cy="88%" r="62%">
<stop offset="0%" stop-color="{ink}" stop-opacity="0.22"/>
<stop offset="100%" stop-color="{ink}" stop-opacity="0"/>
</radialGradient>
<clipPath id="cp"><path d="{body}"/></clipPath>
</defs>
<g filter="url(#wb)"><path d="{bleed_path}" fill="{bleed}" opacity="0.4"/></g>
<g filter="url(#wa)"><path d="{body}" fill="{body_fill}"/></g>
<g clip-path="url(#cp)">{notes}<path d="{body}" fill="url(#sh)"/></g>
<g filter="url(#wb)">
<path d="{body}" fill="none" stroke="{ink}" stroke-width="2.3" stroke-linejoin="round"/>
<path d="{lid_path}" fill="{lid}" stroke="{ink}" stroke-width="2.3"/>
</g>
</svg>"#,
        w = geom::VIEW_W,
        hh = geom::VIEW_H,
        filters = filters(),
        ink = palette::INK,
        body = geom::BODY_PATH,
        bleed_path = geom::BLEED_PATH,
        bleed = h.bleed,
        body_fill = h.body,
        lid_path = geom::LID_PATH,
        lid = h.lid,
    )
}

/// Wraps SVG markup as a `data:` URL for an `<img src>`.
pub fn data_url(svg: &str) -> String {
    format!(
        "data:image/svg+xml;charset=utf-8,{}",
        String::from(js_sys::encode_uri_component(svg))
    )
}
