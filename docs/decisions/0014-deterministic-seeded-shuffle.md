# ADR 0014: Deterministic seeded shuffle via an inline SplitMix64

- Status: accepted
- Date: 2026-07-11
- Issue: #109

## Context
Games must start from shuffled decks (CR 103.3), and the engine's hard rules
(`crates/rune-engine/AGENTS.md`, ADR 0002) forbid I/O, wall-clock time, threads,
and "randomness without an injected seed". `GameState` already reserves an
`rng_seed` field (CR 103.3) for exactly this. The shuffle must therefore be a
pure function of `(seed, decklists)`: the same inputs reproduce the same library
order (for replay, resync, and AI tree search), and no OS entropy may leak in.

The obvious reach for `rand` / `rand_chacha` would pull a dependency into a crate
that today compiles with an empty `[dependencies]`, and the engine `AGENTS.md`
rule requires an ADR for any dependency. The randomness we need is small and
self-contained, so the trade is between a dependency plus its transitive tree and
a few lines of well-known, auditable code.

## Decision
The engine implements its own randomness in `crates/rune-engine/src/rng.rs`: a
**SplitMix64** generator (one `u64` of state) plus an unbiased Fisher–Yates
`shuffle`, with bounded draws using rejection sampling to avoid modulo bias.
**No PRNG crate is added.**

- Every draw comes from `GameState::rng_seed` and nowhere else. `GameState::new`
  seeds the generator from `GameSetup::rng_seed`, shuffles each library in
  seating order, and stores the *advanced* generator state back into `rng_seed`
  so later randomness continues the same stream.
- SplitMix64 is a standard, public-domain algorithm (Steele/Vigna); its constants
  and structure are fixed, so "same seed + same decklists → identical order" is a
  guaranteed property, and different seeds diverge.

## Consequences
- Easier: the engine keeps its empty dependency tree; shuffles are replayable and
  testable with plain equality assertions; nothing can smuggle in OS entropy.
- Harder / given up: SplitMix64 is not cryptographically secure — a determined
  observer who knows the seed can predict the deck order. That is acceptable
  because the seed is server-side state and is never projected into any
  `GameView` (see `GameState::rng_seed` docs). If a future feature needs an
  unpredictable-to-players shuffle, the seed can be chosen with more entropy at
  the server layer without changing the engine's pure interface.
- We own a few lines of PRNG code; they are covered by unit tests
  (`rng.rs`) asserting determinism, range, and permutation invariants.
