//! The engine's one and only source of randomness: a tiny, deterministic PRNG.
//!
//! Per the crate `AGENTS.md` rule ("no randomness without an injected seed") and
//! the [`GameState::rng_seed`](crate::GameState::rng_seed) invariant, randomness
//! must be pure and replayable: the same seed always produces the same sequence,
//! and there is no wall-clock, OS entropy, or thread-local generator anywhere.
//!
//! [`SplitMix64`] is that generator — a well-known 64-bit mixing function with a
//! single `u64` of state, small enough to inline and justify without adding a
//! dependency (see `docs/decisions/0014-deterministic-seeded-shuffle.md`). Its
//! state is exactly the shape of the reserved seed slot, so a game can advance
//! the seed by running the generator and storing its state back.

/// A deterministic [SplitMix64](https://prng.di.unimi.it/splitmix64.c) generator.
///
/// One `u64` of state, seeded at construction. Every call to [`Self::next_u64`]
/// advances the state and returns a well-mixed value; identical seeds yield
/// identical streams, which is what makes shuffles replayable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SplitMix64 {
    /// The generator's running state; also the value stored back into
    /// [`GameState::rng_seed`](crate::GameState::rng_seed) once randomness is
    /// consumed, so the seed slot always reflects work already done.
    state: u64,
}

impl SplitMix64 {
    /// Seed a fresh generator with `seed`.
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The current internal state, to be stored back into the reserved seed slot
    /// after randomness is consumed so replay resumes from the same point.
    pub(crate) fn state(self) -> u64 {
        self.state
    }

    /// Advance the state and return the next 64-bit output.
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniformly distributed value in `[0, bound)`, or `0` when `bound` is `0`.
    ///
    /// Uses rejection sampling so the result is free of modulo bias: outputs that
    /// fall in the short leftover region at the top of the `u64` range are
    /// discarded and redrawn, leaving a range whose length is an exact multiple
    /// of `bound`. Determinism is preserved because the discarded draws advance
    /// the same seeded stream.
    pub(crate) fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        // `bound.wrapping_neg()` is `2^64 - bound`; taken mod `bound` this is
        // `2^64 mod bound`, the count of "extra" low values to reject so the
        // remaining outputs divide evenly by `bound`.
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let value = self.next_u64();
            if value >= threshold {
                return value % bound;
            }
        }
    }

    /// Shuffle `slice` in place with an unbiased Fisher–Yates pass driven by this
    /// generator (CR 103.3 "randomize the order").
    ///
    /// Walks from the last index down, swapping each element with one chosen
    /// uniformly from the not-yet-fixed prefix. Same seed and same input length
    /// produce the same permutation.
    pub(crate) fn shuffle<T>(&mut self, slice: &mut [T]) {
        for i in (1..slice.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            slice.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn same_seed_yields_same_stream() {
        let mut a = SplitMix64::new(12345);
        let mut b = SplitMix64::new(12345);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_yield_different_streams() {
        let mut a = SplitMix64::new(1);
        let mut b = SplitMix64::new(2);
        // Overwhelmingly likely to diverge on the very first draw.
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut rng = SplitMix64::new(99);
        let mut seen = [false; 6];
        for _ in 0..1000 {
            let v = rng.below(6);
            assert!(v < 6);
            seen[v as usize] = true;
        }
        // With 1000 draws every bucket in a range of six is hit.
        assert!(seen.iter().all(|&hit| hit));
    }

    #[test]
    fn below_zero_bound_is_zero() {
        let mut rng = SplitMix64::new(7);
        assert_eq!(rng.below(0), 0);
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut rng = SplitMix64::new(2024);
        let mut data: Vec<u32> = (0..50).collect();
        rng.shuffle(&mut data);
        let mut sorted = data.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        // A 50-element shuffle essentially never returns identity order.
        assert_ne!(data, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn shuffle_is_deterministic_for_a_seed() {
        let mut rng_a = SplitMix64::new(555);
        let mut rng_b = SplitMix64::new(555);
        let mut a: Vec<u32> = (0..20).collect();
        let mut b: Vec<u32> = (0..20).collect();
        rng_a.shuffle(&mut a);
        rng_b.shuffle(&mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn shuffle_handles_trivial_lengths() {
        let mut rng = SplitMix64::new(1);
        let mut empty: Vec<u32> = Vec::new();
        rng.shuffle(&mut empty);
        assert!(empty.is_empty());
        let mut single = vec![42];
        rng.shuffle(&mut single);
        assert_eq!(single, vec![42]);
    }
}
