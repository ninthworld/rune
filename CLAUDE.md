# CLAUDE.md

@AGENTS.md

Claude-specific notes:
- Prefer `make check` over invoking cargo/npm directly so you run exactly what CI runs.
- When a task spans engine and client, split it into two PRs against the same issue.
- Package-level instructions: `crates/rune-engine/AGENTS.md`, `clients/web/AGENTS.md`.
