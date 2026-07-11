# RUNE roadmap

Milestones from today's pre-alpha scaffold to the finished product described in
[`docs/brief.md`](brief.md). Each milestone is an **outcome checkpoint** — "you
can now do X" — with exit criteria, not a work phase: M1 is client/server/
protocol-heavy and M2 is engine-heavy on purpose, so agents work both tracks in
parallel from day one. A milestone is done when its exit criteria pass,
regardless of what later-milestone work has already landed.

This document supersedes [`docs/agents/backlog.md`](agents/backlog.md) (the
seed backlog, fully shipped). Granular features for M1–M2 are tracked as
GitHub issues under matching GitHub milestones; the tables below link them.

> Last reconciled against GitHub issues + `main`: 2026-07-11.

## Where we are (2026-07-11 baseline)

What exists is a rigorously architected vertical slice, not a playable game:

- **Engine** (`crates/rune-engine`): pure/immutable state machine with priority
  passing, a stack, targeting with fizzle (ADR 0009), layer 7c P/T math
  (ADR 0010), and a declarative card-effect IR (ADR 0007) — but the 12 turn
  steps are inert labels (no untap/draw/cleanup), the effect vocabulary is
  AddMana/DrawCard/Tap, the only SBA is life ≤ 0, and there is **no combat, no
  win detection, no mulligans, no shuffling, and no decks** (libraries start
  empty). Six invented cards live in embedded JSON.
- **Server/CLI** (`crates/rune-server`, `rune-cli`): solid room task, lifecycle,
  and backpressure — but **no lobby protocol at all**: a WebSocket upgrade is
  auto-seated into a hardcoded 2-seat room whose game is "live" with one player
  and empty decks. No room create/join, no config, no ready gate, no deck
  submission, no identity, and therefore no reconnection (#48 deliberately
  retired seats one-way pending an identity mechanism).
- **Web client** (`clients/web`): renders battlefield/hand/tiles/action bar and
  full targeting mode well — but the WebSocket store's `connect()` is **never
  called by production code**, so the app shows "Waiting for game state…"
  forever. No connection screen, no lobby UI, no stack panel, no combat UI, no
  log, no inspect. Vitest/RTL runs in CI, but there is **no browser/e2e
  harness** and the Pixi canvas no-ops headless, so canvas rendering is
  untested.
- **Docs**: `protocol.md` covers exactly two messages (GameView, ChooseAction);
  ADRs 0001–0010; `design/ui-requirements.md` carries the full UI capability
  superset with a v1/v1.5 scoping that M4–M6 reuse.

## Milestones

### M1 — Take a seat

**Outcome:** two people can find each other and start a real game. Launch
`rune-server`, open the web client (or `rune-cli`), enter a server address,
create a room with a configuration, share the room id, have a second player
join, both submit decks and ready up — and the game begins with shuffled
libraries, opening hands, and mulligans. A refreshed browser reconnects to its
seat. The client never shows a dead screen: every state from cold start to
seated is interactive.

**Exit criteria:**

- [ ] A Playwright e2e test drives real Chromium from connection screen →
      create room → second client joins → decks submitted → ready → first
      GameView rendered on the battlefield, and runs in CI.
- [ ] `docs/protocol.md` documents the lobby contract; `rune-protocol`
      round-trips it; the "entire API is two messages" framing is amended.
- [ ] Rooms are created explicitly with config (lobby/room plumbing supports
      2–8 seats even while the engine remains 2p); no auto-seating; no game
      runs before all seats are filled, decked, and ready.
- [ ] Disconnected players reconnect to their seat via session token (closing
      the one-way retirement left by #48).
- [ ] Engine: `GameSetup` (players, starting life, hand size), deck loading,
      seeded shuffle via `rng_seed`, opening hands, London mulligan — pure and
      tested.
- [ ] `docs/rules-coverage.md` exists listing every implemented CR rule; the
      CR-citation convention is in `docs/coding-standards.md`.
- [ ] ADRs accepted for: e2e test strategy, lobby protocol, card identity vs
      printing.

**Features (PR-sized, dependency order):**

| Feature | Area | Issue | Depends on |
|---|---|---|---|
| ADR 0011: e2e browser test strategy | client | TBD-1 | — |
| Connection screen: address entry, connect, status | client | TBD-2 | — |
| Playwright harness + connect-to-battlefield smoke test | client, ci | TBD-3 | TBD-1, TBD-2 |
| ADR 0012: lobby protocol (identity, rooms, ready, decks) | protocol | TBD-4 | — |
| ADR 0013: card identity vs printing (sets model) | engine | TBD-5 | — |
| `docs/rules-coverage.md` + CR citation convention | docs | TBD-6 | — |
| Lobby message types in `rune-protocol` + protocol.md | protocol | TBD-7 | TBD-4 |
| Engine: GameSetup, deck loading, seeded shuffle, opening hands | engine | TBD-8 | — |
| Server: explicit rooms — create with config, join by id | server | TBD-9 | TBD-7 |
| Engine: London mulligan | engine | TBD-10 | TBD-8 |
| Server: pre-game gate — deck submission + ready check | server | TBD-11 | TBD-8, TBD-9 |
| Server: reconnect to a held seat via session token | server | TBD-12 | TBD-9 |
| Client: lobby UI — create/join, deck select, ready | client | TBD-13 | TBD-7, TBD-9 |
| CLI: lobby flow (interactive + `--agent`) | cli | TBD-14 | TBD-7, TBD-9, TBD-11 |
| e2e: full lobby flow test (two clients → first GameView) | client, ci | filed after TBD-3 lands | TBD-3, TBD-11, TBD-13 |

### M2 — Play to the win

**Outcome:** a full game of Magic plays end to end and someone wins. Two humans
(or a human and `rune-cli --agent`) play a complete real game in the web
client: untap, draw, play lands and creatures, attack and block, deal and take
damage, creatures die, a player wins by damage or decking (or concedes), and
the client shows a game-over screen. The stack is visible, so the game is
followable.

**Exit criteria:**

- [ ] Turn-based actions are real: untap at untap, draw at draw, cleanup
      discard-to-7 and damage wipe (CR 502/504/514), with tests citing the
      rules.
- [ ] Combat works: declare attackers, declare blockers (multi-block), combat
      damage, lethal-damage SBA destroys creatures (CR 508–510, 704.5g).
- [ ] Game over is a first-class engine outcome (life ≤ 0, decking CR 704.5c,
      concede) surfaced through a `GameView` result field; the server stops
      the room loop; clients render a result screen.
- [ ] The web client renders the stack (spells + synthetic ability entries)
      and a combat flow driven purely by `valid_actions`.
- [ ] An e2e test plays a scripted full game (two automated clients) from
      lobby to victory screen.

**Features (PR-sized, dependency order):**

| Feature | Area | Issue | Depends on |
|---|---|---|---|
| Turn-based actions: untap, draw, cleanup | engine | TBD-15 | — |
| Combat I: declare attackers and blockers | engine | TBD-16 | TBD-15 |
| Combat II: combat damage + lethal-damage SBA | engine | TBD-17 | TBD-16 |
| Game over: decking, win detection, GameView result | engine, protocol | TBD-18 | TBD-15 |
| Client: stack panel (spells + synthetic ability render) | client | filed with M2 wave 2 | — |
| Client: game-over screen | client | filed with M2 wave 2 | TBD-18 |
| Client: combat affordances via valid_actions | client | filed with M2 wave 2 | TBD-16 |
| e2e: scripted full game to victory | client, ci | filed with M2 wave 2 | all above |

### M3 — A real card pool

**Outcome:** you can build different decks from a starter set and they play
differently. The card-identity/printing model from ADR 0013 is implemented
(oracle card vs printing, set files; a reprint in a second set requires zero
logic changes — the model is designed to be compatible with real Scryfall-style
oracle data, while card images, frames, and WotC branding stay forbidden). A
legal-safe invented starter set (~100–150 cards) ships. The effect IR grows to
cover it: damage, destroy, counters placement, pump, counterspells; instants,
sorceries, auras, artifacts; ETB/dies triggers; core keywords (flying, haste,
vigilance, trample, deathtouch, lifelink, first strike). Spells can target
(extending ADR 0009 beyond abilities); the replacement-effect pipeline gets its
first real customers; deck validation runs against a format config; prompt
types `option`, `select_from_zone`, `order` land; a rule-based CLI agent plays
legally to a win.

**Exit:** two starter-set decks play a complete, correct game;
`rules-coverage.md` covers the new CR sections; adding a reprint to a second
set file changes no logic.

### M4 — Readable games

**Outcome:** a newcomer can follow and finish a game without asking what
happened. Game log (clickable entities, collapsible spam), universal
inspect/oracle-text popover, graveyard/exile zone browsers, overview/focus
modes, real decision timers (`action_deadline` enforced), basic priority
automation (auto-pass hint, per-phase stops), fizzle/illegality feedback,
keyboard input parity.

**Exit:** the comprehension items of `design/ui-requirements.md` §9 and the
automation basics of §4 pass a scripted usability run; timers enforce.

### M5 — More than two

**Outcome:** 3–4 players, and the people watching them. Engine multiplayer
(APNAP priority order, turn order, per-attacker attack targets, elimination
with objects leaving), lobby supports FFA 3–4 end to end, client multiplayer
layouts (triangle/corners per the brief), spectator mode fed by fully redacted
GameViews.

**Exit:** a 4-player FFA plays to a single winner with a spectator watching —
the `ui-requirements.md` v1 scope.

### M6 — Formats at scale *(coarse)*

Commander night: command zone with cast tax, commander-damage matrix, 5–8
player hub-and-spoke layouts, the full automation suite (stops, auto-yield,
hold priority), prompt types `divide`/`split_piles`/`name_a_card`, controller
input, expanded card pool — the `ui-requirements.md` v1.5 scope.

### M7 — Beyond the browser *(coarse)*

Same game, more places: Tauri desktop bundle (client + server child process),
WASM in-browser offline engine, LLM agents behind the `Agent` trait as polished
opponents, mobile last — the brief's deployment modes and development-sequence
steps 7–8. Explicit non-goals stay non-goals: 2HG/teams, Archenemy/Planechase,
deck builder, monetization.

## How this drives work

- **Issues are the queue** (see [`agents/workflow.md`](agents/workflow.md)):
  roadmap features become `agent-task` issues with `area:*` labels;
  `status:ready` means unblocked, `status:blocked` names the blocker.
- M1/M2 issues live under matching **GitHub milestones**; later milestones get
  issues when they come into range.
- The three tracks — client (connection → e2e → lobby UI), server/protocol
  (ADR → messages → rooms → gate → reconnect), engine (setup/mulligan;
  turn actions → combat → game over) — touch disjoint files, so several agents
  can work simultaneously.
- When a milestone's exit criteria pass, tick them here, reconcile against
  GitHub (update the "last reconciled" date), and break the next milestone
  into issues.
