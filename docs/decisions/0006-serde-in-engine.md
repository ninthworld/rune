# ADR 0006: serde in the engine for bundled card data

- Status: accepted
- Date: 2026-07-10
- Issue: #25

## Context
`rune-engine` has shipped with an empty `[dependencies]` table on purpose: the
crate must stay free of I/O and runtime dependencies (crate `AGENTS.md`), and
adding any dependency requires an ADR. The card database (issue #25) needs to
resolve a `CardId` to immutable card characteristics (name, type line, mana
cost, oracle text, power/toughness) from a bundled snapshot.

The snapshot is data, and the shape will grow. The realistic options for turning
it into typed Rust:

1. **serde + serde_json.** Derive `Deserialize` on the card type; parse a
   compile-time-embedded (`include_str!`) JSON string. serde/serde_json are
   already in the workspace lockfile (`rune-protocol` depends on serde) and are
   MIT/Apache-2.0, so they add no new license surface (`deny.toml`, ADR-0005).
2. **A hand-rolled parser.** Keeps `[dependencies]` empty, but is more code to
   own and brittle to JSON edge cases — a poor trade for a format that will only
   get richer.

The engine's "no dependencies" rule is really a *no I/O, no async, no runtime
services* rule. serde used purely for compile-time-embedded data parsing does
not violate that: there is no filesystem, network, clock, thread, or randomness
involved — `include_str!` embeds the bytes at build time and `serde_json`
parses an in-memory `&str`. The engine stays pure.

## Decision
The engine may depend on **serde** (with `derive`) and **serde_json**, scoped to
deserializing bundled, compile-time-embedded data snapshots.

- Card data is embedded via `include_str!`; the engine performs **no `std::fs`,
  no network, no runtime I/O**. This preserves the "zero I/O in the engine" hard
  rule (`AGENTS.md`).
- This is the *only* sanctioned use today. Any dependency that introduces I/O,
  async, timers, threads, wall-clock time, or unseeded randomness remains
  forbidden and would need its own ADR.
- New dependencies must remain MIT-compatible per ADR-0005 and pass `deny.toml`.

## Consequences
- **Easier:** card data is typed and validated by `#[derive(Deserialize)]`; the
  snapshot format can grow without hand-written parsing; no new crates enter the
  dependency graph (serde/serde_json were already present transitively).
- **Harder / given up:** the engine's `[dependencies]` table is no longer
  literally empty, so "the engine has zero dependencies" stops being a mechanical
  invariant. The invariant it actually protects — purity, no I/O — is preserved
  and now stated explicitly here and in the crate `AGENTS.md`.
- Future contributors have a precedent to point at: compile-time data parsing is
  allowed; runtime services are not.
