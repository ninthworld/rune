# Contributing to RUNE

This project is developed issue-first, by both AI agents and humans. The rules below
apply to everyone; agent-specific detail lives in `AGENTS.md` and `docs/agents/`.

## The loop

1. **Issue** — every change starts as a GitHub issue with acceptance criteria
   (use the templates). Label it (`area:engine`, `area:client`, `area:protocol`,
   `area:docs`) and size it small enough for one PR.
2. **Branch** — `agent/<issue>-<slug>` for agent work, `feat/<slug>` or
   `fix/<slug>` for humans.
3. **PR** — small, single-purpose, linked with `Closes #N`, template filled in.
   Conventional Commits (`feat(engine): add phase FSM skeleton`).
4. **CI** — the `Engine` and `Client` checks are required. Red CI is never merged.
5. **Review** — at least one human approval. Authors (human or agent) never merge
   their own PRs. Agent-authored PRs carry the `agent` label.

## Standards

- Rust: `cargo fmt` clean, `clippy -D warnings` clean, tests for all rules behavior.
- TypeScript: strict mode, typecheck clean, no game logic (see hard rules in AGENTS.md).
- Docs: behavior changes update `docs/`; architecture changes get an ADR.
- Tests are part of the change, not a follow-up.

## Scope discipline

Do not refactor, reformat, or "improve" code unrelated to your issue in the same PR.
If you find something worth fixing, open an issue for it.

## Legal constraints

Read the Legal Considerations section of `docs/brief.md` before contributing anything
touching card data, card rendering, names, or distribution. Non-negotiables: no card
images, no official frame designs, no WotC branding, no monetization.
