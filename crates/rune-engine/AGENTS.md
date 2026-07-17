# Engine agent guide

`rune-engine` is the pure, deterministic rules state machine. Read the engine section of
[`docs/brief.md`](../../docs/brief.md) before changing it.

## Hard rules

- Pure functions over immutable `GameState`. `apply_action` clones; it never mutates.
- **No dependencies on tokio, networking, timers, threads, wall-clock time, or randomness
  without an injected seed.** Runtime dependencies are limited to serde for embedded card
  data; adding another dependency requires an ADR.
- No listeners/observers. Triggers are collected by diffing before/after states.
  Characteristics are computed fresh by pure functions (layer system), never cached.
- Every permanent gets a fresh `PermanentId` on battlefield entry — zone-change identity
  is the mechanism, do not add zone-change counters.
- Every rules behavior change ships with tests in the same PR. Rules bugs get a
  regression test named after the issue (`issue_123_...`).
- Cards are **data**, authored against [`docs/card-schema.md`](../../docs/card-schema.md):
  a functional definition per card under a stable `functional_id`, no presentation
  assets (the schema rejects them structurally), and code-defined behavior only via the
  declared `scripted` escape hatch.
- **Never write a `CardId` down.** `build.rs` assembles `data/catalog/` + `data/sets/`
  at compile time and interns `CardId(0..n)` from the sorted `FunctionalId`s (ADR 0018
  §3), so authoring one card renumbers its neighbours. Name a card by its
  `functional_id` and resolve the handle (`CardDatabase::card_id`) — in decklists, in
  `scripted.rs`, and in tests (`crate::fixtures::fixture`).
- `build.rs` may read catalog files at compile time. The running engine performs no I/O.
  Shared validators live in `src/catalog.rs` so build-time and load-time checks agree.

## Commands

- `cargo test -p rune-engine`
- `cargo clippy -p rune-engine --all-targets -- -D warnings`
