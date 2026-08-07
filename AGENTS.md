# SAGE agent guide

SAGE is a server-authoritative Magic: The Gathering implementation with a pure Rust engine and
a web client. Read [`docs/brief.md`](docs/brief.md) for the product and architecture, and
[`docs/coding-standards.md`](docs/coding-standards.md) before changing code.

**The web client is a dark table, not a form.** `clients/web` renders real card frames —
printed proportions, a tint from the printed cost, mana pips, an art window — arranged as a board:
chrome at the edges, each seat's permanents in creature and land rows the *server's* `card_types`
decide, and one gear for everything about the device rather than the game. The directness goes
with it: one click takes an action the server offered exactly one of, the pointer previews a card,
and the keyboard carries priority. **How it occupies space is
[`docs/client-design.md`](docs/client-design.md)**, which is binding on `clients/web`: zoom,
resolution, and aspect are the same problem; no region of the board scrolls vertically or ever
grows a scrollbar, and a full row pans sideways instead; text is fitted, never truncated.

**For appearance, the order is prototype → `client-design.md` → `clients/web`.** The document
records what `clients/prototype` settled, and where the two disagree the prototype wins; the
shipping client is built to the document. That ordering covers appearance only — it never
overrides a hard rule below or an accepted ADR. Stated in full in
[`docs/brief.md`](docs/brief.md#which-document-wins). Playing is still the merge criterion: make
it good before making it pretty, and nothing visual is worth a rule in `interaction.ts` bending
for it.

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
  deeper flows non-blocking, and the engine gate independent of it. **The browser is the one
  the pinned `@playwright/test` names, in every environment** — `make e2e-browser` provisions
  exactly that revision and is a no-op where it already exists, including the CI container.
  Never fetch a browser the driver did not ask for (`playwright@latest`, a caret range, a
  hand-picked revision), and never pin an `executablePath`: both are how a local run and CI
  end up driving different binaries.
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
- `crates/sage-cli/` — terminal and deterministic-agent client. It proves the protocol is
  independent of the web UI and is the playtest surface whenever the browser is unavailable.
- `crates/sage-scenario/` — **development-only** contributor tool: builds an exact game
  position from a checked-in file and opens the real client on it, for playtesting one
  mechanic without playing to it. Nothing ships from it and no shipped crate depends on it.
  Format and vocabulary: [`docs/scenarios.md`](docs/scenarios.md); examples in `scenarios/`.
- `clients/web/` — the browser client, and the playable one; has its own `AGENTS.md`.
- `clients/prototype/` — a throwaway sandbox for trying screens before building them for real.
  Nothing ships from it and `docs/client-design.md` does not govern it.
- `docs/` — brief, protocol, card schema, scenarios, coding standards, and ADRs. Everything in `docs/` is
  current and binding; there is no superseded material to sift. The one precedence question —
  prototype vs. `client-design.md` vs. `clients/web` — is answered above and in the brief.
- `docs/generated/` — generated artifacts. Never hand-edit one; regenerate it (`make compat`).

## Commands

- `make check` — fast engine gate: `engine-lint` + `engine-test`. Needs no node.
- `make verify` — complete pre-merge gate: `check` + `client-check` + `e2e-smoke` + `deny`.
- `make client-check` — everything the `Client` CI job runs.
- `make e2e-smoke` — the blocking browser gate, against a real server.
- `make e2e-views` — the broad, non-blocking browser tier. Needs no server and no Rust.
- `make e2e-scenario` — the non-blocking scenario tier, against the contributor runner.
- `make scenario SCENARIO=<file>` — open the real client on an exact position and print the
  URL (`docs/scenarios.md`). Development only, disposable, loopback only.
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
