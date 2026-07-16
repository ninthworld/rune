# Contributing to RUNE

RUNE is solo-maintained (one maintainer, with Claude as the coding assistant). The loop is
deliberately light — see [`AGENTS.md`](AGENTS.md) for the hard rules and repository map.

## The loop

1. **Branch** off `main` with a short descriptive name (`feat/<slug>`, `fix/<slug>`,
   `docs/<slug>`).
2. **Change** — keep it small and single-purpose. Tests are part of the change, not a
   follow-up.
3. **Commit** — Conventional Commits (`feat(engine): add phase FSM`, `fix(client): …`).
4. **Verify** — `make check` throughout; `make verify` before opening a PR (reproduces the
   `Engine`, `Client`, `E2E`, and `cargo-deny` checks). Red CI is never merged.
5. **PR & merge** — open a PR; merge once CI is green.

## Standards

- Rust: `cargo fmt` clean, `clippy -D warnings` clean, tests for all rules behavior.
- TypeScript: strict mode, typecheck clean, no game logic (see hard rules in `AGENTS.md`).
- Docs: behavior changes update `docs/`; architecture changes get an ADR in
  `docs/decisions/`.

## Scope discipline

Don't refactor or reformat unrelated code in the same change. If you spot something worth
fixing, note it and do it in its own change.

## Legal constraints

Read the Legal Considerations section of `docs/brief.md` before contributing anything
touching card data, card rendering, names, or distribution. Non-negotiables: no card
images, no official frame designs, no WotC branding, no monetization.
