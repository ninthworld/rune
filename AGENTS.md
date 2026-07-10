# AGENTS.md — RUNE

RUNE is an open-source Magic: The Gathering implementation: a Rust server that owns
all game logic and a React/Pixi web client that is a "dumb" renderer. They speak a
two-message JSON/WebSocket protocol. Full context: `docs/brief.md`.

All code follows `docs/coding-standards.md` (enforced by `make check`) — read it
before writing code.

## Hard rules

- **Zero game logic in the client.** The client renders `GameView` and sends back an
  `action_id` from `valid_actions[]`. It never computes legality, cost, or effect.
- **Zero I/O in the engine.** `crates/rune-engine` must not depend on tokio, sockets,
  timers, or rooms. Pure functions over immutable `GameState` only.
- **Protocol changes are contract changes.** Any change to message shapes requires
  updating `docs/protocol.md` and the `rune-protocol` crate in the same PR.
- **The entire client UI must be reconstructable from one `GameView` + pending prompt.**
  No client state is load-bearing across messages.
- **No card images, no official frames, no WotC branding, no monetization paths.**
  See `docs/brief.md` (Legal Considerations) before touching card data or rendering.
- Never commit secrets, `.env` files, `node_modules/`, or `target/`.
- Never force-push shared branches. Never merge your own PR.

## Repository map

- `crates/rune-engine/` — rules engine (layer 3). Immutable state machine.
- `crates/rune-protocol/` — GameView/Action types shared by server and clients.
- `crates/rune-server/` — matchmaking + rooms (layers 1–2), wraps the engine.
- `crates/rune-cli/` — terminal client; proves the protocol without a UI.
- `clients/web/` — React + Pixi client. Has its own `AGENTS.md`.
- `docs/` — brief, protocol spec, UI requirements, ADRs (`docs/decisions/`).
- `prototypes/` — reference-only HTML prototypes. Never import from here.

Nested instructions: `crates/rune-engine/AGENTS.md`, `clients/web/AGENTS.md`.

## Commands

- `make check` — everything CI runs. Must pass before every PR.
- `make engine-test` — `cargo test --workspace`
- `make engine-lint` — `cargo fmt --check` + `cargo clippy -- -D warnings`
- `make client-check` — typecheck + build in `clients/web`
- `scripts/bootstrap.sh` — one-time toolchain setup

## Workflow

1. Work from a GitHub issue. If none exists for your task, create one using the
   agent-task template with acceptance criteria before writing code.
2. Branch: `agent/<issue-number>-<short-slug>`.
3. Commits: Conventional Commits (`feat(engine): …`, `fix(client): …`, `docs: …`).
4. Keep PRs small and single-purpose. Fill in the PR template; link the issue with
   `Closes #N`. Add or update tests for everything you change.
5. Definition of done: `make check` green, docs/ADRs updated if behavior or
   architecture changed, PR description explains what and why, no unrelated diffs.
6. Architectural decisions get an ADR in `docs/decisions/` (copy `0000-template.md`).

Process details and GitHub settings: `docs/agents/workflow.md`.
