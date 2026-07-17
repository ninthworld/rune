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
- **No card images, no official frames, no WotC branding, no monetization paths.**
  See `docs/brief.md` (Legal Considerations) before touching card data or rendering.
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
- `make smoke` — browser smoke canary (ADR 0011): one Playwright spec drives a real
  Chromium against a real seeded `rune-server` and plays real turns through the rendered
  UI (the StrictMode canvas-attach guard, #276). The `Smoke` CI job; part of `make verify`,
  not `make check`. Needs a Chromium — locally it uses a pre-installed one when present,
  else run `cd clients/web && npx playwright install chromium` once.
- `scripts/bootstrap.sh` — verify local prerequisites.

> `make check` is the fast unit gate, **not** the whole CI surface — that is `make check`
> (Engine + Client) **plus** the `cargo-deny` job **plus** the browser `Smoke` canary (ADR
> 0011), exactly what `make verify` runs locally. The smoke canary stays outside
> `make check`, which stays fast and browser-free.

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
