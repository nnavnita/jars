//! The Leptos component tree.
//!
//! Canvas is only used where something moves: the jar you're drawing from.
//! Shelf thumbnails are static `<img>` tags, so six jars on a shelf cost six
//! image decodes rather than six animation loops.

pub mod jar_view;
pub mod panels;

use jars_core::model::Jar;
use leptos::prelude::*;

use crate::state::{self, App, Route};
use crate::{palette, platform, sprites};

/// How long the box takes to open and the jar to rise clear of it. Once this
/// has passed the label is ready to be written on.
const BOX_MS: i32 = 980;

pub use jar_view::JarView;
pub use panels::{BinView, Problem, Recycling, Settings, ToastBar};

#[component]
pub fn Root() -> impl IntoView {
    let app = App::new();
    provide_context(app);
    app.boot();

    // The recycling bin repaints the whole page, and only the document element
    // reliably covers the viewport — a layer inside the page sits behind the
    // body's own background.
    Effect::new(move |_| {
        let room = match app.route.get() {
            Route::Recycling => "recycling",
            _ => "shelf",
        };
        if let Some(body) = platform::document().body() {
            let _ = body.set_attribute("data-room", room);
        }
    });

    view! {
        <Problem/>
        <main class="page">
            {move || match app.route.get() {
                Route::Shelf => view! { <Shelf/> }.into_any(),
                Route::Jar(id) => view! { <JarView jar_id=id/> }.into_any(),
                Route::Bin(id) => view! { <BinView jar_id=id/> }.into_any(),
                Route::Recycling => view! { <Recycling/> }.into_any(),
                Route::Settings => view! { <Settings/> }.into_any(),
            }}
        </main>
        <ToastBar/>
    }
}

/// A jar drawn as a static image, at whatever fill level it's currently at.
#[component]
pub fn JarThumb(jar: Jar) -> impl IntoView {
    let src = sprites::data_url(&sprites::jar_svg_full(jar.hue, jar.notes.len(), jar.draw_seed));
    // Naming the colour gives a screen reader something better than "image",
    // and it's how the jars differ at a glance for everyone else.
    let alt = format!("{} jar holding {}", palette::hue(jar.hue).name, note_count(jar.notes.len()));
    view! { <JarArt src=src alt=alt label=jar.name/> }
}

/// A jar image with its handwritten label laid over it.
///
/// The label is HTML rather than part of the SVG because an SVG rendered
/// through an `<img>` can't use the page's fonts — the handwriting would
/// quietly fall back to a default face.
#[component]
pub fn JarArt(
    src: String,
    #[prop(default = String::new())] alt: String,
    label: String,
) -> impl IntoView {
    view! {
        <span class="jarart">
            <img class="thumb" src=src alt=alt draggable="false"/>
            <Show when={
                let has = !label.trim().is_empty();
                move || has
            }>
                <span class="jarlabel">{label.clone()}</span>
            </Show>
        </span>
    }
}

#[component]
fn Shelf() -> impl IntoView {
    let app = expect_context::<App>();
    let name = RwSignal::new(String::new());
    let name_ref = NodeRef::<leptos::html::Input>::new();
    // The jar's colour and pile are decided before it's named, because you see
    // it come out of the box first. Nothing is saved until you confirm.
    let unboxing = RwSignal::new(None::<(u8, u64)>);
    // Flips once the jar is clear of the box and the label can be written on.
    let labelling = RwSignal::new(false);

    let open_box = move || {
        let hue = app.store.get_untracked().next_hue();
        unboxing.set(Some((hue, platform::new_seed())));
        labelling.set(false);
        state::after(BOX_MS, move || labelling.set(true));
    };

    // Focus the label only once it's actually out of the box.
    Effect::new(move |_| {
        if labelling.get() {
            if let Some(el) = name_ref.get() {
                let _ = el.focus();
            }
        }
    });

    let close_box = move || {
        unboxing.set(None);
        labelling.set(false);
        name.set(String::new());
    };

    let create = move || {
        let Some((hue, seed)) = unboxing.get_untracked() else { return };
        let wanted = name.get_untracked();
        let wanted = if wanted.trim().is_empty() { "new jar".to_string() } else { wanted };
        let id = app.new_jar_with(&wanted, hue, seed);
        close_box();
        app.go(Route::Jar(id));
    };

    view! {
        <Show when=move || unboxing.get().is_some()>
            <div class="boxscene opening">
                <div class="box">
                    <div class="flap left"></div>
                    <div class="flap right"></div>
                    {move || {
                        let (hue, seed) = unboxing.get().unwrap_or((0, 1));
                        let src = sprites::data_url(&sprites::jar_svg_full(hue, 0, seed));
                        view! {
                            <span class="newjar">
                                <span class="jarart">
                                    <img class="thumb" src=src alt="" draggable="false"/>
                                    <span class="jarlabel">
                                        <input
                                            class="scribble"
                                            node_ref=name_ref
                                            placeholder="label me"
                                            prop:value=move || name.get()
                                            on:input=move |ev| name.set(event_target_value(&ev))
                                            on:keydown=move |ev| {
                                                match ev.key().as_str() {
                                                    "Enter" => create(),
                                                    "Escape" => close_box(),
                                                    _ => {}
                                                }
                                            }
                                        />
                                    </span>
                                </span>
                            </span>
                        }
                    }}
                    <div class="boxbody"></div>
                </div>

                <div class="boxask" class:ready=move || labelling.get()>
                    <p class="boxhint">"Write on the label."</p>
                    <div class="actions">
                        <button class="primary" on:click=move |_| create()>"Done"</button>
                        <button class="ghost" on:click=move |_| close_box()>"Never mind"</button>
                    </div>
                </div>
            </div>
        </Show>

        <header class="bar">
            <h1>jars</h1>
            <button class="ghost" on:click=move |_| app.go(Route::Settings)>settings</button>
        </header>

        <Show when=move || app.nudge_due() && !app.store.get().jars.is_empty()>
            <div class="nudge">
                <span>"It's been a while since you backed up your jars."</span>
                <button class="link" on:click=move |_| app.go(Route::Settings)>"Back up"</button>
            </div>
        </Show>

        {move || {
            let jars = app.store.get().jars;
            if jars.is_empty() && app.loaded.get() {
                view! {
                    <div class="empty">
                        <p class="big">"No jars yet."</p>
                        <p>"A jar holds folded notes. When you want something to do, you draw one at random \u{2014} without reading the rest."</p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <ul class="shelf">
                        {jars.into_iter().map(|jar| {
                            let id = jar.id.clone();
                            let count = jar.notes.len();
                            let label = jar.name.clone();
                            view! {
                                <li>
                                    <button
                                        class="jarcard"
                                        title=label.clone()
                                        aria-label=format!("{label}, {}", note_count(count))
                                        on:click=move |_| app.go(Route::Jar(id.clone()))
                                    >
                                        <JarThumb jar=jar/>
                                        <span class="jarcount">{note_count(count)}</span>
                                    </button>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                }.into_any()
            }
        }}

        <Show when=move || !app.store.get().recycling.is_empty()>
            <p class="shelffoot">
                <button class="link" on:click=move |_| app.go(Route::Recycling)>
                    {move || format!("recycling bin ({})", app.store.get().recycling.len())}
                </button>
            </p>
        </Show>

        <div class="actions">
            <button class="primary" on:click=move |_| open_box()>"New jar"</button>
            <ImportButton/>
        </div>
    }
}

/// Reads a `.json` jar file and adds it to the shelf.
#[component]
pub fn ImportButton() -> impl IntoView {
    let app = expect_context::<App>();
    view! {
        <label class="ghost file">
            "Import a jar"
            <input
                type="file"
                accept="application/json,.json"
                on:change=move |ev| {
                    let input: web_sys::HtmlInputElement = event_target(&ev);
                    if let Some(file) = input.files().and_then(|f| f.get(0)) {
                        app.import_jar(file);
                    }
                    // Clearing the value lets the same file be picked twice.
                    input.set_value("");
                }
            />
        </label>
    }
}

pub fn note_count(n: usize) -> String {
    match n {
        0 => "empty".to_string(),
        1 => "1 note".to_string(),
        n => format!("{n} notes"),
    }
}
