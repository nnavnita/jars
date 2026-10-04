//! One jar, and the ritual of drawing from it.
//!
//! The note stays in the model the whole time it's being shown. Only the
//! renderer is told to hide it, via `held` — so closing the tab mid-reveal
//! loses nothing, where removing it up front and adding it back on "put it
//! back" would quietly drop a note on any reload.

use std::cell::{Cell, RefCell};

use jars_core::draw;
use jars_core::model::Note;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::scene::{Finished, Scene};
use crate::state::{self, App, Route, Toast, Undo};
use crate::{palette, platform, ui};

/// How long each outcome's animation plays before the jar actually changes.
/// Done is the longest because the stamp has to land and be read.
const EXIT_DONE_MS: i32 = 720;
const EXIT_TRASH_MS: i32 = 620;

/// How long the jar takes to arc into the recycling bin.
const THROW_MS: i32 = 940;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Exit {
    Done,
    Trash,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Idle,
    /// The jar is leaning over, about to let a note out.
    Tilting,
    /// A note is flying up out of the jar.
    Flying,
    /// The note is unfolded and the three choices are showing.
    Revealed,
    /// The note is being dropped back in.
    Returning,
    /// The note is dissolving or crumpling away.
    Leaving(Exit),
}

impl Stage {
    /// True when a note is out of the pile and in the user's hand.
    fn in_hand(self) -> bool {
        matches!(self, Stage::Revealed | Stage::Returning | Stage::Leaving(_))
    }
}

thread_local! {
    /// Only one jar is ever on screen, so one slot is enough.
    static SCENE: RefCell<Option<Scene>> = const { RefCell::new(None) };
    /// Cleared on unmount so the frame loop stops.
    static ALIVE: Cell<bool> = const { Cell::new(false) };
    /// Holds the frame callback. Replacing it drops the previous one, which
    /// keeps remounting from stacking up loops.
    static FRAME: RefCell<Option<Closure<dyn FnMut()>>> = const { RefCell::new(None) };
}

fn with_scene<R>(f: impl FnOnce(&mut Scene) -> R) -> Option<R> {
    SCENE.with(|s| s.borrow_mut().as_mut().map(f))
}

fn request_frame() {
    FRAME.with(|frame| {
        if let Some(cb) = frame.borrow().as_ref() {
            let _ = platform::window().request_animation_frame(cb.as_ref().unchecked_ref());
        }
    });
}

#[component]
pub fn JarView(jar_id: String) -> impl IntoView {
    let app = expect_context::<App>();
    let jar_id = RwSignal::new(jar_id);
    let stage = RwSignal::new(Stage::Idle);
    let in_hand = RwSignal::new(None::<Note>);
    let adding = RwSignal::new(String::new());
    let confirming_delete = RwSignal::new(false);
    // Holds the jar's art while it's being lobbed into the bin.
    let throwing = RwSignal::new(None::<(u8, usize, u64)>);
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();

    let jar = move || app.store.get().jar(&jar_id.get()).cloned();

    // Build the scene once the canvas exists, and run a frame loop for as long
    // as this view is mounted.
    Effect::new(move |_| {
        let Some(canvas) = canvas_ref.get() else { return };
        SCENE.with(|s| *s.borrow_mut() = Scene::new(canvas, state::cache()));
        ALIVE.with(|a| a.set(true));

        FRAME.with(|frame| {
            *frame.borrow_mut() = Some(Closure::new(move || {
                if !ALIVE.with(|a| a.get()) {
                    return;
                }
                let finished = with_scene(|sc| sc.tick(platform::now_frame())).and_then(|t| t.finished);
                if let Some(f) = finished {
                    advance(app, jar_id, stage, in_hand, f);
                }
                request_frame();
            }));
        });
        request_frame();
    });

    on_cleanup(|| {
        ALIVE.with(|a| a.set(false));
        SCENE.with(|s| *s.borrow_mut() = None);
    });

    // Keep the renderer in step with the model.
    Effect::new(move |_| {
        if let Some(j) = jar() {
            let held = stage.get().in_hand();
            with_scene(|sc| sc.set_jar(j.hue, j.draw_seed, j.notes.len(), held, &j.name));
        }
    });

    let pull = move || {
        let Some(j) = jar() else { return };
        if stage.get_untracked() != Stage::Idle || j.notes.is_empty() {
            return;
        }
        let Some(note) = draw::pick(&j.notes, j.last_drawn.as_deref(), platform::random()).cloned()
        else {
            return;
        };

        let id = jar_id.get_untracked();
        let drawn_id = note.id.clone();
        in_hand.set(Some(note));
        app.mutate(move |s| {
            if let Some(j) = s.jar_mut(&id) {
                j.last_drawn = Some(drawn_id);
            }
        });
        stage.set(Stage::Tilting);
        with_scene(|sc| sc.tilt(platform::now_frame()));
    };

    let put_back = move || {
        stage.set(Stage::Returning);
        with_scene(|sc| sc.fly_in(platform::now_frame()));
    };

    let resolve = move |exit: Exit| {
        let Some(note) = in_hand.get_untracked() else { return };
        stage.set(Stage::Leaving(exit));

        // Let the animation play out before the jar actually changes, so the
        // note isn't yanked out from under the thing animating it.
        let settle = match exit {
            Exit::Done => EXIT_DONE_MS,
            Exit::Trash => EXIT_TRASH_MS,
        };
        state::after(settle, move || {
            let id = jar_id.get_untracked();
            let note_id = note.id.clone();
            match exit {
                Exit::Done => {
                    let for_undo = note.clone();
                    app.mutate({
                        let id = id.clone();
                        let note_id = note_id.clone();
                        move |s| {
                            if let Some(j) = s.jar_mut(&id) {
                                j.complete(&note_id);
                            }
                        }
                    });
                    app.show_toast(Toast {
                        message: "Done.".into(),
                        // A done note is kept nowhere else, so the undo holds
                        // the only remaining copy of it.
                        undo: Some(Undo::Done { jar: id, note: for_undo }),
                    });
                }
                Exit::Trash => {
                    let now = platform::now_ms();
                    app.mutate({
                        let id = id.clone();
                        let note_id = note_id.clone();
                        move |s| {
                            if let Some(j) = s.jar_mut(&id) {
                                j.bin_note(&note_id, now);
                            }
                        }
                    });
                    app.show_toast(Toast {
                        message: "Moved to the bin.".into(),
                        undo: Some(Undo::Trashed { jar: id, note_id }),
                    });
                }
            }
            in_hand.set(None);
            stage.set(Stage::Idle);
        });
    };

    let add = move || {
        let text = adding.get_untracked();
        if text.trim().is_empty() {
            return;
        }
        let id = jar_id.get_untracked();
        let note_id = platform::new_id();
        let now = platform::now_ms();
        app.mutate(move |s| {
            if let Some(j) = s.jar_mut(&id) {
                // The only error is empty text, already ruled out above.
                let _ = j.add_note(note_id, &text, now);
            }
        });
        adding.set(String::new());
        with_scene(|sc| sc.fly_in(platform::now_frame()));
    };

    view! {
        <header class="bar">
            <button class="ghost" on:click=move |_| app.go(Route::Shelf)>"\u{2190} shelf"</button>
            <h1 class="jartitle">{move || jar().map(|j| j.name).unwrap_or_default()}</h1>
            <button
                class="ghost"
                on:click=move |_| app.go(Route::Bin(jar_id.get_untracked()))
            >
                "bin"
            </button>
        </header>

        <Show
            when=move || jar().is_some()
            fallback=move || view! {
                <div class="empty"><p class="big">"That jar isn't here any more."</p></div>
            }
        >
            <div class="jarstage">
                <canvas
                    node_ref=canvas_ref
                    class="jarcanvas"
                    role="img"
                    aria-label=move || jar()
                        .map(|j| format!(
                            "{} jar holding {}",
                            palette::hue(j.hue).name,
                            ui::note_count(j.notes.len()),
                        ))
                        .unwrap_or_default()
                ></canvas>
                <p class="count">{move || ui::note_count(jar().map(|j| j.notes.len()).unwrap_or(0))}</p>
            </div>

            <div class="actions">
                <button
                    class="primary big"
                    disabled=move || {
                        stage.get() != Stage::Idle || jar().map(|j| j.notes.is_empty()).unwrap_or(true)
                    }
                    on:click=move |_| pull()
                >
                    {move || if jar().map(|j| j.notes.is_empty()).unwrap_or(true) {
                        "Jar's empty"
                    } else {
                        "Pick a note"
                    }}
                </button>
            </div>

            <div class="addrow">
                <input
                    class="field"
                    placeholder="write a note\u{2026}"
                    maxlength=jars_core::MAX_NOTE_LEN.to_string()
                    prop:value=move || adding.get()
                    on:input=move |ev| adding.set(event_target_value(&ev))
                    on:keydown=move |ev| if ev.key() == "Enter" { add() }
                />
                <button class="ghost" on:click=move |_| add()>"Fold it in"</button>
            </div>

            <div class="jarmenu">
                <button class="link" on:click=move |_| app.export_jar(&jar_id.get_untracked())>
                    "Export this jar"
                </button>
                <Show
                    when=move || confirming_delete.get()
                    fallback=move || view! {
                        <button class="link danger" on:click=move |_| confirming_delete.set(true)>
                            "Delete jar"
                        </button>
                    }
                >
                    <span class="confirm">
                        {move || format!(
                            "Move \u{201c}{}\u{201d} and its {} to the recycling bin?",
                            jar().map(|j| j.name).unwrap_or_default(),
                            ui::note_count(jar().map(|j| j.notes.len()).unwrap_or(0)),
                        )}
                        <button class="link danger" on:click=move |_| {
                            let Some(j) = jar() else { return };
                            confirming_delete.set(false);
                            throwing.set(Some((j.hue, j.notes.len(), j.draw_seed)));

                            // The jar is still on the shelf until it lands, so
                            // that the thing being thrown is the thing shown.
                            state::after(THROW_MS, move || {
                                let id = jar_id.get_untracked();
                                let now = platform::now_ms();
                                app.mutate({
                                    let id = id.clone();
                                    move |s| { s.recycle_jar(&id, now); }
                                });
                                // Deleting a jar takes every note in it, so this
                                // is recoverable rather than final.
                                app.show_toast(Toast {
                                    message: "Jar thrown in the recycling bin.".into(),
                                    undo: Some(Undo::RecycledJar { jar_id: id }),
                                });
                                throwing.set(None);
                                app.go(Route::Shelf);
                            });
                        }>"Yes, recycle"</button>
                        <button class="link" on:click=move |_| confirming_delete.set(false)>"Keep it"</button>
                    </span>
                </Show>
            </div>
        </Show>

        <Show when=move || throwing.get().is_some()>
            {move || {
                let (hue, notes, seed) = throwing.get().unwrap_or((0, 0, 1));
                let src = crate::sprites::data_url(&crate::sprites::jar_svg_full(hue, notes, seed));
                let label = jar().map(|j| j.name).unwrap_or_default();
                view! {
                    <div class="throwscene" aria-hidden="true">
                        <span class="flyingjar">
                            <ui::JarArt src=src label=label/>
                        </span>
                        <div class="bin">
                            <div class="binlid"></div>
                            <div class="binbody"><span class="recycle">"\u{267B}"</span></div>
                        </div>
                    </div>
                }
            }}
        </Show>

        <Show when=move || stage.get().in_hand()>
            <div class="reveal-backdrop">
                <div
                    class="reveal"
                    class:going=move || matches!(stage.get(), Stage::Leaving(_) | Stage::Returning)
                    class:back=move || stage.get() == Stage::Returning
                    class:done=move || stage.get() == Stage::Leaving(Exit::Done)
                    class:trash=move || stage.get() == Stage::Leaving(Exit::Trash)
                >
                    <p class="notetext">{move || in_hand.get().map(|c| c.text).unwrap_or_default()}</p>
                    <Show when=move || stage.get() == Stage::Leaving(Exit::Done)>
                        <span class="stamp" aria-hidden="true">"DONE"</span>
                    </Show>
                    <Show when=move || stage.get() == Stage::Revealed>
                        <div class="choices">
                            <button class="ghost" on:click=move |_| put_back()>"Back in jar"</button>
                            <button class="primary" on:click=move |_| resolve(Exit::Done)>"Done"</button>
                            <button class="ghost danger" on:click=move |_| resolve(Exit::Trash)>"Trash"</button>
                        </div>
                    </Show>
                </div>
            </div>
        </Show>
    }
}

/// Moves the ritual on when an animation completes.
fn advance(
    _app: App,
    _jar_id: RwSignal<String>,
    stage: RwSignal<Stage>,
    in_hand: RwSignal<Option<Note>>,
    finished: Finished,
) {
    match finished {
        Finished::Tilt => {
            if stage.get_untracked() == Stage::Tilting {
                stage.set(Stage::Flying);
                with_scene(|sc| sc.fly_out(platform::now_frame()));
            }
        }
        Finished::FlyOut => {
            if stage.get_untracked() == Stage::Flying {
                stage.set(Stage::Revealed);
            }
        }
        Finished::FlyIn => {
            // A note being added also flies in, and that must not end a reveal
            // that isn't happening.
            if stage.get_untracked() == Stage::Returning {
                stage.set(Stage::Idle);
                in_hand.set(None);
            }
        }
    }
}
