# SAGE

SAGE — **S**erver **A**uthoritative **G**ame **E**ngine — is an open-source implementation of
Magic: The Gathering. A pure Rust engine owns the rules, a WebSocket server owns sessions and
rooms, and clients render personalized server views and return issued action identifiers.

**The long-term goal is XMage in the browser** — comparable rules and card coverage, on a pure
state-based server-authoritative engine, reachable without an install — and *then* to make it
beautiful. The first milestone is the vertical slice of that: two people click a link and play
a real game in a browser.

> **Status: the browser client is being built.** The engine, server, protocol, and terminal
> client work today. `clients/web` currently holds the protocol mirror and its toolchain, not a
> playable surface — until it is playable, `sage-cli` is how you play.

The engine plays deterministic games of two to four players to a single winner: casting,
targeting, the stack, combat with per-attacker targets and player-chosen damage assignment,
elimination, common keywords, triggers, auras, counters, and initial replacement effects. The
server provides rooms, validated decks, reconnect tokens, decision timers, priority automation,
free-for-all and commander formats, and spectators.

The card IR's expressive vocabulary — not authoring throughput — is the current constraint on
catalog growth. Growing it is the primary engine workstream.

## Architecture

```text
┌────────────────────────── sage-server ─────────────────────────┐
│ Lobby and rooms       WebSocket sessions and server policy     │
│ sage-engine           Pure, immutable rules state machine      │
└─────────────────────────────┬──────────────────────────────────┘
                  LobbyView / GameView ↓  ↑ command / action id
              ┌───────────────┴───────────────┐
              │ web client    terminal client │ automated agent
              └───────────────────────────────┘
```

- The engine has no runtime I/O and produces a new `GameState` for each action.
- The server redacts hidden information and sends a complete personalized view after each
  change. All automation *policy* lives here; the engine only answers rules questions.
- Clients derive interactivity only from `valid_commands` or `valid_actions`; they never
  compute rules or legality.
- Card definitions are structured data. The server generates display rules text from the same
  data the engine executes.

See the [project brief](docs/brief.md) for scope and the
[protocol specification](docs/protocol.md) for the wire contract.

## Repository

| Path | Purpose |
| --- | --- |
| `crates/sage-engine` | Pure rules engine and embedded card catalog |
| `crates/sage-protocol` | Shared Rust wire types |
| `crates/sage-server` | WebSocket lobby, rooms, and view projection |
| `crates/sage-cli` | Interactive terminal and deterministic-agent client |
| `clients/web` | Browser client and the TypeScript protocol mirror |
| `docs` | Brief, protocol, card schema, coding standards, and ADRs |

## Set up and verify

```sh
scripts/bootstrap.sh
make check
make verify
```

`make check` is the fast engine gate and needs no node toolchain. `make verify` adds the
client gate and dependency-policy checks, matching the required pre-merge CI surface. The
browser-e2e gate lands with the first playable surface.

## Run locally

Start the server:

```sh
cargo run -p sage-server
```

It listens on `127.0.0.1:9000` by default. Use `--addr` or `SAGE_SERVER_ADDR` to override it:

```sh
cargo run -p sage-server -- --addr 0.0.0.0:9000
```

In another terminal, start an interactive terminal client or the deterministic agent:

```sh
cargo run -p sage-cli
cargo run -p sage-cli -- --agent
```

The CLI accepts `--addr`, `--agent`, and `--agent-timeout`; corresponding environment fallbacks
are documented by `--help`. Until the web client is playable, the CLI is the way to play.

## Documentation

- [Project brief](docs/brief.md) — purpose, architecture, scope, and legal constraints
- [Protocol](docs/protocol.md) — current lobby and in-game wire contract
- [Card schema](docs/card-schema.md) — authoring and validation of card definitions
- [Compatibility report](docs/compatibility-report.md) — how support claims are generated
- [ADRs](docs/decisions/) — architectural decisions and their rationale

## Legal

SAGE is a free fan project and is not affiliated with or endorsed by Wizards of the Coast. It
distributes no card images, official frames, Wizards branding, or exact Oracle text, and must
not be monetized. Cards use structured functional definitions and server-generated rules text.
A player may separately opt in, on their own device, to their browser fetching card images from
a third-party source; those images are never redistributed by the project. See the
[legal constraints](docs/brief.md#legal-constraints).

The source code is licensed under the [MIT License](LICENSE).
