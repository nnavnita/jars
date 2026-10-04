//! splitmix64, so a jar's pile looks scattered but lands identically every reload.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // A zero seed would make the first few outputs correlate; nudge it off zero.
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        // Top 24 bits give exactly the mantissa an f32 can hold without rounding to 1.0.
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        let seq_a: Vec<u64> = (0..64).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..64).map(|_| b.next_u64()).collect();
        assert_eq!(seq_a, seq_b);
    }

    #[test]
    fn sequence_does_not_repeat_itself() {
        let mut r = Rng::new(42);
        let seq: Vec<u64> = (0..64).map(|_| r.next_u64()).collect();
        let mut sorted = seq.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), seq.len(), "splitmix64 emitted a duplicate in 64 draws");
    }

    #[test]
    fn different_seeds_diverge() {
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
    }

    #[test]
    fn floats_stay_in_unit_range() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            let v = r.next_f32();
            assert!((0.0..1.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn zero_seed_is_not_degenerate() {
        let mut r = Rng::new(0);
        let first = r.next_u64();
        assert_ne!(first, 0);
        assert_ne!(first, r.next_u64());
    }

    #[test]
    fn range_respects_bounds() {
        let mut r = Rng::new(99);
        for _ in 0..1000 {
            let v = r.range(-5.0, 5.0);
            assert!((-5.0..5.0).contains(&v));
        }
    }
}
