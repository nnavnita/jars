//! The bin, settings, the toast, and the problem banner.

use leptos::prelude::*;

use crate::state::{App, Route};
use crate::{backup, ui};
use crate::ui::JarThumb;

/// Trashed notes, recoverable until the bin is emptied.
#[component]
pub fn BinView(jar_id: String) -> impl IntoView {
    let app = expect_context::<App>();
    let jar_id = RwSignal::new(jar_id);
    let jar = move || app.store.get().jar(&jar_id.get()).cloned();

    view! {
        <header class="bar">
            <button class="ghost" on:click=move |_| app.go(Route::Jar(jar_id.get_untracked()))>
                "\u{2190} jar"
            </button>
            <h1 class="jartitle">"bin"</h1>
            <span class="spacer"></span>
        </header>

        {move || {
            let Some(j) = jar() else {
                return view! { <div class="empty"><p class="big">"That jar isn't here any more."</p></div> }.into_any();
            };
            if j.bin.is_empty() {
                return view! {
                    <div class="empty"><p class="big">"Nothing in the bin."</p></div>
                }.into_any();
            }
            let id = j.id.clone();
            view! {
                <ul class="binlist">
                    {j.bin.into_iter().map(|b| {
                        let note_id = b.note.id.clone();
                        let id = id.clone();
                        view! {
                            <li>
                                <span class="bintext">{b.note.text}</span>
                                <button class="link" on:click=move |_| {
                                    let (id, note_id) = (id.clone(), note_id.clone());
                                    app.mutate(move |s| { if let Some(j) = s.jar_mut(&id) { j.restore(&note_id); } });
                                    app.say("Back in the jar.");
                                }>"Put back"</button>
                            </li>
                        }
                    }).collect_view()}
                </ul>
                <div class="actions">
                    <button class="ghost danger" on:click=move |_| {
                        let id = jar_id.get_untracked();
                        app.mutate(move |s| { if let Some(j) = s.jar_mut(&id) { j.empty_bin(); } });
                        app.say("Bin emptied.");
                    }>"Empty the bin"</button>
                </div>
            }.into_any()
        }}
    }
}

/// Jars that have been deleted but not yet thrown out for good.
#[component]
pub fn Recycling() -> impl IntoView {
    let app = expect_context::<App>();

    view! {
        // The bin is its own place, not a shelf — it takes the bin's colours.
        <div class="recycleroom">
        <header class="bar">
            <button class="ghost" on:click=move |_| app.go(Route::Shelf)>"\u{2190} shelf"</button>
            <h1 class="jartitle">"recycling bin"</h1>
            <span class="spacer"></span>
        </header>

        {move || {
            let recycled = app.store.get().recycling;
            if recycled.is_empty() {
                return view! {
                    <div class="empty"><p class="big">"Nothing in the recycling bin."</p></div>
                }.into_any();
            }
            view! {
                <p class="muted center">
                    "These jars are still here, notes and all, until you empty the recycling bin."
                </p>
                <ul class="shelf bare">
                    {recycled.into_iter().map(|r| {
                        let id = r.jar.id.clone();
                        let label = r.jar.name.clone();
                        let count = r.jar.notes.len();
                        view! {
                            <li>
                                <div class="jarcard recycled">
                                    <JarThumb jar=r.jar/>
                                    <span class="jarname">{label}</span>
                                    <span class="jarcount">{ui::note_count(count)}</span>
                                    <button class="link" on:click=move |_| {
                                        let id = id.clone();
                                        app.mutate(move |s| { s.restore_jar(&id); });
                                        app.say("Back on the shelf.");
                                    }>"Put back"</button>
                                </div>
                            </li>
                        }
                    }).collect_view()}
                </ul>
                <div class="actions">
                    <button class="ghost danger" on:click=move |_| {
                        app.mutate(|s| s.empty_recycling());
                        app.say("Recycling bin emptied.");
                    }>"Empty the recycling bin"</button>
                </div>
            }.into_any()
        }}
        </div>
    }
}

#[component]
pub fn Settings() -> impl IntoView {
    let app = expect_context::<App>();
    let silent = backup::silent_overwrite_supported();

    view! {
        <header class="bar">
            <button class="ghost" on:click=move |_| app.go(Route::Shelf)>"\u{2190} shelf"</button>
            <h1 class="jartitle">"settings"</h1>
            <span class="spacer"></span>
        </header>

        <section class="panel">
            <h2>"Backups"</h2>
            <p class="muted">
                "Your jars live in this browser on this device. Nothing is uploaded anywhere, which also means nothing can be recovered for you if the browser's data is cleared."
            </p>

            {if silent {
                view! {
                    <p class="muted">
                        "This browser can keep a backup file up to date on its own. Pick a file once and it gets rewritten in place after every change \u{2014} one file, never a pile of them."
                    </p>
                    <div class="actions">
                        <button class="primary" on:click=move |_| app.link_backup_file()>
                            {move || if app.backup_linked.get() { "Change backup file" } else { "Choose backup file" }}
                        </button>
                    </div>
                    <Show when=move || app.backup_linked.get()>
                        <p class="ok">"Linked. Saved automatically."</p>
                    </Show>
                }.into_any()
            } else {
                view! {
                    <p class="muted">
                        "This browser can't let a page overwrite a file it saved earlier, so backups here are manual \u{2014} one tap, whenever you think of it. Installing jars to your home screen also stops the browser evicting your jars if you don't open it for a while."
                    </p>
                    <div class="actions">
                        <button class="primary" on:click=move |_| app.download_backup()>"Back up now"</button>
                    </div>
                }.into_any()
            }}

            <p class="muted small">
                {move || match app.store.get().last_backup_at {
                    Some(_) => "Last backed up recently.".to_string(),
                    None => "Not backed up yet.".to_string(),
                }}
            </p>
        </section>

        <section class="panel">
            <h2>"Jars"</h2>
            <div class="actions"><ui::ImportButton/></div>
        </section>

        <section class="panel">
            <h2>"About"</h2>
            <p class="muted">
                "Jars holds ideas you don't want to have to choose between \u{2014} date nights, odd jobs, things to make, things to ask. Fold them in, then let the jar decide."
            </p>
        </section>
    }
}

/// A persistent banner for things the user needs to know about.
#[component]
pub fn Problem() -> impl IntoView {
    let app = expect_context::<App>();
    view! {
        <Show when=move || app.problem.get().is_some()>
            <div class="problem" role="alert">
                <span>{move || app.problem.get().unwrap_or_default()}</span>
                <button class="link" on:click=move |_| app.problem.set(None)>"Dismiss"</button>
            </div>
        </Show>
    }
}

/// Short-lived confirmation, sometimes with an undo.
#[component]
pub fn ToastBar() -> impl IntoView {
    let app = expect_context::<App>();
    view! {
        <Show when=move || app.toast.get().is_some()>
            <div class="toast" role="status">
                <span>{move || app.toast.get().map(|t| t.message).unwrap_or_default()}</span>
                <Show when=move || app.toast.get().and_then(|t| t.undo).is_some()>
                    <button class="link" on:click=move |_| {
                        if let Some(u) = app.toast.get_untracked().and_then(|t| t.undo) {
                            app.undo(u);
                        }
                    }>"Undo"</button>
                </Show>
            </div>
        </Show>
    }
}
