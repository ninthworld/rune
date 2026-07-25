# RUNE coding standards

These standards apply to all code. The architectural rules in [`AGENTS.md`](../AGENTS.md)
take precedence. `make check` runs the fast Engine and Client checks; `make verify` adds
the required dependency-policy checks and is the pre-merge gate.

## Baseline

Compiler and Clippy warnings are denied. Panicking APIs are forbidden outside tests,
public items require documentation, and unsafe Rust is forbidden. The workspace does not
enable `clippy::pedantic` or `clippy::nursery`.

## Rust

Enforced by `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`
(see the `[workspace.lints]` table in the root `Cargo.toml`). Every crate opts in
with `[lints] workspace = true`.

- **Formatting.** Use default `rustfmt`; do not add local overrides.
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
- **No `unsafe`.** `unsafe_code` is forbidden workspace-wide.
- **Prefer `pub(crate)`.** `unreachable_pub` warns when a `pub` item isn't
  reachable from the crate root — tighten it unless it's genuinely part of the
  crate's public API.
- **Naming and layout.** Follow Rust conventions and keep modules cohesive. Engine
  derivations are computed on demand, never cached in game state.

## TypeScript / web client

Enforced by `make client-check`. See [`clients/web/AGENTS.md`](../clients/web/AGENTS.md).

- **Formatting.** Use Prettier with no local overrides.
- **Linting.** ESLint with `typescript-eslint` and `react-hooks` rules; no
  disables without an inline justification comment.
- **`strict` TypeScript.** No implicit `any`; prefer precise protocol types.
- **No game logic.** The client renders `GameView` and echoes an `action_id`.
  It never computes legality, cost, or effect (`AGENTS.md` hard rule).

## File size

Keep files small enough that a reader can hold one in their head. Target well under
~500 lines. Treat **~800 lines as a soft ceiling** that should prompt a split, and
**1000+ lines as a hard smell** to break up before adding more code.

- **Split along cohesive seams.** When a file approaches the ceiling, move cohesive
  groups into submodules with crate-root (or module-root) re-exports so the public API
  is unchanged. This is pure code motion — tests and fixtures move with the code they
  exercise. See #78 (engine `lib.rs`) and #406 (protocol `lib.rs`) for the worked
  recipe.
- **Tests and fixtures count.** A file that is mostly tests still gets split — the
  historical offenders were ~75% tests.
- **Cohesion, not a `wc -l` gate.** This is guidance, not a hard line-count check. A
  single cohesive match table or generated file may legitimately be long; the point is
  to stop *unrelated* concerns from piling into one file.

## Cross-cutting

- **Tests.** Test every behavior change. Protocol shapes need round-trip tests; engine
  rules need state-transition tests.
- **Commits.** Conventional Commits (`feat(engine): …`, `fix(client): …`,
  `docs: …`, `chore: …`).
- **Docs stay in sync.** Protocol changes update the Rust and TypeScript types plus
  `docs/protocol.md`; architectural changes update `docs/decisions/`.
- **Cite the Comprehensive Rules.** Engine implementations cite `CR NNN.Nx` in the
  relevant doc comment, and their tests use the same rule number in names such as
  `cr_605_3_…`. Record partial support in a nearby `// NOTE:` comment. List the living
  coverage with `rg 'cr_\d' crates/rune-engine/src`.
- **No secrets, no vendored non-MIT code**, no `target/`, no `node_modules/`.

## Verification

```
make check    # fast gate — run constantly while working
make verify   # full pre-merge gate — check + cargo-deny, before opening a PR
```

Before review, ensure `make verify` passes, documentation matches behavior, and the diff
contains no unrelated changes.
