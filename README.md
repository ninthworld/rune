# SAGE

SAGE — **S**erver **A**uthoritative **G**ame **E**ngine — is an open-source implementation of
Magic: The Gathering. A pure Rust engine owns the rules, a WebSocket server owns sessions and
rooms, and clients render personalized server views and return issued action identifiers.

**The long-term goal is XMage in the browser** — comparable rules and card coverage, on a pure
state-based server-authoritative engine, reachable without an install — and *then* to make it
beautiful. The first milestone is the vertical slice of that: two people click a link and play
a real game in a browser.

> **Status: the browser client is playable, and now looks like a table.** A dark board of real
> card frames drawn from structured data, a lobby, a deck editor, and a settings panel. Several
> surfaces are drawn but inert because nothing on the wire carries them yet — chat, the
> who-is-here roster, and the per-table undo rule. Visual polish still comes after the game is
> good to play, not before.

The engine plays deterministic multiplayer games to a single winner: casting, targeting, the
stack, combat with per-attacker targets and player-chosen damage assignment, elimination, common
keywords, triggers, auras, counters, tokens, planeswalkers, and emblems. How many seats a game
has is the format's decision, not a global constant — the lobby plumbs 2–8, and every format's
name says which game it seats: `starter-1v1` and `standard_2p` (the web client's `1v1`) seat
exactly 2, `standard_ffa` seats 3–4, `commander` seats 2–4, and the permissive
`standard_multiplayer` catch-all is the one that allows the full 2–8 range. The server provides rooms,
validated decks, reconnect tokens, decision timers, priority automation (the *settle*), and
spectators.

Replacement effects exist only as a card's own enters-the-battlefield self-replacements (CR
614.1c — "enters tapped", "enters with counters"). There is no general replacement layer, so
damage prevention, regeneration, and cost modification are out of scope for now.

The card IR's expressive vocabulary — not authoring throughput — is the current constraint on
catalog growth. Growing it is the primary engine workstream. The catalog holds a verified slice
of Core Set 2019, never a full set; the generated
[compatibility report](docs/generated/compatibility.md) is the checkable list of exactly which
cards are supported and which mechanics are deliberately excluded, with the single blocker
behind each exclusion. It is regenerated from the catalog, and `make check` fails if it drifts.

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
| `clients/prototype` | Throwaway design sandbox; nothing ships from it |
| `docs` | Brief, protocol, card schema, coding standards, and ADRs |

## Set up and verify

```sh
scripts/bootstrap.sh
make check
make verify
```

`make check` is the fast engine gate and needs no node toolchain. `make verify` adds the
client gate, the blocking browser smoke path, and dependency-policy checks, matching the
required pre-merge CI surface. `make e2e-views` runs the broader, non-blocking browser tier.

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
are documented by `--help`.

For the browser client, with the server running:

```sh
cd clients/web && npm install && npm run dev
```

Create a table, build or pick a deck, seat an AI opponent, and ready up.

There is no hosted deployment and no deploy pipeline: SAGE runs locally, from source. CI runs
the engine, client, dependency-policy, and browser gates only.

## Known gaps

Named here so the summary above is not read as more than it is:

- **Chat** — no lobby or table chat exists on the wire. The client draws the tab and says so.
- **Presence roster** — `LobbyView` states the directory, the room, and your own seat, and
  nothing about who else is where. The lobby's *Players* and the room's *Watching* tabs show
  the counts the server does state and an empty list otherwise.
- **Spectators** — the server implements them (join, redacted `SpectatorView`, an advertised
  count), but a spectator's identity is never published, so no watcher list can be shown.
- **Undo** — offered as a table rule at creation and restated in the room, with no command
  behind it. Nothing is undone today.
- **Deck building** — an in-client editor over one flat decklist. There is no sideboard on the
  wire, no saved decks, and no import format.
- **Replacement effects, cost modification, and X costs** — see the compatibility report.
- **A legible settle** — the brief names this as the product hypothesis; today a settle is a run
  of log lines. It, and the gaps above, are tracked as open questions in
  [`docs/client-design.md` §10](docs/client-design.md).

## Documentation

- [Project brief](docs/brief.md) — purpose, architecture, scope, and legal constraints
- [Protocol](docs/protocol.md) — current lobby and in-game wire contract
- [Client design](docs/client-design.md) — how the client occupies space, and what is still open
- [Card schema](docs/card-schema.md) — authoring and validation of card definitions
- [Compatibility report](docs/compatibility-report.md) — how support claims are generated, and
  the [generated report](docs/generated/compatibility.md) itself
- [ADRs](docs/decisions/) — architectural decisions and their rationale

## Legal

SAGE is a free fan project and is not affiliated with or endorsed by Wizards of the Coast. It
distributes no card images, official frames, Wizards branding, or exact Oracle text, and must
not be monetized. Cards use structured functional definitions and server-generated rules text.
A player may separately opt in, on their own device, to their browser fetching card images from
a third-party source; those images are never redistributed by the project. See the
[legal constraints](docs/brief.md#legal-constraints).

The source code is licensed under the [MIT License](LICENSE).
