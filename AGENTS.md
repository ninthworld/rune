# SAGE agent guide

SAGE is a server-authoritative Magic: The Gathering implementation with a pure Rust engine and
a web client. Read [`docs/brief.md`](docs/brief.md) for the product and architecture, and
[`docs/coding-standards.md`](docs/coding-standards.md) before changing code.

**The web client is playable and deliberately ugly.** `clients/web` renders a grey-box lobby
and board — structure and legibility only, no visual design, because none of it can be judged
until the game plays well. Make it good before making it pretty.

## Hard rules

- **Zero game logic in the client.** The client renders `GameView` and sends back an
  `action_id` from `valid_actions[]`. It never computes legality, cost, or effect.
- **Zero I/O in the engine.** `crates/sage-engine` must not depend on tokio, sockets, timers,
  threads, or wall-clock time. Pure functions over immutable `GameState` only. `build.rs` may
  read card files at compile time.
- **Automation policy lives in the server, never the engine.** The engine may expose pure
  rules *predicates*; the loop, the preferences, and the pacing decisions are the server's
  (ADR 0010). Baking a UX judgment into the rules layer is how the engine becomes
  unsustainable — this is the seam most worth protecting.
- **Protocol changes are contract changes.** Update `docs/protocol.md`, `sage-protocol`, and
  the TypeScript mirror in the same PR.
- **The entire client UI must be reconstructable from one `GameView` + pending prompt.** No
  client state is load-bearing across messages.
- **The browser e2e suite is a required gate.** The seams this project is made of — socket,
  protocol, view reconstruction, reconnect — produce integration bugs a headless DOM cannot
  see. Keep it cheap enough to stay (ADR 0011): one thin smoke path as the blocking gate with
  deeper flows non-blocking, the engine gate independent of it, and **never run
  `playwright install`** — consume the preinstalled browser via `PLAYWRIGHT_BROWSERS_PATH`
  with `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`, pinning `executablePath` if the pinned package
  disagrees with the image.
- **E2E proves it works; the maintainer judges whether it is good.** Automated coverage cannot
  answer "is this fun to play." When a change can only be assessed by a person playing it, say
  plainly what you could not verify and leave that judgment to them.
- **Every change ends in a playable state.** Playing is the merge criterion. Do not build the
  second version of something before the first is good enough to play on.
- **ADRs are written after a decision survives contact with working code**, not before. A
  design document written ahead of the code it describes is speculation with a version number.
- **The project ships no card images, no official frames, symbols, watermarks, or WotC
  branding, no exact Oracle text, and no monetization path.** The one exception is the
  player-side, opt-in art pipeline of ADR 0012: the player's own browser may fetch card images
  from a third-party source — the illustration alone inside SAGE's frame, or the whole card
  image — cached device-local only, never committed, bundled, served, proxied, or
  redistributed.
- **Don't let a file grow past ~800–1000 lines.** Split along cohesive seams into submodules
  with root re-exports (see `docs/coding-standards.md`, File size).
- Never commit secrets, `.env` files, `node_modules/`, or `target/`.
- Only force-push a branch you exclusively own, using `--force-with-lease`. Never rewrite
  `main` or a shared branch.

## Repository map

- `crates/sage-engine/` — pure rules engine; has its own `AGENTS.md`.
- `crates/sage-protocol/` — shared wire types.
- `crates/sage-server/` — WebSocket lobby and game rooms.
- `crates/sage-cli/` — terminal and deterministic-agent client; the playtest surface until the
  web client is playable.
- `clients/web/` — the browser client; has its own `AGENTS.md`.
- `docs/` — brief, protocol, card schema, coding standards, and ADRs. Everything in `docs/` is
  current and binding; there is no superseded material to sift.

## Commands

- `make check` — fast engine gate: `engine-lint` + `engine-test`. Needs no node.
- `make verify` — complete pre-merge gate: `check` + `client-check` + `e2e-smoke` + `deny`.
- `make client-check` — everything the `Client` CI job runs.
- `make e2e-smoke` — the blocking browser gate, against a real server.
- `make e2e-views` — the broad, non-blocking browser tier. Needs no server and no Rust.
- `make engine-test` — `cargo test --workspace`
- `make engine-lint` — `cargo fmt --check` + `cargo clippy -- -D warnings`
- `make compat` — regenerate the card-compatibility report (fails `make check` on drift).
- `make deny` — dependency policy and advisory checks.
- `scripts/bootstrap.sh` — verify local prerequisites.

## Workflow

1. Branch off `main` with a short descriptive name (`feat/…`, `fix/…`, `docs/…`).
2. Commits: Conventional Commits (`feat(engine): …`, `fix(client): …`, `docs: …`).
3. Keep changes small and single-purpose. Add or update tests for everything you change.
4. Run `make check` while working and `make verify` before opening a PR.
5. Update `docs/protocol.md` and `docs/card-schema.md` when behavior changes; keep unrelated
   diffs out.
6. Open a PR when checks are green; merge only after required CI passes.

Keep each `AGENTS.md` under 200 lines and limited to instructions that apply whenever an agent
works in its scope. Put task-specific rationale and reference material in linked documentation.
