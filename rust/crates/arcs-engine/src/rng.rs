//! Deterministic RNG and the chance seam.
//!
//! The port keeps **statistical parity only** with the TS engine: this is a
//! clean SplitMix64, not a replication of mulberry32 or the JS seed quirks.
//! Chance is consumed only through [`ChanceSource`], the seam that later
//! grows a `Scripted` injector for replays and tests.

/// A seedable RNG. Everything derives from `next_u64`.
pub trait Rng {
    fn next_u64(&mut self) -> u64;

    #[inline]
    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// A uniform f64 in `[0, 1)` with 53 bits of precision.
    #[inline]
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// A uniform index in `0..n`. `n` must be non-zero.
    #[inline]
    fn gen_range(&mut self, n: usize) -> usize {
        debug_assert!(n > 0, "gen_range(0)");
        // Multiply-shift: bias is < n / 2^64, far below anything observable
        // at game sizes (n <= 31).
        ((self.next_u64() as u128 * n as u128) >> 64) as usize
    }

    /// Fisher-Yates, in place.
    fn shuffle<T>(&mut self, items: &mut [T])
    where
        Self: Sized,
    {
        for i in (1..items.len()).rev() {
            let j = self.gen_range(i + 1);
            items.swap(i, j);
        }
    }

    /// A uniformly chosen element. `items` must be non-empty.
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T
    where
        Self: Sized,
    {
        &items[self.gen_range(items.len())]
    }

    /// Derive a fresh seed from the stream (per-game / per-rollout forks).
    #[inline]
    fn fork_seed(&mut self) -> u64 {
        self.next_u64()
    }
}

/// Sebastiano Vigna's SplitMix64: tiny, fast, passes BigCrush, and any u64
/// seed (including 0) is fine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    #[inline]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl Rng for SplitMix64 {
    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Where chance outcomes come from. `resolve_chance_mut` (R1+) draws from
/// this and nowhere else, so a future `Scripted` variant can replay recorded
/// games without touching the rules.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum ChanceSource {
    /// Sample outcomes from a seeded RNG (normal play).
    Sampled(SplitMix64),
}

impl ChanceSource {
    #[inline]
    pub const fn sampled(seed: u64) -> Self {
        Self::Sampled(SplitMix64::new(seed))
    }

    /// The RNG to draw the next chance outcome from.
    #[inline]
    pub fn rng(&mut self) -> &mut SplitMix64 {
        match self {
            Self::Sampled(rng) => rng,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = SplitMix64::new(0xDEAD_BEEF);
        let mut b = SplitMix64::new(0xDEAD_BEEF);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn known_splitmix64_vectors() {
        // Reference values for seed 1234567 from Vigna's splitmix64.c.
        let mut rng = SplitMix64::new(1234567);
        assert_eq!(rng.next_u64(), 6457827717110365317);
        assert_eq!(rng.next_u64(), 3203168211198807973);
        assert_eq!(rng.next_u64(), 9817491932198370423);
    }

    #[test]
    fn next_f64_in_unit_interval() {
        let mut rng = SplitMix64::new(42);
        for _ in 0..10_000 {
            let x = rng.next_f64();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn gen_range_covers_and_stays_in_bounds() {
        let mut rng = SplitMix64::new(7);
        let mut seen = [false; 6];
        for _ in 0..1000 {
            seen[rng.gen_range(6)] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut rng = SplitMix64::new(99);
        let mut items: Vec<u8> = (0..24).collect();
        rng.shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..24).collect::<Vec<u8>>());
    }

    #[test]
    fn chance_source_is_deterministic() {
        let mut a = ChanceSource::sampled(5);
        let mut b = ChanceSource::sampled(5);
        assert_eq!(a.rng().next_u64(), b.rng().next_u64());
    }
}
