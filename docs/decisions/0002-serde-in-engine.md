# ADR 0002: serde in the engine, for compile-time-embedded data only

- Status: accepted
- Date: 2026-07-30

## Context

The engine must stay free of I/O and runtime services (ADR 0001), and any dependency it takes
is a permanent constraint on that purity. At the same time the card database has to turn a
bundled snapshot of card data into typed Rust: characteristics, abilities, and the effect IR.

Two ways to do that:

1. **serde + serde_json.** Derive `Deserialize` on the card types and parse a
   compile-time-embedded (`include_str!`) JSON string. Both crates are already in the
   workspace lockfile — `sage-protocol` depends on serde — and both are MIT/Apache-2.0, so
   they add no new license surface under `deny.toml`.
2. **A hand-rolled parser.** Keeps `[dependencies]` empty, but is more code to own and
   brittle at JSON edge cases — a poor trade for a format that only gets richer.

The engine's "no dependencies" instinct is really a *no I/O, no async, no runtime services*
rule. serde used purely for compile-time-embedded data does not violate it: there is no
filesystem, network, clock, thread, or randomness involved. `include_str!` embeds bytes at
build time and `serde_json` parses an in-memory `&str`. The engine stays pure.

## Decision

The engine may depend on **serde** (with `derive`) and **serde_json**, scoped to
deserializing bundled, compile-time-embedded data.

- Card data is embedded via `include_str!`. The engine performs **no `std::fs`, no network,
  no runtime I/O**, preserving the zero-I/O rule.
- This is the only sanctioned use. Any dependency that introduces I/O, async, timers,
  threads, wall-clock time, or unseeded randomness remains forbidden and needs its own ADR.
- New dependencies must remain MIT-compatible and pass `deny.toml`.

## Consequences

- **Easier.** Card data is typed and validated by `#[derive(Deserialize)]`; the snapshot
  format can grow without hand-written parsing; no new crates enter the dependency graph.
- **Harder / given up.** "The engine has zero dependencies" stops being a mechanical
  invariant that a glance at `Cargo.toml` can confirm. The invariant it actually protects —
  purity, no I/O — is preserved, but now has to be stated and enforced by review rather than
  by an empty table.
- The precedent is narrow and quotable: compile-time data parsing is allowed; runtime
  services are not.
