# RUNE roadmap

RUNE is progressing from a deterministic two-player rules implementation toward a clear,
reliable multiplayer product. Milestones describe user-visible outcomes; issue state alone
does not prove an outcome is complete.

## Current state

The engine can play a deterministic two-player creature-combat game through the real server
protocol to a win. It includes the turn and priority loops, mana and casting, targets and the
stack, attackers and blockers, combat damage, common combat keywords, counters, auras,
triggers, initial replacement effects, mulligans, and terminal results.

The server provides explicit rooms, validated deck submission, a ready gate, per-tab reconnect,
and optional decision timers. The catalog contains 36 functional definitions and a complete
basic-land cycle; the bundled starter decks are shared by the web client and the full-game
agent test.

The web client implements the lobby and game flow, targeting, combat selection, stack, game
over, card inspection, public-zone browsers, a turn ribbon, keyboard controls, and timer
display. The battlefield canvas now survives StrictMode remounts and exposes a visible fallback
if rendering fails. Remaining affordance and table-layout gaps still prevent the experience
from being reliably understandable to a new player.

## Immediate priorities

Stabilize the existing two-player experience before expanding the rules surface:

1. Make issued hand and battlefield actions visibly discoverable
   ([#277](https://github.com/ninthworld/rune/issues/277)).
2. Give the table clear player areas and visible zone geography
   ([#278](https://github.com/ninthworld/rune/issues/278)).

Real-browser coverage (a smoke path through rendered turns,
[#279](https://github.com/ninthworld/rune/issues/279)) stays **deferred** with the rest of the
E2E suite (ADR 0011) while the in-game UI is still in flux; the canvas render path is guarded by
component-level tests in the meantime.

## Milestones

### M1 — Take a seat

**Outcome:** two players can connect, find or create a room, submit decks, ready, begin a game,
and reclaim their seats after a page refresh.

The server and clients implement identity, a public room directory, explicit rooms, deck
submission, readiness, and session-token reconnect. Players can browse and join open rooms or
enter an id directly.

### M2 — Play to the win

**Outcome:** two players can complete a legal game in the browser and understand the result.

The engine, protocol, and UI flows are implemented and covered by unit and integration tests.
Reliable canvas rendering and a visible failure state are shipped, as are action
discoverability and table geography (#277, #278). A real-browser smoke path (#279) stays
deferred with the E2E suite (ADR 0011).

### M3 — A real card pool

**Outcome:** bundled decks contain cards with distinct, tested functions, and support claims
are generated from evidence rather than prose.

Shipped:

- versioned, per-card functional definitions with stable `FunctionalId` values;
- generated catalog assembly and shared validation;
- server-generated rules text with exhaustive formatter coverage;
- functional effects for every bundled nonland card;
- all five basic lands and two legal, mechanically distinct starter decks; and
- a deterministic full-game test using the bundled deck data through the real server.

Remaining: add a deterministic, CI-checked compatibility report before claiming catalog
compatibility beyond tested behavior.

### M4 — Readable games

**Outcome:** a newcomer can follow decisions and state changes, inspect public information,
and complete a game without hidden interaction knowledge.

Shipped foundations:

- universal card inspection;
- graveyard and exile browsers;
- decision timers with server enforcement and client countdowns;
- keyboard access for the core play flow; and
- turn, phase, active-player, and priority presentation.

Remaining:

- visible action affordances and table geography (#277–#278);
- structured, redacted game events in `GameView`
  ([#259](https://github.com/ninthworld/rune/issues/259));
- a client game-log panel ([#260](https://github.com/ninthworld/rune/issues/260));
- server-owned priority automation and stops
  ([#264](https://github.com/ninthworld/rune/issues/264)); and
- action rejection and fizzle explanations
  ([#265](https://github.com/ninthworld/rune/issues/265)).

### M5 — More than two

**Outcome:** 3–4 players and spectators can complete free-for-all games.

Required work includes multiplayer turn and priority ordering, per-attacker defenders,
elimination, multiplayer room formats, redacted spectator views, and responsive player-area
layouts. The lobby’s 2–8-seat shape is only preparation; it does not imply multiplayer engine
support.

### M6 — Formats at scale

**Outcome:** players can build decks and play format-specific games on the multiplayer
foundation.

Expected capabilities include deck construction with server-validated format rules, saved
deck lists, Commander’s command zone, commander tax and damage, team seating and shared-team
state for formats such as Two-Headed Giant, larger player layouts, more prompt types, expanded
automation, and a substantially larger verified card catalog.

### M7 — Beyond the browser

**Outcome:** the same engine and protocol support additional shells without forking rules.

Potential targets are a desktop bundle, an offline browser engine, and polished automated
opponents. Mobile comes after the desktop and multiplayer interaction models stabilize.

## Persistent exclusions

- Collection ownership, trading, and marketplace features
- Official card images, frames, branding, or exact Oracle text
- Monetization
- Ante, subgames, and novelty mechanics until explicitly added through an architectural
  decision
