//! Application state and the actions that change it.
//!
//! Signals hold plain data only. Browser objects — the sprite cache, the backup
//! file handle — live in thread-locals instead, because they aren't `Send` and
//! would otherwise force every signal that touches them into local storage.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use jars_core::model::{Note, Jar, JarExport, Store};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::platform::{self, window};
use crate::scene::SharedCache;
use crate::sprites::Cache;
use crate::{backup, store};

/// How long after a change the jars are written to IndexedDB.
const SAVE_DEBOUNCE_MS: i32 = 400;
/// How long after a change the backup file is rewritten. Longer, because it
/// touches the user's filesystem and shouldn't thrash while they type.
const BACKUP_DEBOUNCE_MS: i32 = 10_000;
/// How long an undo stays offered.
pub const UNDO_MS: i32 = 5_000;

thread_local! {
    static CACHE: SharedCache = Rc::new(RefCell::new(Cache::default()));
    static BACKUP_HANDLE: RefCell<Option<JsValue>> = const { RefCell::new(None) };
    static TIMERS: RefCell<HashMap<&'static str, i32>> = RefCell::new(HashMap::new());
}

/// The one bitmap set every jar on screen draws from.
pub fn cache() -> SharedCache {
    CACHE.with(|c| c.clone())
}

/// Runs `f` after `ms`, cancelling any earlier pending call under the same key.
fn debounce(key: &'static str, ms: i32, f: impl FnOnce() + 'static) {
    TIMERS.with(|timers| {
        let mut timers = timers.borrow_mut();
        if let Some(old) = timers.remove(key) {
            window().clear_timeout_with_handle(old);
        }
        let cb = Closure::once_into_js(f);
        match window().set_timeout_with_callback_and_timeout_and_arguments_0(
            cb.unchecked_ref(),
            ms,
        ) {
            Ok(id) => {
                timers.insert(key, id);
            }
            Err(_) => platform::warn("couldn't schedule a save timer"),
        }
    });
}

#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Shelf,
    Jar(String),
    /// The notes binned out of one jar.
    Bin(String),
    /// Jars that have been deleted but not yet thrown out for good.
    Recycling,
    Settings,
}

impl Route {
    fn from_hash(hash: &str) -> Route {
        let path = hash.trim_start_matches('#').trim_start_matches('/');
        let mut parts = path.split('/');
        match (parts.next(), parts.next()) {
            (Some("jar"), Some(id)) if !id.is_empty() => Route::Jar(decode(id)),
            (Some("bin"), Some(id)) if !id.is_empty() => Route::Bin(decode(id)),
            (Some("recycling"), _) => Route::Recycling,
            (Some("settings"), _) => Route::Settings,
            _ => Route::Shelf,
        }
    }

    pub fn to_hash(&self) -> String {
        match self {
            Route::Shelf => "#/".into(),
            Route::Jar(id) => format!("#/jar/{}", encode(id)),
            Route::Bin(id) => format!("#/bin/{}", encode(id)),
            Route::Recycling => "#/recycling".into(),
            Route::Settings => "#/settings".into(),
        }
    }
}

fn encode(s: &str) -> String {
    String::from(js_sys::encode_uri_component(s))
}

fn decode(s: &str) -> String {
    js_sys::decode_uri_component(s).map(String::from).unwrap_or_else(|_| s.to_string())
}

/// A transient message, optionally offering to put a note back.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub message: String,
    pub undo: Option<Undo>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Undo {
    /// A note marked done. Nothing in the model remembers it, so the note
    /// itself rides along in the toast — this is the only copy.
    Done { jar: String, note: Note },
    /// A binned note, which the bin still holds.
    Trashed { jar: String, note_id: String },
    /// A whole jar in the recycling.
    RecycledJar { jar_id: String },
}

#[derive(Clone, Copy)]
pub struct App {
    pub store: RwSignal<Store>,
    /// False until the first load finishes, so an empty shelf isn't saved over
    /// real data.
    pub loaded: RwSignal<bool>,
    pub route: RwSignal<Route>,
    pub toast: RwSignal<Option<Toast>>,
    /// A problem worth a persistent banner, e.g. storage being unavailable.
    pub problem: RwSignal<Option<String>>,
    /// Whether a backup file has been nominated (Chrome/Edge only).
    pub backup_linked: RwSignal<bool>,
}

impl App {
    pub fn new() -> App {
        App {
            store: RwSignal::new(Store::default()),
            loaded: RwSignal::new(false),
            route: RwSignal::new(Route::from_hash(&current_hash())),
            toast: RwSignal::new(None),
            problem: RwSignal::new(None),
            backup_linked: RwSignal::new(false),
        }
    }

    /// Loads saved jars, asks for durable storage, and starts tracking the hash.
    pub fn boot(self) {
        self.watch_hash();

        wasm_bindgen_futures::spawn_local(async move {
            match store::load().await {
                Ok(loaded) => self.store.set(loaded),
                Err(store::StoreError::Corrupt(e)) => {
                    platform::warn(&format!("jars: saved data unreadable: {e}"));
                    // Keep going with an empty shelf rather than wedging. The
                    // unreadable record has already been set aside under its
                    // own key, so carrying on can't destroy it.
                    self.problem.set(Some(
                        "Your saved jars couldn't be read, so the shelf is starting empty. \
                         The unreadable data has been kept on this device \u{2014} nothing was deleted."
                            .into(),
                    ));
                }
                Err(e) => self.problem.set(Some(e.to_string())),
            }
            self.loaded.set(true);

            if let Some(handle) = store::load_backup_handle().await {
                BACKUP_HANDLE.with(|h| *h.borrow_mut() = Some(handle));
                self.backup_linked.set(true);
            }

            store::request_persistence().await;
        });
    }

    fn watch_hash(self) {
        let cb = Closure::<dyn FnMut()>::new(move || {
            self.route.set(Route::from_hash(&current_hash()));
        });
        window().set_onhashchange(Some(cb.as_ref().unchecked_ref()));
        // The listener must outlive this call; it lives as long as the page.
        cb.forget();
    }

    pub fn go(self, route: Route) {
        let _ = window().location().set_hash(&route.to_hash());
    }

    /// Changes the jars and schedules the writes that follow.
    pub fn mutate(self, f: impl FnOnce(&mut Store)) {
        self.store.update(f);
        self.persist();
    }

    fn persist(self) {
        if !self.loaded.get_untracked() {
            return;
        }
        let snapshot = self.store.get_untracked();
        debounce("save", SAVE_DEBOUNCE_MS, move || {
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = store::save(&snapshot).await {
                    platform::warn(&format!("save failed: {e}"));
                }
            });
        });

        if self.backup_linked.get_untracked() {
            debounce("backup", BACKUP_DEBOUNCE_MS, move || self.write_backup());
        }
    }

    /// Rewrites the nominated backup file in place.
    fn write_backup(self) {
        let Some(handle) = BACKUP_HANDLE.with(|h| h.borrow().clone()) else { return };
        let contents = self.store.get_untracked().to_json();

        wasm_bindgen_futures::spawn_local(async move {
            match backup::write_to_handle(&handle, &contents).await {
                Ok(()) => self.mark_backed_up(),
                Err(e) => {
                    // Permission can be withdrawn between sessions. Drop back
                    // to the manual path rather than failing silently forever.
                    platform::warn(&format!("backup write failed: {e:?}"));
                    BACKUP_HANDLE.with(|h| *h.borrow_mut() = None);
                    self.backup_linked.set(false);
                    self.problem.set(Some(
                        "Couldn't write the backup file. Re-link it in Settings.".into(),
                    ));
                }
            }
        });
    }

    fn mark_backed_up(self) {
        let now = platform::now_ms();
        self.store.update(|s| s.last_backup_at = Some(now));
        let snapshot = self.store.get_untracked();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = store::save(&snapshot).await;
        });
    }

    /// Asks the user to nominate a backup file, then writes it immediately.
    pub fn link_backup_file(self) {
        wasm_bindgen_futures::spawn_local(async move {
            match backup::choose_backup_file().await {
                Ok(handle) => {
                    if let Err(e) = store::save_backup_handle(&handle).await {
                        platform::warn(&format!("couldn't remember the backup file: {e}"));
                    }
                    BACKUP_HANDLE.with(|h| *h.borrow_mut() = Some(handle));
                    self.backup_linked.set(true);
                    self.write_backup();
                    self.say("Backup file linked. It'll stay up to date on its own.");
                }
                Err(e) if backup::is_abort(&e) => {}
                Err(e) => {
                    self.problem.set(Some(format!("Couldn't link that file: {e:?}")));
                }
            }
        });
    }

    /// Downloads everything, for the browsers that can't overwrite in place.
    pub fn download_backup(self) {
        let contents = self.store.get_untracked().to_json();
        match backup::download(backup::BACKUP_FILENAME, &contents) {
            Ok(()) => {
                self.mark_backed_up();
                self.say("Jars backed up.");
            }
            Err(e) => self.problem.set(Some(format!("Backup failed: {e:?}"))),
        }
    }

    pub fn export_jar(self, jar_id: &str) {
        let Some(jar) = self.store.get_untracked().jar(jar_id).cloned() else { return };
        let export = JarExport::wrap(&jar);
        match backup::download(&backup::export_filename(&jar.name), &export.to_json()) {
            Ok(()) => self.say(&format!("\u{201c}{}\u{201d} exported.", jar.name)),
            Err(e) => self.problem.set(Some(format!("Export failed: {e:?}"))),
        }
    }

    /// Reads a jar file and adds it to the shelf as a new jar.
    pub fn import_jar(self, file: web_sys::File) {
        wasm_bindgen_futures::spawn_local(async move {
            let raw = match backup::read_file(&file).await {
                Ok(t) => t,
                Err(e) => {
                    self.problem.set(Some(format!("Couldn't read that file: {e:?}")));
                    return;
                }
            };

            // Nothing is written unless the whole file parses, so a bad import
            // can't leave half a jar behind.
            match JarExport::from_json(&raw) {
                Ok(export) => {
                    let name = self.store.get_untracked().unique_name(&export.jar.name);
                    let mut jar = export.jar;
                    jar.id = platform::new_id();
                    jar.name = name.clone();
                    jar.draw_seed = platform::new_seed();
                    jar.created_at = platform::now_ms();
                    jar.hue = self.store.get_untracked().next_hue();
                    self.mutate(|s| s.jars.push(jar));
                    self.say(&format!("\u{201c}{name}\u{201d} added to your shelf."));
                }
                Err(e) => self.problem.set(Some(e.to_string())),
            }
        });
    }

    /// Creates a jar with a colour and pile chosen earlier.
    ///
    /// The new-jar flow shows the jar coming out of its box before you've
    /// named it, so its appearance has to be decided up front — and nothing is
    /// written to the shelf until you actually confirm.
    pub fn new_jar_with(self, name: &str, hue: u8, seed: u64) -> String {
        let id = platform::new_id();
        let store = self.store.get_untracked();
        let jar = Jar::new(id.clone(), store.unique_name(name), hue, platform::now_ms(), seed);
        self.mutate(|s| s.jars.push(jar));
        id
    }

    /// Shows a message with no undo attached.
    pub fn say(self, message: &str) {
        self.show_toast(Toast { message: message.to_string(), undo: None });
    }

    pub fn show_toast(self, toast: Toast) {
        self.toast.set(Some(toast.clone()));
        // Each toast cancels the previous timer, so a quick second action
        // doesn't have its undo cut short by the first one's countdown.
        debounce("toast", UNDO_MS, move || {
            if self.toast.get_untracked().as_ref() == Some(&toast) {
                self.toast.set(None);
            }
        });
    }

    pub fn undo(self, undo: Undo) {
        match undo {
            Undo::Done { jar, note } => self.mutate(|s| {
                if let Some(j) = s.jar_mut(&jar) {
                    j.restore_note(note);
                }
            }),
            Undo::Trashed { jar, note_id } => self.mutate(|s| {
                if let Some(j) = s.jar_mut(&jar) {
                    j.restore(&note_id);
                }
            }),
            Undo::RecycledJar { jar_id } => self.mutate(|s| {
                s.restore_jar(&jar_id);
            }),
        }
        self.toast.set(None);
    }

    /// Whether to nag about backing up.
    pub fn nudge_due(self) -> bool {
        backup::nudge_due(self.store.get().last_backup_at, platform::now_ms())
    }
}

fn current_hash() -> String {
    window().location().hash().unwrap_or_default()
}

/// Runs `f` once after `ms`. Not cancellable — for short animation handoffs.
pub fn after(ms: i32, f: impl FnOnce() + 'static) {
    let cb = Closure::once_into_js(f);
    if window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), ms)
        .is_err()
    {
        platform::warn("couldn't schedule a timer");
    }
}
