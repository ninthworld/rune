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
- Only a branch you exclusively own — your own `agent/<issue>-<slug>` branch that no
  one else is committing to — may be rebased and force-pushed, and only with
  `git push --force-with-lease` (never `--force`). This is how you bring a stale PR
  current onto `main` (see "Handling stale branches" in `docs/agents/workflow.md`).
  Never rewrite or force-push a shared branch — `main`, or any branch another agent or
  human has commits on. Never merge your own PR; the `main` ruleset requires a separate
  human approval.

## Repository map

- `crates/rune-engine/` — rules engine (layer 3). Immutable state machine.
- `crates/rune-protocol/` — GameView/Action types shared by server and clients.
- `crates/rune-server/` — matchmaking + rooms (layers 1–2), wraps the engine.
- `crates/rune-cli/` — terminal client; proves the protocol without a UI.
- `clients/web/` — React + Pixi client. Has its own `AGENTS.md`.
- `docs/` — brief, protocol spec, card schema, UI requirements, ADRs (`docs/decisions/`).
- `tools/agent-task/` — the issue runner (ADR 0016, `scripts/agent-task`) and the
  milestone stewardship cycle (ADR 0017, `scripts/agent-cycle`, `cycle-*.js`), which is a
  second consumer of the runner's own primitives rather than a parallel toolchain.
  Dependency-free Node; never add a dependency to it.
- `tools/ci-policy/` — the workflow gate (`make ci-lint`): immutable Action pins,
  least-privilege tokens, no untrusted interpolation. Dependency-free Node; same rule.
- `prototypes/` — reference-only HTML prototypes. Never import from here.

Nested instructions: `crates/rune-engine/AGENTS.md`, `clients/web/AGENTS.md`.

## Commands

- `make check` — the fast inner-loop gate (Engine + Client CI jobs). Run it constantly
  while implementing; it must pass before every PR.
- `make verify` — the complete pre-merge gate. Composes `make check` + `make e2e` +
  `make deny`, so its coverage matches every required GitHub check (`Engine`, `Client`,
  `E2E`, `cargo-deny`). Run it before requesting final review (when the environment can
  run the browser suite).
- `make engine-test` — `cargo test --workspace`
- `make engine-lint` — `cargo fmt --check` + `cargo clippy -- -D warnings`
- `make client-check` — lint + typecheck + test + build in `clients/web`
- `make e2e` — browser end-to-end suite (its own `E2E` CI job, **not** part of
  `make check`; needs a browser + built client — see `docs/decisions/0011-*.md`)
- `make deny` — `cargo deny check advisories licenses bans sources` (the `cargo-deny` job)
- `make ci-lint` — workflow gate: actionlint + `tools/ci-policy` (immutable Action pins,
  least-privilege tokens). Runs in the `cargo-deny` job; needs `actionlint` on `PATH`.
- `scripts/bootstrap.sh` — one-time prerequisite check for both gates
- `make e2e-browser` — install the pinned Playwright Chromium the E2E suite needs

> `make check` is the fast unit gate, **not** the entire CI surface. The full surface is
> `make check` (Engine + Client) **plus** the separate `E2E` job (ADR 0011) **plus** the
> `cargo-deny` job — exactly what `make verify` runs locally.

## Workflow

1. Work from a GitHub issue — a `status:ready` **leaf** issue whose `Blocked by:` list is
   closed, and nothing else. If none exists for your task, create one using the agent-task
   template with acceptance criteria before writing code.
2. Branch: `agent/<issue-number>-<short-slug>`.
3. Commits: Conventional Commits (`feat(engine): …`, `fix(client): …`, `docs: …`).
4. **One leaf issue → one PR**, in both directions. Keep PRs small and single-purpose;
   fill in the PR template; link the issue with `Closes #N` (exactly one). An outcome too
   big for one PR is a parent issue that needs decomposing, not a second PR against the
   same issue. Add or update tests for everything you change.
5. Definition of done: `make check` green throughout implementation and `make verify`
   green before final review (where the browser suite can run), docs/ADRs updated if
   behavior or architecture changed, PR description explains what and why, no unrelated
   diffs.
6. Architectural decisions get an ADR in `docs/decisions/` (copy `0000-template.md`).
7. Your job ends at "green checks + a PR ready for review." Agents never approve and
   never merge — not their own PRs and not another agent's.

The end-to-end lifecycle (milestone → issue → PR, and the human gates between them):
`docs/agents/continuance.md`. Commands, labels, and GitHub settings:
`docs/agents/workflow.md`.
