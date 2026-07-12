# CLAUDE.md

@AGENTS.md

Claude-specific notes:
- Required reading before writing code: `docs/coding-standards.md` (enforced by `make check`).
- Prefer `make check` over invoking cargo/npm directly so you run exactly what CI runs.
- When a task spans engine and client, it is two leaf issues under one parent — not two
  PRs against the same issue. One leaf issue maps to exactly one PR
  (`docs/agents/continuance.md`).
- Package-level instructions: `crates/rune-engine/AGENTS.md`, `clients/web/AGENTS.md`.
