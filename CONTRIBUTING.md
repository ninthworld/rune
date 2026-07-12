# Contributing to RUNE

This project is developed issue-first, by both AI agents and humans. The rules below
apply to everyone; agent-specific detail lives in `AGENTS.md` and `docs/agents/`.

The full lifecycle — how milestones become issues, issues become pull requests, and what
each human gate is for — is [`docs/agents/continuance.md`](docs/agents/continuance.md).
The summary:

## The loop

1. **Issue** — every change starts as a GitHub issue with acceptance criteria
   (use the templates). Label it (`area:engine`, `area:client`, `area:protocol`,
   `area:docs`) and size it small enough for one PR.
2. **Branch** — `agent/<issue>-<slug>` for agent work, `feat/<slug>` or
   `fix/<slug>` for humans.
3. **PR** — small, single-purpose, closing exactly one leaf issue with `Closes #N`,
   template filled in. Conventional Commits (`feat(engine): add phase FSM skeleton`).
4. **CI** — the `Engine`, `Client`, `E2E`, and `cargo-deny` checks are required, and the
   branch must be current with `main`. Reproduce all four locally with `make verify`.
   Red CI is never merged.
5. **Review** — at least one human approval, from someone other than the author. Authors
   (human or agent) never approve or merge their own PRs. Agent-authored PRs carry the
   `agent` label.

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
