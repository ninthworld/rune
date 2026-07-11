# RUNE coding standards

The rules that always apply. Read this before writing code; agents must load it
alongside `AGENTS.md`. Everything machine-checkable here is enforced by
`make check` (which is exactly what CI runs) — if it isn't green, it isn't done.

These standards sit **below** the architectural hard rules in `AGENTS.md`
(zero game logic in the client, zero I/O in the engine, protocol = contract).
When they appear to conflict, the hard rules win.

## Baseline

Enforcement level is **practical-strict**: deny compiler/clippy warnings, forbid
panicking APIs outside tests, and require docs on public items. We deliberately
do **not** enable `clippy::pedantic` or `clippy::nursery` — they add more noise
than signal at this stage.

## Rust

Enforced by `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`
(see the `[workspace.lints]` table in the root `Cargo.toml`). Every crate opts in
with `[lints] workspace = true`.

- **Formatting.** Default `rustfmt`, no local overrides. Run `make engine-fmt`
  before committing; CI fails on any diff.
- **No panicking APIs in non-test code.** `unwrap()`, `expect()`, `panic!`,
  `todo!`, `unimplemented!`, and `dbg!` are denied. Return `Result`/`Option` and
  handle the `None`/`Err` path. In the engine, an impossible state is a bug —
  model it out or return an error, don't `unwrap`.
- **Tests may panic.** In a `#[cfg(test)]` module, add
  `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` (or the
  subset you use). A failed `unwrap`/`assert` *is* the test's failure signal.
- **Docs on public items.** `missing_docs` is on: every public item — modules,
  types, fields, enum variants, functions — needs a `///` (or `//!`) comment.
  Keep them short; say what the reader can't infer from the name.
- **No `unsafe`.** `unsafe_code` is `forbid` workspace-wide. This is a pure,
  safe codebase; there is no performance case that justifies it here.
- **Prefer `pub(crate)`.** `unreachable_pub` warns when a `pub` item isn't
  reachable from the crate root — tighten it unless it's genuinely part of the
  crate's public API.
- **Naming & layout.** Standard Rust conventions (`snake_case` items,
  `CamelCase` types, `SCREAMING_SNAKE_CASE` consts). Keep modules small and
  cohesive; everything derivable is computed on demand, never cached on state
  (see `crates/rune-engine/AGENTS.md`).

## TypeScript / web client

Enforced by `make client-check` (typecheck + build) and, once wired,
`npm run lint`. See `clients/web/AGENTS.md`.

- **Formatting.** Prettier, no local overrides; CI fails on any diff.
- **Linting.** ESLint with `typescript-eslint` and `react-hooks` rules; no
  disables without an inline justification comment.
- **`strict` TypeScript.** No implicit `any`; prefer precise protocol types.
- **No game logic.** The client renders `GameView` and echoes an `action_id`.
  It never computes legality, cost, or effect (`AGENTS.md` hard rule).

## Cross-cutting

- **Tests.** Add or update tests for everything you change. Protocol shape
  changes get round-trip tests; engine changes get state-transition tests.
- **Commits.** Conventional Commits (`feat(engine): …`, `fix(client): …`,
  `docs: …`, `chore: …`).
- **Docs stay in sync.** Protocol changes update `docs/protocol.md`;
  architectural changes get an ADR in `docs/decisions/`.
- **Cite the CR for rule behavior.** Engine code that implements a
  Comprehensive Rules rule cites it as `CR NNN.Nx` (e.g. `CR 605.3`) in the
  doc comment of the item that implements it, so the rule and its code stay
  traceable both ways. Any PR that adds or changes rule behavior updates
  `docs/rules-coverage.md` in the **same PR** — add or amend the row (rule
  number, one-line summary, status, code anchor, test anchor), marking anything
  incomplete `partial` and naming the gap.
- **No secrets, no vendored non-MIT code**, no `target/`, no `node_modules/`.

## Before you push

```
make check
```

Green `make check`, updated tests and docs, no unrelated diffs. That's the bar.
