# RUNE

An open-source Magic: The Gathering engine and client. A high-performance Rust server
owns every rule of the game; a React + Pixi.js web client renders what the server says
and nothing more. Any client — web UI, terminal, or an LLM agent — speaks the same
two-message protocol.

> **Status: pre-alpha scaffold.** Structure, contracts, and CI exist; the engine and
> client are stubs. See [`docs/roadmap.md`](docs/roadmap.md) for the milestones and
> what's next.

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

## Running the project

The server owns the game; every client connects to it over WebSocket. Start the
server first, then attach a client. (Pre-alpha: the engine and client are still
stubs, so these run but don't yet play a full game.)

### Server

```sh
cargo run -p rune-server
```

Binds `127.0.0.1:9000` by default. Override the listen address with the `--addr`
flag or the `RUNE_SERVER_ADDR` environment variable:

```sh
cargo run -p rune-server -- --addr 0.0.0.0:9000
RUNE_SERVER_ADDR=0.0.0.0:9000 cargo run -p rune-server
```

The server logs to stderr and serves until Ctrl-C.

### Terminal client (`rune-cli`)

With a server running, connect the terminal client. Interactive mode renders each
`GameView` as a numbered list of legal actions and reads your choice from stdin:

```sh
cargo run -p rune-cli
```

Agent mode (`--agent`) hands each view to the built-in deterministic agent instead
of prompting — useful for smoke-testing AI opponents:

```sh
cargo run -p rune-cli -- --agent
```

Flags (each has an environment-variable fallback):

| Flag | Env fallback | Default | Purpose |
|---|---|---|---|
| `--addr`, `-a` `<host:port \| ws://…>` | `RUNE_SERVER_ADDR` | `127.0.0.1:9000` | Server to connect to |
| `--agent` | — | off | Drive with the built-in agent instead of stdin |
| `--agent-timeout <seconds>` | `RUNE_AGENT_TIMEOUT` | `5` | Per-decision deadline in agent mode |

### Web client (`clients/web`)

The React + Pixi.js client is a Vite app. Install dependencies once, then run the
dev server:

```sh
cd clients/web
npm install
npm run dev            # Vite dev server with hot reload (default http://localhost:5173)
```

Build and preview a production bundle:

```sh
npm run build          # type-check, then emit a production build to dist/
npm run preview        # serve the built bundle locally
```

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
tolerated-fan-project posture as long-standing prior art (XMage, Forge).

The source code is licensed under the [MIT License](LICENSE); see
[`docs/decisions/0005-license.md`](docs/decisions/0005-license.md) for the rationale.
The MIT license governs the code only — the non-monetization and no-card-images
posture above still applies to how the project is distributed.
