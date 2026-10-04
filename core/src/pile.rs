//! Where the notes sit inside a jar.
//!
//! The arrangement is a deterministic function of the note count and the jar's
//! seed. That matters: a pile recomputed with fresh randomness every frame
//! would visibly twitch, so the scatter has to be a stable property of the jar.

use crate::geom;
use crate::rng::Rng;

/// One note at rest, in sprite space, anchored top-left before rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub x: f32,
    pub y: f32,
    /// Degrees.
    pub rot: f32,
    pub w: f32,
    pub h: f32,
}

/// Lays out `count` notes, bottom row first.
///
/// Rows hold two notes each and stack upward. A crowded jar compresses its row
/// pitch instead of pushing notes out through the lid.
pub fn layout(count: usize, seed: u64) -> Vec<Placed> {
    if count == 0 {
        return Vec::new();
    }

    let mut rng = Rng::new(seed);
    let rows = count.div_ceil(geom::PER_ROW);
    let headroom = geom::FLOOR_Y - geom::CEILING_Y;
    // With few notes keep the natural pitch; past that, squeeze to fit.
    let pitch = (headroom / rows as f32).min(geom::ROW_PITCH);

    (0..count)
        .map(|i| {
            let row = i / geom::PER_ROW;
            let column = i % geom::PER_ROW;
            Placed {
                x: geom::LEFT_X + column as f32 * geom::COLUMN_GAP + rng.range(-4.5, 4.5),
                y: geom::FLOOR_Y - row as f32 * pitch - rng.range(0.0, 2.5),
                rot: rng.range(-21.0, 21.0),
                w: geom::NOTE_W + rng.range(0.0, 5.0),
                h: geom::NOTE_H + rng.range(0.0, 2.5),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_jar_has_no_notes() {
        assert!(layout(0, 1).is_empty());
    }

    #[test]
    fn count_is_respected() {
        for n in [1, 2, 3, 7, 40] {
            assert_eq!(layout(n, 5).len(), n);
        }
    }

    #[test]
    fn same_seed_gives_the_same_pile() {
        assert_eq!(layout(9, 123), layout(9, 123));
    }

    #[test]
    fn different_seeds_give_different_piles() {
        assert_ne!(layout(9, 1), layout(9, 2));
    }

    #[test]
    fn removing_a_note_leaves_the_rest_where_they_were() {
        // The pile must not reshuffle when one note is drawn out of it.
        let before = layout(8, 77);
        let after = layout(7, 77);
        assert_eq!(&before[..7], &after[..], "the remaining notes jumped");
    }

    #[test]
    fn the_pile_stacks_upward() {
        let p = layout(8, 11);
        let bottom_row = p[0].y.max(p[1].y);
        let next_row = p[2].y.max(p[3].y);
        assert!(next_row < bottom_row, "row 2 should sit above row 1");
    }

    #[test]
    fn a_crowded_jar_still_fits_inside() {
        for n in [1, 5, 20, 60, 200] {
            for p in layout(n, 3) {
                assert!(
                    p.y >= geom::CEILING_Y - 3.0 && p.y <= geom::FLOOR_Y,
                    "note escaped vertically at count {n}: y={}",
                    p.y
                );
                assert!(
                    p.x > geom::LEFT_X - 10.0 && p.x + p.w < geom::VIEW_W - 40.0,
                    "note escaped sideways at count {n}: x={}",
                    p.x
                );
            }
        }
    }

    #[test]
    fn notes_vary_in_size_and_angle() {
        let p = layout(12, 9);
        assert!(p.iter().any(|c| c.rot < 0.0) && p.iter().any(|c| c.rot > 0.0));
        assert!(p.windows(2).any(|w| w[0].w != w[1].w), "every note the same width looks printed");
    }
}
