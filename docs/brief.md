# RUNE project brief

RUNE is an open-source, server-authoritative Magic: The Gathering implementation. Its
purpose is to provide one deterministic rules engine that can support multiple clients:
web, terminal, automated agents, and future desktop or offline shells.

This document defines the product boundaries and architecture. Current wire shapes live
in [`protocol.md`](protocol.md), card authoring lives in
[`card-schema.md`](card-schema.md), and architectural rationale lives in
[`decisions/`](decisions/).

## Product principles

- **One rules authority.** The Rust engine decides legality, costs, effects, state-based
  actions, and outcomes.
- **Replaceable clients.** A client renders a personalized view and returns an action the
  server already offered. Web-specific behavior cannot become a rules dependency.
- **Deterministic state.** Game transitions are pure and reproducible from state, actions,
  card data, and an injected random seed.
- **Complete views.** A client can reconstruct the lobby or game UI from the latest server
  view without retaining authoritative state across messages.
- **Structured cards.** Card behavior is authored as validated data and executed by the
  engine. Display rules text is generated from that behavior.
- **Accessible presentation without official assets.** Cards are rendered from
  server-supplied data, procedurally by default; presentation quality is a product goal
  under the 2.5D direction ([ADR 0029](decisions/0029-2-5d-presentation-direction.md)),
  but never at the cost of accessibility or official-asset constraints.

## Architecture

### Rules engine

`crates/rune-engine` is a pure, single-game state machine. It owns:

- zones, turns, priority, the stack, combat, and game results;
- legal-action generation and action validation;
- card effects, triggers, replacement effects, and computed characteristics;
- state-based actions and deterministic random operations; and
- the embedded functional card catalog.

The engine performs no runtime I/O. It does not know about sockets, rooms, reconnects,
accounts, or wall-clock timers. Its public operations receive all required inputs and
return values or errors. `build.rs` may read card files while compiling; the resulting
catalog is embedded in the binary.

`GameState` stores facts that cannot be derived, such as zone contents, counters, marked
damage, object identities, and the deterministic random stream. Current characteristics
are computed from state and card data rather than cached. Each battlefield entry receives
a fresh `PermanentId`, which provides zone-change identity.

### Server

`crates/rune-server` wraps the engine with network and session concerns:

- WebSocket connections and JSON serialization;
- opaque session tokens and reconnect handling;
- room creation, joining, seats, submitted decks, and the ready gate;
- format and deck-validation policy;
- one task per active room;
- personalized view projection and hidden-information redaction;
- optional decision timers and conservative timeout actions; and
- server-driven AI opponents a host can seat (ADR 0028).

The lobby supports room configurations with 2–8 seats. A bundled free-for-all format
(`standard_ffa`) seats 3–4 players and starts real multiplayer games on the engine's
multiplayer rules (per-attacker attack targets, multi-defender blocking, and elimination),
and a `commander` format seats 2–4 players with singleton and color-identity deck
validation, 40 starting life, and the engine's command-zone mechanics; the two-player
formats remain the default. Room creation validates each format's seat range, and the
lobby serves the card catalog and each format's deck rules for clients to display. A host
may also fill any open seat with a **server-driven AI opponent** (ADR 0028): the AI plays
its seat through the same protocol path a human does — never a rules authority in the
engine, never a client-side bot — so a room may seat any mix of humans and AI at any
supported player count. The first AI kind plays a simple random-legal policy, behind a
seam built for stronger opponents later.

### Protocol

A connection has two phases:

1. Before a game, the server sends complete `LobbyView` values and the client sends tagged
   `LobbyCommand` values.
2. During a game, the server sends personalized `GameView` values and the client returns a
   `ChooseAction` containing an issued action id, its content token, and any requested
   choices.

`valid_commands` and `valid_actions` are the only sources of interactivity. The server
enumerates legal candidates for targets and other prompts; the client displays those
candidates and returns the selection atomically. Invalid or stale input is rejected and a
fresh authoritative view is sent.

The Rust types in `rune-protocol`, their TypeScript mirror, and
[`protocol.md`](protocol.md) form one contract and change together.

### Clients

The web client in `clients/web` uses React for controls and information surfaces and Pixi
for the table and card visuals. Both layers render the same normalized `GameView` and
share visual tokens. The client may hold ephemeral UI state—selection, an open inspector,
or a reconnect token—but never authoritative game state or computed legality.

The terminal client in `crates/rune-cli` proves that the protocol is independent of the
web UI. It can prompt a human from the issued action list or let a deterministic agent
choose from the same list.

## Card model

Each card has one functional definition under
`crates/rune-engine/data/catalog/<functional_id>.json`. A stable `FunctionalId` names that
definition across builds. The build script interns compact `CardId` handles for engine use;
those numeric handles are not persisted or authored.

Printing records under `data/sets/` refer to functional definitions and contain only set
bibliography. Reprinting a card does not duplicate or change its behavior.

The schema rejects presentation assets and prose. Card behavior is expressed through
structured characteristics, abilities, effects, keywords, and a declared code escape hatch
for exceptional behavior. The server generates readable rules text from the same structures,
so display text cannot silently diverge from executable behavior.

## Current scope

RUNE currently supports deterministic games of two to four players built around creature
combat, with spectators. The implemented slice includes the full turn loop, priority, casting
and mana payment, targets, the stack, per-attacker attack targets, multi-defender blocking,
combat damage with player-chosen assignment order, common combat keywords including double
strike, continuous keyword-granting effects, counters, auras, triggers, initial replacement
effects, mulligans, concessions, loss by life or decking, and mid-game elimination in
multiplayer games. On the format side, the lobby serves the card catalog and each format's
deck rules over the wire, players build and submit decks in a client deck builder with
device-local saved lists (ADR 0027), and a commander format enforces singleton construction
and color identity over the engine's command-zone, tax, and commander-damage mechanics.

The project intentionally grows by verified rule slices rather than claiming broad card or
format compatibility. Tests and the generated, CI-checked compatibility report
([`generated/compatibility.md`](generated/compatibility.md)) are the evidence for support.
See the [roadmap](roadmap.md) for remaining work.

## Future direction

Planned growth proceeds in this order:

1. Make two-player games clear, accessible, and resilient in the web client.
2. Expand rules and card compatibility with generated evidence for each supported card.
3. Add free-for-all multiplayer and spectators.
4. Add deck construction and saved deck lists against server-owned format legality.
5. Add format-specific rules, including Commander and team formats, on the multiplayer
   foundation.
6. Reuse the same engine and protocol in desktop, offline, or additional client shells.

Alongside this sequence, the client presentation is pivoting from its original
graphics-light direction to a **polished 2.5D presentation** — illustrated, tactile,
animated, with depth from perspective, layering, shadows, and motion — tracked as a
roadmap milestone ([ADR 0029](decisions/0029-2-5d-presentation-direction.md), issue
#464). The pivot changes presentation only: engine determinism, server authority,
accessibility, and the legal constraints below are unchanged.

These are directions, not promises about a particular framework, hosting topology, capacity,
or release date. Architecture changes require an ADR.

## Legal constraints

RUNE follows a deliberately conservative fan-project policy:

- no card images or other official artwork;
- no official card frames, symbols, watermarks, or Wizards of the Coast branding;
- no exact Oracle text, flavor text, or presentation assets in the repository;
- no implication of affiliation with or endorsement by Wizards of the Coast; and
- no monetization.

Cards are procedural renders of structured data, and player-facing rules text is generated
by the server. The functional schema rejects prohibited presentation fields.

The prohibitions above cover **presentation assets**. A card's *functional data* — its name and
mechanical characteristics (types, mana cost, power/toughness, and the ability IR) — may match a
real card and be sourced from a real set; the bundled catalog draws its functional definitions
from Core Set 2019 ([ADR 0026](decisions/0026-real-functional-card-data.md)). No Oracle text,
flavor text, art, frames, symbols, or branding is shipped: rules text stays server-generated and
printings stay bibliographic.

These constraints govern **what the project distributes** — the repository, the built
client, and the server. A player may additionally opt in, on their own device, to having
their browser fetch card images directly from a third-party source (currently Scryfall);
those images are cached only on that device and are never uploaded, proxied, served, or
redistributed by the project ([ADR 0024](decisions/0024-user-side-card-art.md)). By
default only the bare illustration renders inside RUNE's own procedural frame; the player
may instead choose to display the entire card image, again on their device only. Bundled
art is limited to original, project-owned illustrations.

Any further weakening of these constraints requires an explicit legal review and
architectural decision; it is not authorized by existing plans or ADRs.

The code is available under the MIT License. That license does not change the project’s
distribution policy above.

## Product exclusions

- Collection ownership and marketplace features
- Official card presentation or branding
- Client-side rules evaluation
- A *requirement* for fully modeled 3D environments or characters (the client targets a
  2.5D presentation — [ADR 0029](decisions/0029-2-5d-presentation-direction.md); the
  earlier blanket exclusion of an effects-capable presentation is superseded)
- Monetization
- Ante, subgames, and novelty mechanics in the current roadmap
