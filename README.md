# RUNE

An open-source Magic: The Gathering engine and client. A high-performance Rust server
owns every rule of the game; a React + Pixi.js web client renders what the server says
and nothing more. Any client — web UI, terminal, or an LLM agent — speaks the same
two-message protocol.

> **Status: pre-alpha scaffold.** Structure, contracts, and CI exist; the engine and
> client are stubs. See [`docs/agents/backlog.md`](docs/agents/backlog.md) for what's next.

## Architecture

```
┌─────────────────────────── rune-server ───────────────────────────┐
│  Layer 1  Matchmaking / lobby      (connections, identity, rooms) │
│  Layer 2  Room / session           (one task per room, timers)    │
│  Layer 3  rune-engine              (pure, immutable rules engine) │
└──────────────────────────────┬─────────────────────────────────────┘
                     GameView ↓ │ ↑ { action_id }        (rune-protocol)
        ┌──────────────┬────────┴───────┬────────────────┐
        │ clients/web  │   rune-cli     │   LLM agent    │
        │ React + Pixi │   terminal     │   (any client) │
        └──────────────┴────────────────┴────────────────┘
```

Design principles (full detail in [`docs/brief.md`](docs/brief.md)):

- **Server-authoritative.** The client only renders `GameView` and picks from
  `valid_actions[]`. Zero rules knowledge lives client-side.
- **Immutable engine.** `apply_action(state, action) -> GameState`. Undo, replay,
  resync, and AI tree search fall out of this for free.
- **No card images.** Cards are procedurally rendered from data — legal clarity and
  crisp scaling at every size.

## Getting started

```sh
scripts/bootstrap.sh   # checks toolchains (Rust stable, Node 20+)
make check             # everything CI runs: fmt, clippy, tests, client build
```

| Directory | What it is |
|---|---|
| `crates/rune-engine` | Rules engine — pure functions, immutable state |
| `crates/rune-protocol` | Shared GameView / Action types |
| `crates/rune-server` | WebSocket server: lobby, rooms, sessions |
| `crates/rune-cli` | Terminal client for protocol testing |
| `clients/web` | React + Pixi.js web client |
| `docs` | Brief, protocol, UI requirements, ADRs |
| `prototypes` | Standalone HTML UI prototypes (reference only) |

## Development model

This repository is primarily developed by AI coding agents working through GitHub
issues and pull requests, with humans reviewing and merging. The contract for agents
lives in [`AGENTS.md`](AGENTS.md); the process lives in
[`docs/agents/workflow.md`](docs/agents/workflow.md). Every PR must pass CI
(`Engine` and `Client` checks) and human review — nothing merges automatically.

## Documentation

- [`docs/brief.md`](docs/brief.md) — the project brief (architecture, scope, legal)
- [`docs/protocol.md`](docs/protocol.md) — the two-message client/server protocol
- [`docs/design/ui-requirements.md`](docs/design/ui-requirements.md) — everything the UI must support
- [`docs/design/ui-design-notes.md`](docs/design/ui-design-notes.md) — locked UI design decisions
- [`docs/decisions/`](docs/decisions/) — architecture decision records

## Legal

RUNE is a free fan project, not affiliated with or endorsed by Wizards of the Coast.
It implements game rules (not copyrightable), uses no card images or official frame
designs, and must never be monetized. Card oracle text is used under the same
tolerated-fan-project posture as long-standing prior art (XMage, Forge). License
selection is tracked in [`docs/decisions/0005-license.md`](docs/decisions/0005-license.md).
