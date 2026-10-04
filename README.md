# jars

A todo list you don't get to choose from.

Each list is a **jar**. Each item is a folded **note** of paper inside it. The
notes are folded, so you can't read them — when you want something to do, you
draw one at random and either put it back, mark it done, or bin it. Jars empty
visibly as you work through them.

Built for the lists where choosing is the hard part: date ideas, odd jobs around
the house, things to make, conversation starters.

A new jar arrives in a cardboard box: the flaps open, the jar comes out, and you
write its name on the paper label stuck to it. Deleting a jar lobs it into the
recycling bin, where it waits — notes and all — until you empty it.

**Live at [nnavnita.com/jars](https://nnavnita.com/jars)**

## How it works

Rust compiled to WebAssembly. [Leptos](https://leptos.dev) renders the interface;
the jar itself is drawn to a canvas.

The watercolour look is procedural, with no art assets. Each jar is generated as
an SVG string using `feTurbulence` for paper grain and `feDisplacementMap` for
the hand-drawn wobble, rasterized **once** into a bitmap, then blitted every
frame. Recolouring a jar is a palette edit, not new artwork.

Crushing a note into a ball animates `clip-path` rather than `transform`:
a transform can only squash a rectangle smoothly, which reads as a shrinking
card, whereas moving each polygon vertex independently makes the silhouette
buckle the way paper actually does.

## Your data

Everything lives in your browser on your device. Nothing is uploaded anywhere,
and there are no accounts — which also means nothing can be recovered for you if
the browser's data is cleared, so back up from Settings.

Chrome and Edge can keep one backup file up to date automatically via the File
System Access API. Safari and Firefox don't offer that API, so backups there are
a one-tap manual export.

Jars can be exported to a file and imported by someone else, which is how you
share one. The copy is a snapshot; the two diverge afterwards.

## Developing

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk

cargo test -p jars-core   # pure logic, no browser needed
cd web && trunk serve     # http://localhost:8080
```

## Layout

| Path | What's in it |
|---|---|
| `core/` | `jars-core` — model, random draw, pile layout. Pure Rust, no browser. |
| `web/` | `jars-web` — Leptos UI, canvas renderer, IndexedDB, backups. |
| `web/assets/` | Icons, manifest, service worker. |

## Licence

MIT. See `LICENSE.txt`.

Set in [Reenie Beanie](https://fonts.google.com/specimen/Reenie+Beanie) by James
Grieshaber, under the SIL Open Font License 1.1. The Latin subset is bundled and
served from this origin — no third-party requests at runtime — with the licence
in `web/assets/ReenieBeanie-OFL.txt`.
