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
- Only force-push a branch you exclusively own, and only with
  `git push --force-with-lease` (never `--force`). Never rewrite or force-push `main` or
  any branch someone else has commits on.

## Repository map

- `crates/rune-engine/` — rules engine (layer 3). Immutable state machine.
- `crates/rune-protocol/` — GameView/Action types shared by server and clients.
- `crates/rune-server/` — matchmaking + rooms (layers 1–2), wraps the engine.
- `crates/rune-cli/` — terminal client; proves the protocol without a UI.
- `clients/web/` — React + Pixi client. Has its own `AGENTS.md`.
- `docs/` — brief, protocol spec, card schema, UI requirements, ADRs (`docs/decisions/`).
- `prototypes/` — reference-only HTML prototypes. Never import from here.

Nested instructions: `crates/rune-engine/AGENTS.md`, `clients/web/AGENTS.md`.

## Commands

- `make check` — the fast inner-loop gate (Engine + Client CI jobs). Run it constantly
  while implementing; it must pass before every PR.
- `make verify` — the complete pre-merge gate. Composes `make check` + `make deny`, so its
  coverage matches every required GitHub check (`Engine`, `Client`, `cargo-deny`). Run it
  before opening a PR.
- `make engine-test` — `cargo test --workspace`
- `make engine-lint` — `cargo fmt --check` + `cargo clippy -- -D warnings`
- `make client-check` — lint + typecheck + test + build in `clients/web`
- `make deny` — `cargo deny check advisories licenses bans sources` (the `cargo-deny` job)
- `scripts/bootstrap.sh` — one-time prerequisite check for both gates

> `make check` is the fast unit gate, **not** the entire CI surface. The full surface is
> `make check` (Engine + Client) **plus** the `cargo-deny` job — exactly what `make verify`
> runs locally. (The browser E2E suite of ADR 0011 is removed for now; it will return.)

## Workflow

Solo-maintained (one maintainer + Claude). Keep it light:

1. Branch off `main` with a short descriptive name (`feat/…`, `fix/…`, `docs/…`).
2. Commits: Conventional Commits (`feat(engine): …`, `fix(client): …`, `docs: …`).
3. Keep changes small and single-purpose. Add or update tests for everything you change.
4. Definition of done: `make check` green throughout, `make verify` green before opening a
   PR (where the browser suite can run), docs/ADRs updated if behavior or architecture
   changed, no unrelated diffs.
5. Architectural decisions get an ADR in `docs/decisions/` (copy `0000-template.md`).
6. Open a PR when checks are green; merge it once CI passes.
