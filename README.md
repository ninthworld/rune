# RUNE

RUNE is an open-source, server-authoritative Magic: The Gathering implementation. A
pure Rust engine owns the rules, a WebSocket server owns sessions and rooms, and React,
Pixi, terminal, or automated clients render personalized server views and return issued
action identifiers.

The project is in active development. The engine can play a deterministic two-player
creature-combat game to a win, including casting, targeting, the stack, combat, common
keywords, triggers, auras, and initial replacement effects. The server supports rooms,
validated decks, reconnect tokens, and decision timers. The web client covers the game
loop, but its table presentation and interaction affordances still have open usability
issues. See the [roadmap](docs/roadmap.md) for current work.

## Architecture

```text
┌────────────────────────── rune-server ──────────────────────────┐
│ Lobby and rooms       WebSocket sessions and server policy     │
│ rune-engine           Pure, immutable rules state machine      │
└─────────────────────────────┬───────────────────────────────────┘
                  LobbyView / GameView ↓  ↑ command / action id
          ┌───────────────────┼───────────────────┐
          │ React + Pixi web  │ terminal client   │ automated agent
          └───────────────────┴───────────────────┘
```

- The engine has no runtime I/O and produces a new `GameState` for each action.
- The server redacts hidden information and sends a complete personalized view after
  each change.
- Clients derive interactivity only from `valid_commands` or `valid_actions`; they do
  not compute rules or legality.
- Card definitions are structured data. The server generates display rules text from
  the same data the engine executes.

See the [project brief](docs/brief.md) for scope and the
[protocol specification](docs/protocol.md) for the wire contract.

## Repository

| Path | Purpose |
| --- | --- |
| `crates/rune-engine` | Pure rules engine and embedded card catalog |
| `crates/rune-protocol` | Shared Rust wire types |
| `crates/rune-server` | WebSocket lobby, rooms, and view projection |
| `crates/rune-cli` | Interactive terminal and deterministic-agent client |
| `clients/web` | React and Pixi web client |
| `docs` | Specifications, design requirements, roadmap, and ADRs |
| `prototypes` | Historical UI references; never imported by production code |

## Set up and verify

```sh
scripts/bootstrap.sh
make check
make verify
```

`make check` is the fast Engine and Client gate. `make verify` adds dependency-policy
checks and matches the required pre-merge CI surface.

## Run locally

Start the server:

```sh
cargo run -p rune-server
```

It listens on `127.0.0.1:9000` by default. Use `--addr` or `RUNE_SERVER_ADDR` to
override it:

```sh
cargo run -p rune-server -- --addr 0.0.0.0:9000
```

In another terminal, start an interactive terminal client or the deterministic agent:

```sh
cargo run -p rune-cli
cargo run -p rune-cli -- --agent
```

The CLI accepts `--addr`, `--agent`, and `--agent-timeout`; corresponding environment
fallbacks are documented by `--help`.

To run the web client:

```sh
cd clients/web
npm install
npm run dev
```

Vite serves the development client at `http://localhost:5173` by default. Use
`npm run build` and `npm run preview` to test a production bundle.

## Documentation

- [Project brief](docs/brief.md) — purpose, architecture, scope, and legal constraints
- [Protocol](docs/protocol.md) — current lobby and in-game wire contract
- [Card schema](docs/card-schema.md) — authoring and validation of card definitions
- [UI requirements](docs/design/ui-requirements.md) — current and future UI capabilities
- [Roadmap](docs/roadmap.md) — shipped outcomes and remaining milestones
- [ADRs](docs/decisions/) — architectural decisions and their rationale

## Legal

RUNE is a free fan project and is not affiliated with or endorsed by Wizards of the
Coast. It includes no card images, official frames, Wizards branding, or exact Oracle
text and must not be monetized. Cards use structured functional definitions and
server-generated rules text. See the [legal constraints](docs/brief.md#legal-constraints).

The source code is licensed under the [MIT License](LICENSE).
