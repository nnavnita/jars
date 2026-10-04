//! The data the app owns, and the rules for changing it.

use serde::{Deserialize, Serialize};

/// Bumped whenever the on-disk shape changes in a way older builds can't read.
pub const SCHEMA_VERSION: u32 = 1;

/// A note is one line. Long enough for an idea, short enough to read unfolded.
pub const MAX_NOTE_LEN: usize = 140;

/// Jar colours available, as indices into the Sky Nursery palette.
pub const HUES: u8 = 6;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub text: String,
    pub added_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BinnedNote {
    pub note: Note,
    pub binned_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Jar {
    pub id: String,
    pub name: String,
    /// Index into the palette, always `< HUES`.
    pub hue: u8,
    pub created_at: i64,
    pub notes: Vec<Note>,
    #[serde(default)]
    pub bin: Vec<BinnedNote>,
    /// Keeps this jar's pile arrangement stable across reloads.
    pub draw_seed: u64,
    /// Last note drawn, so the next draw can avoid repeating it.
    #[serde(default)]
    pub last_drawn: Option<String>,
}

impl Jar {
    pub fn new(id: String, name: String, hue: u8, now: i64, seed: u64) -> Self {
        Jar {
            id,
            name: name.trim().to_string(),
            hue: hue % HUES,
            created_at: now,
            notes: Vec::new(),
            bin: Vec::new(),
            draw_seed: seed,
            last_drawn: None,
        }
    }

    /// Adds a note. Rejects blank text; trims and truncates the rest.
    ///
    /// Truncation counts characters, not bytes, so a jar full of emoji can't
    /// panic on a split in the middle of one.
    pub fn add_note(&mut self, id: String, text: &str, now: i64) -> Result<(), NoteError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(NoteError::Empty);
        }
        let text: String = trimmed.chars().take(MAX_NOTE_LEN).collect();
        self.notes.push(Note { id, text, added_at: now });
        Ok(())
    }

    fn take(&mut self, note_id: &str) -> Option<Note> {
        let i = self.notes.iter().position(|c| c.id == note_id)?;
        Some(self.notes.remove(i))
    }

    /// Marks a note done: removed outright, with no archive kept.
    ///
    /// Returns it so the UI can hold it for the undo toast.
    pub fn complete(&mut self, note_id: &str) -> Option<Note> {
        let note = self.take(note_id)?;
        self.clear_last_drawn(note_id);
        Some(note)
    }

    /// Moves a note to the bin, where it stays until the bin is emptied.
    pub fn bin_note(&mut self, note_id: &str, now: i64) -> Option<Note> {
        let note = self.take(note_id)?;
        self.clear_last_drawn(note_id);
        self.bin.push(BinnedNote { note: note.clone(), binned_at: now });
        Some(note)
    }

    /// Puts a binned note back in the jar.
    pub fn restore(&mut self, note_id: &str) -> bool {
        match self.bin.iter().position(|b| b.note.id == note_id) {
            Some(i) => {
                self.notes.push(self.bin.remove(i).note);
                true
            }
            None => false,
        }
    }

    /// Puts a note back directly, used to undo a `complete`.
    pub fn restore_note(&mut self, note: Note) {
        self.notes.push(note);
    }

    pub fn empty_bin(&mut self) {
        self.bin.clear();
    }

    /// Forgetting a removed note's id keeps `last_drawn` from blocking a draw
    /// forever once the note it points at no longer exists.
    fn clear_last_drawn(&mut self, note_id: &str) {
        if self.last_drawn.as_deref() == Some(note_id) {
            self.last_drawn = None;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteError {
    Empty,
}

impl core::fmt::Display for NoteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NoteError::Empty => write!(f, "a note needs some words on it"),
        }
    }
}

/// A deleted jar, kept until the recycling is emptied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecycledJar {
    pub jar: Jar,
    pub recycled_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Store {
    pub schema_version: u32,
    pub jars: Vec<Jar>,
    /// Deleting a jar throws away everything in it, so it goes here first.
    /// `serde(default)` keeps stores written before recycling existed readable.
    #[serde(default)]
    pub recycling: Vec<RecycledJar>,
    #[serde(default)]
    pub last_backup_at: Option<i64>,
}

impl Default for Store {
    fn default() -> Self {
        Store {
            schema_version: SCHEMA_VERSION,
            jars: Vec::new(),
            recycling: Vec::new(),
            last_backup_at: None,
        }
    }
}

impl Store {
    pub fn jar(&self, id: &str) -> Option<&Jar> {
        self.jars.iter().find(|j| j.id == id)
    }

    pub fn jar_mut(&mut self, id: &str) -> Option<&mut Jar> {
        self.jars.iter_mut().find(|j| j.id == id)
    }

    pub fn remove_jar(&mut self, id: &str) -> Option<Jar> {
        let i = self.jars.iter().position(|j| j.id == id)?;
        Some(self.jars.remove(i))
    }

    /// Moves a jar to the recycling, where it waits until that's emptied.
    pub fn recycle_jar(&mut self, id: &str, now: i64) -> bool {
        match self.remove_jar(id) {
            Some(jar) => {
                self.recycling.push(RecycledJar { jar, recycled_at: now });
                true
            }
            None => false,
        }
    }

    /// Puts a recycled jar back on the shelf.
    ///
    /// Its name and colour may both have been taken while it was away, so both
    /// are reassigned rather than allowed to collide.
    pub fn restore_jar(&mut self, id: &str) -> bool {
        let Some(i) = self.recycling.iter().position(|r| r.jar.id == id) else {
            return false;
        };
        let mut jar = self.recycling.remove(i).jar;
        jar.name = self.unique_name(&jar.name);
        if self.jars.iter().any(|j| j.hue == jar.hue) {
            jar.hue = self.next_hue();
        }
        self.jars.push(jar);
        true
    }

    pub fn empty_recycling(&mut self) {
        self.recycling.clear();
    }

    /// The hue for a new jar: the first one not already on the shelf, so six
    /// jars are six different colours before any colour repeats.
    pub fn next_hue(&self) -> u8 {
        let used: Vec<u8> = self.jars.iter().map(|j| j.hue).collect();
        (0..HUES).find(|h| !used.contains(h)).unwrap_or((self.jars.len() as u8) % HUES)
    }

    /// Picks a name that doesn't collide, appending `(2)`, `(3)` as needed.
    pub fn unique_name(&self, wanted: &str) -> String {
        let wanted = wanted.trim();
        if !self.jars.iter().any(|j| j.name == wanted) {
            return wanted.to_string();
        }
        (2..)
            .map(|n| format!("{wanted} ({n})"))
            .find(|cand| !self.jars.iter().any(|j| &j.name == cand))
            .expect("an unused suffix always exists")
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Store always serialises")
    }

    /// Reads a saved store, refusing anything written by a newer build.
    pub fn from_json(raw: &str) -> Result<Store, LoadError> {
        let probe: VersionProbe = serde_json::from_str(raw).map_err(|_| LoadError::Malformed)?;
        if probe.schema_version > SCHEMA_VERSION {
            return Err(LoadError::TooNew { found: probe.schema_version });
        }
        let mut store: Store = serde_json::from_str(raw).map_err(|_| LoadError::Malformed)?;
        store.schema_version = SCHEMA_VERSION;
        for jar in &mut store.jars {
            jar.hue %= HUES;
        }
        Ok(store)
    }
}

/// A single jar, as written by Export and read by Import.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JarExport {
    pub schema_version: u32,
    pub jar: Jar,
}

impl JarExport {
    pub fn wrap(jar: &Jar) -> Self {
        // A shared jar is a snapshot of the notes, not of someone's bin or
        // their draw history.
        let mut jar = jar.clone();
        jar.bin.clear();
        jar.last_drawn = None;
        JarExport { schema_version: SCHEMA_VERSION, jar }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("JarExport always serialises")
    }

    pub fn from_json(raw: &str) -> Result<JarExport, LoadError> {
        let probe: VersionProbe = serde_json::from_str(raw).map_err(|_| LoadError::Malformed)?;
        if probe.schema_version > SCHEMA_VERSION {
            return Err(LoadError::TooNew { found: probe.schema_version });
        }
        let mut export: JarExport =
            serde_json::from_str(raw).map_err(|_| LoadError::Malformed)?;
        export.schema_version = SCHEMA_VERSION;
        export.jar.hue %= HUES;
        Ok(export)
    }
}

/// Reads only the version, so a file from the future is rejected with a useful
/// message instead of a generic parse failure.
#[derive(Deserialize)]
struct VersionProbe {
    schema_version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    Malformed,
    TooNew { found: u32 },
}

impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            // Reached from both an import and a load from storage, so this
            // can't call the source a "file".
            LoadError::Malformed => write!(f, "that isn't valid jars data"),
            LoadError::TooNew { found } => write!(
                f,
                "that was made by a newer version of jars (v{found}) \u{2014} update first"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jar() -> Jar {
        Jar::new("j1".into(), "  date ideas  ".into(), 0, 1000, 7)
    }

    #[test]
    fn new_jar_trims_its_name() {
        assert_eq!(jar().name, "date ideas");
    }

    #[test]
    fn hue_always_in_palette() {
        assert!(Jar::new("j".into(), "n".into(), 200, 0, 0).hue < HUES);
    }

    #[test]
    fn add_note_trims_and_keeps_text() {
        let mut j = jar();
        j.add_note("c1".into(), "  picnic at the gardens \n", 1).unwrap();
        assert_eq!(j.notes[0].text, "picnic at the gardens");
    }

    #[test]
    fn blank_note_is_refused() {
        let mut j = jar();
        assert_eq!(j.add_note("c".into(), "   \n\t ", 1), Err(NoteError::Empty));
        assert!(j.is_empty());
    }

    #[test]
    fn long_note_is_truncated_by_characters() {
        let mut j = jar();
        let long = "\u{1f33b}".repeat(400);
        j.add_note("c".into(), &long, 1).unwrap();
        assert_eq!(j.notes[0].text.chars().count(), MAX_NOTE_LEN);
    }

    #[test]
    fn complete_removes_without_archiving() {
        let mut j = jar();
        j.add_note("c1".into(), "water the plants", 1).unwrap();
        let done = j.complete("c1").unwrap();
        assert_eq!(done.text, "water the plants");
        assert!(j.is_empty());
        assert!(j.bin.is_empty(), "done notes must not leave a history");
    }

    #[test]
    fn bin_then_restore_round_trips() {
        let mut j = jar();
        j.add_note("c1".into(), "regrout the bathroom", 1).unwrap();
        j.bin_note("c1", 2).unwrap();
        assert!(j.is_empty());
        assert_eq!(j.bin.len(), 1);
        assert!(j.restore("c1"));
        assert_eq!(j.len(), 1);
        assert!(j.bin.is_empty());
    }

    #[test]
    fn undo_of_done_puts_the_note_back() {
        let mut j = jar();
        j.add_note("c1".into(), "call mum", 1).unwrap();
        let note = j.complete("c1").unwrap();
        j.restore_note(note);
        assert_eq!(j.len(), 1);
    }

    #[test]
    fn removing_the_last_drawn_note_clears_the_marker() {
        let mut j = jar();
        j.add_note("c1".into(), "a", 1).unwrap();
        j.last_drawn = Some("c1".into());
        j.complete("c1");
        assert_eq!(j.last_drawn, None, "a stale marker would block future draws");
    }

    #[test]
    fn missing_note_is_a_no_op() {
        let mut j = jar();
        assert!(j.complete("nope").is_none());
        assert!(j.bin_note("nope", 1).is_none());
        assert!(!j.restore("nope"));
    }

    #[test]
    fn next_hue_fills_the_palette_before_repeating() {
        let mut s = Store::default();
        for i in 0..HUES {
            let h = s.next_hue();
            assert_eq!(h, i);
            s.jars.push(Jar::new(format!("j{i}"), format!("jar {i}"), h, 0, 0));
        }
        assert_eq!(s.next_hue(), 0, "a seventh jar wraps back to the first hue");
    }

    #[test]
    fn unique_name_suffixes_collisions() {
        let mut s = Store::default();
        assert_eq!(s.unique_name("date ideas"), "date ideas");
        s.jars.push(Jar::new("a".into(), "date ideas".into(), 0, 0, 0));
        assert_eq!(s.unique_name("date ideas"), "date ideas (2)");
        s.jars.push(Jar::new("b".into(), "date ideas (2)".into(), 1, 0, 0));
        assert_eq!(s.unique_name("date ideas"), "date ideas (3)");
    }

    #[test]
    fn recycling_a_jar_takes_it_off_the_shelf_but_keeps_it() {
        let mut s = Store::default();
        s.jars.push(jar());
        assert!(s.recycle_jar("j1", 500));
        assert!(s.jars.is_empty());
        assert_eq!(s.recycling.len(), 1);
        assert_eq!(s.recycling[0].recycled_at, 500);
        assert_eq!(s.recycling[0].jar.name, "date ideas");
    }

    #[test]
    fn recycling_keeps_the_notes_that_were_in_the_jar() {
        let mut s = Store::default();
        let mut j = jar();
        j.add_note("c1".into(), "pottery class", 1).unwrap();
        s.jars.push(j);
        s.recycle_jar("j1", 2);
        assert_eq!(s.recycling[0].jar.notes.len(), 1, "a recycled jar must keep its notes");
    }

    #[test]
    fn restoring_puts_the_jar_back() {
        let mut s = Store::default();
        s.jars.push(jar());
        s.recycle_jar("j1", 1);
        assert!(s.restore_jar("j1"));
        assert_eq!(s.jars.len(), 1);
        assert!(s.recycling.is_empty());
    }

    #[test]
    fn restoring_renames_when_the_name_was_taken_meanwhile() {
        let mut s = Store::default();
        s.jars.push(jar());
        s.recycle_jar("j1", 1);
        s.jars.push(Jar::new("j2".into(), "date ideas".into(), 1, 0, 0));
        s.restore_jar("j1");
        let names: Vec<&str> = s.jars.iter().map(|j| j.name.as_str()).collect();
        assert!(names.contains(&"date ideas (2)"), "got {names:?}");
    }

    #[test]
    fn restoring_recolours_when_the_hue_was_taken_meanwhile() {
        let mut s = Store::default();
        s.jars.push(Jar::new("j1".into(), "a".into(), 0, 0, 0));
        s.recycle_jar("j1", 1);
        s.jars.push(Jar::new("j2".into(), "b".into(), 0, 0, 0));
        s.restore_jar("j1");
        assert_ne!(s.jars[0].hue, s.jars[1].hue, "two jars ended up the same colour");
    }

    #[test]
    fn emptying_recycling_is_final() {
        let mut s = Store::default();
        s.jars.push(jar());
        s.recycle_jar("j1", 1);
        s.empty_recycling();
        assert!(s.recycling.is_empty());
        assert!(!s.restore_jar("j1"));
    }

    #[test]
    fn recycling_a_missing_jar_is_a_no_op() {
        let mut s = Store::default();
        assert!(!s.recycle_jar("nope", 1));
        assert!(!s.restore_jar("nope"));
    }

    #[test]
    fn a_recycled_jar_does_not_hold_its_colour() {
        // Its hue should be free for a new jar while it sits in the recycling.
        let mut s = Store::default();
        s.jars.push(Jar::new("j1".into(), "a".into(), 0, 0, 0));
        s.recycle_jar("j1", 1);
        assert_eq!(s.next_hue(), 0);
    }

    #[test]
    fn store_json_round_trips() {
        let mut s = Store::default();
        let mut j = jar();
        j.add_note("c1".into(), "pottery class", 1).unwrap();
        s.jars.push(j);
        let back = Store::from_json(&s.to_json()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn a_store_written_before_recycling_existed_still_loads() {
        let raw = r#"{"schema_version":1,"jars":[]}"#;
        let got = Store::from_json(raw).unwrap();
        assert!(got.recycling.is_empty());
    }

    #[test]
    fn recycling_survives_a_json_round_trip() {
        let mut s = Store::default();
        s.jars.push(jar());
        s.recycle_jar("j1", 99);
        assert_eq!(Store::from_json(&s.to_json()).unwrap(), s);
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert_eq!(Store::from_json("not json"), Err(LoadError::Malformed));
        assert_eq!(Store::from_json("{}"), Err(LoadError::Malformed));
    }

    #[test]
    fn store_from_the_future_is_refused() {
        let raw = r#"{"schema_version":99,"jars":[]}"#;
        assert_eq!(Store::from_json(raw), Err(LoadError::TooNew { found: 99 }));
    }

    #[test]
    fn export_drops_bin_and_draw_history() {
        let mut j = jar();
        j.add_note("c1".into(), "shared idea", 1).unwrap();
        j.add_note("c2".into(), "binned idea", 1).unwrap();
        j.bin_note("c2", 2);
        j.last_drawn = Some("c1".into());

        let export = JarExport::wrap(&j);
        assert_eq!(export.jar.notes.len(), 1);
        assert!(export.jar.bin.is_empty(), "someone else's bin is not shareable");
        assert_eq!(export.jar.last_drawn, None);
    }

    #[test]
    fn export_json_round_trips() {
        let mut j = jar();
        j.add_note("c1".into(), "sunset walk", 1).unwrap();
        let export = JarExport::wrap(&j);
        assert_eq!(JarExport::from_json(&export.to_json()).unwrap(), export);
    }

    #[test]
    fn import_of_a_newer_file_is_refused() {
        let raw = r#"{"schema_version":42,"jar":{"id":"j","name":"n","hue":0,"created_at":0,"notes":[],"draw_seed":0}}"#;
        assert_eq!(JarExport::from_json(raw), Err(LoadError::TooNew { found: 42 }));
    }

    #[test]
    fn import_tolerates_a_missing_bin_field() {
        let raw = r#"{"schema_version":1,"jar":{"id":"j","name":"n","hue":9,"created_at":0,"notes":[],"draw_seed":5}}"#;
        let got = JarExport::from_json(raw).unwrap();
        assert!(got.jar.bin.is_empty());
        assert!(got.jar.hue < HUES, "an out-of-range hue is clamped into the palette");
    }
}
