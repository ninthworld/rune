# CLAUDE.md

@AGENTS.md

Claude-specific notes:
- Required reading before writing code: `docs/coding-standards.md` (enforced by `make check`).
- Prefer `make check` over invoking cargo/npm directly so you run exactly what CI runs.
- Keep changes small and single-purpose; a change spanning engine and client is fine in one
  PR as long as it stays coherent and reviewable.
- Package-level instructions: `crates/rune-engine/AGENTS.md`, `clients/web/AGENTS.md`.
