# jars

Browser app: a todo list where each list is a jar and each item is a folded note
of paper you draw at random. Rust → WebAssembly, hosted on GitHub Pages at
`nnavnita.com/jars`.

Vocabulary matters here and is used consistently in code and copy: a **jar**
holds **notes**. Not "chits" (not universal English), not "slips", not "items".
Notes go in a jar's **bin**; whole jars go in the **recycling bin**.

## Stack

- **`core/`** (`jars-core`) — model, random draw, deterministic pile layout.
  Pure Rust, no browser dependencies, so `cargo test -p jars-core` runs on the
  host with no headless browser. Put logic here whenever it can live here.
- **`web/`** (`jars-web`) — Leptos 0.8 (CSR) for the interface, Canvas2D for the
  jar, `rexie` over IndexedDB for storage.

## Setup

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk

cargo test -p jars-core
cd web && trunk serve          # http://localhost:8080
trunk build --release --public-url /jars/   # what CI deploys
```

## Things that will bite you

- **`--public-url /jars/` is required for the real deploy.** Pages serves the
  repo under a subdirectory; without it every asset 404s.
- **Routing is hash-based** (`#/jar/<id>`) specifically so deep links don't 404
  on a static host. Don't swap in a history router.
- **Sprites are rasterized once per hue and blitted.** Parsing and filtering SVG
  is slow, copying a bitmap is not. Never generate SVG inside a frame loop.
- **A drawn note stays in the model** while it's being shown; only the renderer
  hides it, via `held`. Removing it up front and re-adding on "back in jar"
  would silently drop a note on any reload mid-reveal.
- **The pile is seeded per jar** (`draw_seed`). Fresh randomness each frame would
  make the pile twitch on every repaint.
- **The service worker is skipped on localhost**, or development serves stale
  wasm and every change looks like it didn't apply.
- **Animations stall in a background tab** because browsers suspend
  `requestAnimationFrame` there. They resume and settle on return; this is not a
  bug, and it makes browser automation of the draw ritual awkward.
- **No dark theme on purpose.** The pastel palette is the identity; inverted it
  reads as a different app.
- **The type is Reenie Beanie, bundled** (`web/assets/`, Latin subset, OFL). It
  has a small x-height and thin strokes, so every size in `styles.css` is set
  noticeably larger than it would be in a UI sans. Don't "fix" those sizes.
- **Crushing a note animates `clip-path`, not `transform`.** A transform is
  affine and can only squash a rectangle smoothly; moving polygon vertices is
  what makes the outline buckle like paper. Keep the vertex count matched across
  keyframes or it won't interpolate at all.
- **Deleting a jar is recoverable.** It goes to `Store::recycling`, because
  deleting a jar takes every note in it. The bin has its own green room, painted
  via `body[data-room]` — a layer inside the page sits behind the body's own
  background and won't cover the viewport.
- **A jar wears its name on a paper label.** On the shelf that label is HTML
  over the `<img>`; in the jar view it's drawn on the canvas with `fillText`.
  It is deliberately *not* baked into the SVG: an SVG rendered through an
  `<img>` can't reach the page's fonts, so the handwriting would silently fall
  back to a default face.
- **Label width is a percentage of `.jarart`, not the `<img>`.** The jar's body
  spans about 26.5%–73.5% of the artwork, so the wrapper carries the size cap;
  cap the image instead and the label hangs off the glass.
- **The shelf plank is painted on the `<ul>`**, not per cell, so it spans the
  full width even when a row holds one jar. That needs fixed `grid-auto-rows`.

## Design decisions already settled

Local-only, no accounts, no server — a deliberate trade for zero hosting and
zero privacy surface, accepting that cleared browser data is unrecoverable.
Sharing is export/import of a snapshot file. See
`docs/superpowers/specs/` (gitignored) for the full design record.

## Roadmap

`bullseye.yaml` — managed by the bullseye MCP server. Use `bullseye_frontier`
(cwd = this repo) for what's next.
