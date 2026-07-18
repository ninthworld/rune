# RUNE agent guide

RUNE is a server-authoritative Magic: The Gathering implementation with a pure Rust
engine and a React/Pixi web client. Read [`docs/coding-standards.md`](docs/coding-standards.md)
before changing code; [`docs/brief.md`](docs/brief.md) defines the product and architecture.

## Hard rules

- **Zero game logic in the client.** The client renders `GameView` and sends back an
  `action_id` from `valid_actions[]`. It never computes legality, cost, or effect.
- **Zero I/O in the engine.** `crates/rune-engine` must not depend on tokio, sockets,
  timers, or rooms. Pure functions over immutable `GameState` only.
- **Protocol changes are contract changes.** Update `docs/protocol.md`,
  `rune-protocol`, and the TypeScript mirror in the same PR.
- **The entire client UI must be reconstructable from one `GameView` + pending prompt.**
  No client state is load-bearing across messages.
- **No card images shipped by the project, no official frames, no WotC branding, no
  monetization paths.** See `docs/brief.md` (Legal Considerations) before touching card
  data or rendering. The only exception is the player-side, opt-in art pipeline of
  ADR 0024: the player's own browser may fetch card images from a third-party source,
  cached device-local only — never committed, bundled, served, or redistributed.
- Never commit secrets, `.env` files, `node_modules/`, or `target/`.
- Only force-push a branch you exclusively own, using `--force-with-lease`. Never
  rewrite `main` or a shared branch.

## Repository map

- `crates/rune-engine/` — pure rules engine; has its own `AGENTS.md`.
- `crates/rune-protocol/` — shared wire types.
- `crates/rune-server/` — WebSocket lobby and game rooms.
- `crates/rune-cli/` — terminal and deterministic-agent client.
- `clients/web/` — React/Pixi client; has its own `AGENTS.md`.
- `docs/` — current specifications, design requirements, roadmap, and ADRs.
- `prototypes/` — reference-only HTML prototypes. Never import from here.

## Commands

- `make check` — fast Engine and Client gate.
- `make verify` — complete pre-merge gate: `make check` plus `cargo-deny`.
- `make engine-test` — `cargo test --workspace`
- `make engine-lint` — `cargo fmt --check` + `cargo clippy -- -D warnings`
- `make client-check` — lint + typecheck + test + build in `clients/web`
- `make deny` — dependency policy and advisory checks.
- `scripts/bootstrap.sh` — verify local prerequisites.

## Workflow

1. Branch off `main` with a short descriptive name (`feat/…`, `fix/…`, `docs/…`).
2. Commits: Conventional Commits (`feat(engine): …`, `fix(client): …`, `docs: …`).
3. Keep changes small and single-purpose. Add or update tests for everything you change.
4. Run `make check` while working and `make verify` before opening a PR.
5. Architectural decisions get an ADR in `docs/decisions/` (copy `0000-template.md`).
6. Update specifications when behavior changes; keep unrelated diffs out.
7. Open a PR when checks are green; merge only after required CI passes.

Keep each `AGENTS.md` under 200 lines and limited to instructions that apply whenever an agent
works in its scope. Put task-specific rationale and reference material in linked documentation.
