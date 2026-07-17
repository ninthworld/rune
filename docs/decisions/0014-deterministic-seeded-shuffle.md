# ADR 0014: Deterministic seeded shuffle

- Status: accepted
- Date: 2026-07-11
- Issue: #109

## Context

Games require shuffled libraries, while the engine forbids ambient entropy and must reproduce
the same state from the same setup for replay, tests, and simulation.

## Decision

`crates/rune-engine/src/rng.rs` implements SplitMix64 and an unbiased Fisher–Yates shuffle
using rejection sampling. No PRNG dependency is added.

`GameSetup` supplies the seed. Game construction shuffles libraries in seat order and stores
the advanced generator state in `GameState` so later random operations continue the same
stream. The engine never reads OS entropy; the server chooses the initial seed.

## Consequences

Identical setup values produce identical library order and later random results. SplitMix64 is
not cryptographically secure, so the server must keep seeds out of player views and choose them
with suitable entropy when unpredictability matters. The project owns and tests the small PRNG
implementation.
