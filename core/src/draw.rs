//! Choosing a note at random.
//!
//! The randomness arrives as a plain `f64` in `[0, 1)` so this stays pure and
//! testable; the web crate feeds it `Math.random()`.

use crate::model::Note;

/// Picks a note, avoiding whichever one came up last.
///
/// Drawing the same note twice running feels broken even though it's perfectly
/// random, so `last_drawn` is excluded — unless it's the only note left, in
/// which case there's nothing else to hand over.
pub fn pick<'a>(notes: &'a [Note], last_drawn: Option<&str>, r: f64) -> Option<&'a Note> {
    if notes.is_empty() {
        return None;
    }

    let eligible: Vec<&Note> = match last_drawn {
        Some(skip) if notes.len() > 1 => notes.iter().filter(|c| c.id != skip).collect(),
        _ => notes.iter().collect(),
    };

    // `eligible` can only be empty if every note shares the skipped id, which
    // ids being unique rules out — but fall back rather than panic.
    if eligible.is_empty() {
        return notes.first();
    }

    let i = ((r.clamp(0.0, 1.0) * eligible.len() as f64) as usize).min(eligible.len() - 1);
    Some(eligible[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(ids: &[&str]) -> Vec<Note> {
        ids.iter()
            .map(|id| Note { id: (*id).into(), text: format!("note {id}"), added_at: 0 })
            .collect()
    }

    #[test]
    fn empty_jar_yields_nothing() {
        assert!(pick(&[], None, 0.5).is_none());
    }

    #[test]
    fn single_note_is_always_the_answer() {
        let c = notes(&["a"]);
        for r in [0.0, 0.5, 0.999] {
            assert_eq!(pick(&c, None, r).unwrap().id, "a");
        }
    }

    #[test]
    fn a_lone_note_can_repeat() {
        let c = notes(&["a"]);
        assert_eq!(
            pick(&c, Some("a"), 0.3).unwrap().id,
            "a",
            "with one note left there is nothing else to draw"
        );
    }

    #[test]
    fn last_drawn_is_skipped() {
        let c = notes(&["a", "b"]);
        for r in [0.0, 0.25, 0.5, 0.75, 0.999] {
            assert_eq!(pick(&c, Some("a"), r).unwrap().id, "b");
        }
    }

    #[test]
    fn randomness_spans_the_whole_jar() {
        let c = notes(&["a", "b", "c", "d"]);
        assert_eq!(pick(&c, None, 0.0).unwrap().id, "a");
        assert_eq!(pick(&c, None, 0.26).unwrap().id, "b");
        assert_eq!(pick(&c, None, 0.5).unwrap().id, "c");
        assert_eq!(pick(&c, None, 0.9999).unwrap().id, "d");
    }

    #[test]
    fn r_at_one_does_not_overflow() {
        let c = notes(&["a", "b", "c"]);
        assert_eq!(pick(&c, None, 1.0).unwrap().id, "c");
    }

    #[test]
    fn out_of_range_randomness_is_clamped() {
        let c = notes(&["a", "b", "c"]);
        assert_eq!(pick(&c, None, -3.0).unwrap().id, "a");
        assert_eq!(pick(&c, None, 7.5).unwrap().id, "c");
    }

    #[test]
    fn a_stale_last_drawn_id_does_not_block_the_draw() {
        let c = notes(&["a", "b"]);
        assert!(pick(&c, Some("long-gone"), 0.5).is_some());
    }

    #[test]
    fn every_note_is_reachable_over_many_draws() {
        let c = notes(&["a", "b", "c", "d", "e"]);
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..1000 {
            let r = i as f64 / 1000.0;
            seen.insert(pick(&c, None, r).unwrap().id.clone());
        }
        assert_eq!(seen.len(), 5, "some note could never be drawn");
    }
}
