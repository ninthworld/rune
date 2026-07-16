# RUNE

An open-source Magic: The Gathering engine and client. A high-performance Rust server
owns every rule of the game; a React + Pixi.js web client renders what the server says
and nothing more. Any client — web UI, terminal, or an LLM agent — speaks the same
two-message protocol.

> **Status: early, playable at the engine level.** The Rust engine plays a complete,
> legal, deterministic game of a creature-combat MTG subset to a win — proven by an
> agent-vs-agent game driven through the real server and protocol
> (`crates/rune-cli/tests/agent_game.rs`). The web client renders and drives games but is
> still being brought up to a full-game UI, and the card pool is a small hand-authored
> slice. See [`docs/roadmap.md`](docs/roadmap.md) for what's next.

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
scripts/bootstrap.sh   # checks prerequisites for both gates below
make check             # fast inner-loop gate: Engine + Client (fmt, clippy, tests, client build)
make verify            # full pre-merge gate: check + cargo-deny
```

`make check` is the fast gate you run constantly while working. `make verify` is the
complete pre-merge surface: it composes `make check` and `make deny`, so its coverage
matches every GitHub check required to merge (`Engine`, `Client`, `cargo-deny`). Run
`make verify` before opening a PR.

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
server first, then attach a client. The `rune-cli` agent mode plays a full game today;
the web client is still being brought up to a complete-game UI.

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

RUNE is solo-maintained, with Claude as the coding assistant. The loop is light: branch
off `main`, keep changes small and single-purpose, run `make check` while working and
`make verify` before opening a PR, and merge once CI is green. The rules that matter — zero
game logic in the client, zero I/O in the engine, protocol changes are contract changes —
live in [`AGENTS.md`](AGENTS.md); the contributor loop is in
[`CONTRIBUTING.md`](CONTRIBUTING.md). Every PR must pass CI (`Engine`, `Client`, and
`cargo-deny` — reproduce them with `make verify`).

## Documentation

- [`docs/brief.md`](docs/brief.md) — the project brief (architecture, scope, legal)
- [`docs/protocol.md`](docs/protocol.md) — the two-message client/server protocol
- [`docs/design/ui-requirements.md`](docs/design/ui-requirements.md) — everything the UI must support
- [`docs/design/ui-design-notes.md`](docs/design/ui-design-notes.md) — locked UI design decisions
- [`docs/decisions/`](docs/decisions/) — architecture decision records

## Legal

RUNE is a free fan project, not affiliated with or endorsed by Wizards of the Coast.
It implements game rules (not copyrightable), uses no card images or official frame
designs, and must never be monetized. It also bundles **no exact Oracle text**: cards
are authored as structured functional definitions and the server generates the rules
text a player reads from them, so the project does not rely on the oracle-text grey
zone that prior art (XMage, Forge) operates in. See
[ADR 0018](docs/decisions/0018-scalable-functional-card-definitions.md) and the schema
in [`docs/card-schema.md`](docs/card-schema.md).

The source code is licensed under the [MIT License](LICENSE); see
[`docs/decisions/0005-license.md`](docs/decisions/0005-license.md) for the rationale.
The MIT license governs the code only — the non-monetization and no-card-images
posture above still applies to how the project is distributed.
