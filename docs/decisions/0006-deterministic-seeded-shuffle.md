# ADR 0006: Deterministic seeded shuffle via an inline SplitMix64

- Status: accepted
- Date: 2026-07-30

## Context

Games start from shuffled decks (CR 103.3), and the engine forbids I/O, wall-clock time,
threads, and randomness without an injected seed (ADR 0001). The shuffle must therefore be a
pure function of `(seed, decklists)`: the same inputs reproduce the same library order — for
replay, resync, and AI tree search — and no OS entropy leaks in.

Reaching for `rand` / `rand_chacha` would pull a dependency tree into a crate whose dependency
policy is deliberately narrow (ADR 0002). The randomness needed here is small and
self-contained, so the trade is between a dependency plus its transitive tree and a few lines
of well-known, auditable code.

## Decision

The engine implements its own randomness in `crates/sage-engine/src/rng.rs`: a **SplitMix64**
generator (one `u64` of state) plus an unbiased Fisher–Yates `shuffle`, with bounded draws
using rejection sampling to avoid modulo bias. **No PRNG crate is added.**

- Every draw comes from `GameState::rng_seed` and nowhere else. `GameState::new` seeds the
  generator from `GameSetup::rng_seed`, shuffles each library in seating order, and stores the
  *advanced* generator state back into `rng_seed`, so later randomness continues the same
  stream.
- SplitMix64 is a standard, public-domain algorithm (Steele/Vigna) whose constants and
  structure are fixed, so "same seed plus same decklists produces an identical order" is a
  guaranteed property, and different seeds diverge.

## Consequences

- **Easier.** Shuffles are replayable and testable with plain equality assertions, and nothing
  can smuggle in OS entropy.
- **Harder / given up.** SplitMix64 is not cryptographically secure: an observer who knows the
  seed can predict deck order. That is acceptable because the seed is server-side state and is
  never projected into any view. A feature needing an unpredictable-to-players shuffle can
  choose the seed with more entropy at the server layer without changing the engine's pure
  interface.
- We own a few lines of PRNG code, covered by unit tests asserting determinism, range, and
  permutation invariants.
