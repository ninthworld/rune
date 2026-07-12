# AGENTS.md — rune-engine

The rules engine. Layer 3 of docs/brief.md. Read that section before any change here.

## Hard rules
- Pure functions over immutable `GameState`. `apply_action` clones; it never mutates.
- **No dependencies on tokio, networking, timers, threads, wall-clock time, or randomness
  without an injected seed.** This crate compiles with an empty `[dependencies]` today;
  adding any dependency requires an ADR.
- No listeners/observers. Triggers are collected by diffing before/after states.
  Characteristics are computed fresh by pure functions (layer system), never cached.
- Every permanent gets a fresh `PermanentId` on battlefield entry — zone-change identity
  is the mechanism, do not add zone-change counters.
- Every rules behavior change ships with tests in the same PR. Rules bugs get a
  regression test named after the issue (`issue_123_...`).
- Cards are **data**, authored against the schema in `docs/card-schema.md` (ADR 0018):
  a functional definition per card under a stable `functional_id`, no presentation
  assets (the schema rejects them structurally), and code-defined behavior only via the
  declared `scripted` escape hatch.

## Commands
- `cargo test -p rune-engine`
- `cargo clippy -p rune-engine --all-targets -- -D warnings`
