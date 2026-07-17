# Contributing to RUNE

RUNE is solo-maintained. Keep changes focused, tested, and consistent with the architectural
rules in [`AGENTS.md`](AGENTS.md) and the standards in
[`docs/coding-standards.md`](docs/coding-standards.md).

## The loop

1. Branch from `main` with `feat/<slug>`, `fix/<slug>`, or `docs/<slug>`.
2. Make one coherent change and include its tests and documentation.
3. Commit with a Conventional Commit message, such as `feat(engine): add phase FSM`.
4. Run `make check` while working and `make verify` before opening the PR.
5. Open a focused PR and merge only after required CI passes.

## Standards

- Rust changes must pass formatting, Clippy with warnings denied, and relevant tests.
- TypeScript changes must pass lint, strict type checking, tests, and build.
- Protocol changes update the Rust types, TypeScript mirror, and `docs/protocol.md` together.
- Architectural changes add or amend an ADR in `docs/decisions/`.

## Scope discipline

Do not refactor or reformat unrelated code in the same change.

## Legal constraints

Read the [legal constraints](docs/brief.md#legal-constraints) before changing card data,
rendering, or distribution. Do not add card images, official frame designs, Wizards of the
Coast branding, exact Oracle text, or monetization.
