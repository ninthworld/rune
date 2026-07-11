# RUNE roadmap

Milestones from today's pre-alpha scaffold to the finished product described in
[`docs/brief.md`](brief.md). Each milestone is an **outcome checkpoint** — "you
can now do X" — with exit criteria, not a work phase: M1 is client/server/
protocol-heavy and M2 is engine-heavy on purpose, so agents work both tracks in
parallel from day one. A milestone is done when its exit criteria pass,
regardless of what later-milestone work has already landed.

This document supersedes [`docs/agents/backlog.md`](agents/backlog.md) (the
seed backlog, fully shipped). Granular features for M1–M3 are tracked as
GitHub issues (each body names its milestone); the tables below link them.
Matching GitHub Milestones can be created in the repo settings and the issues
bulk-assigned — issue bodies already carry the milestone designation either way.

> Last reconciled against GitHub issues + `main`: 2026-07-11 (second pass, after
> the M1/M2 wave #102–#119 merged; follow-up issues #140–#160 filed).

## Where we are (2026-07-11, second reconciliation)

The M1 lobby track and the M2 engine track have both shipped; what's missing is
the client half of M2 and everything M3 names:

- **Engine** (`crates/rune-engine`): turn-based actions are real — untap, draw,
  cleanup discard-to-7 and damage wipe (CR 502/504/514, #116); combat works end
  to end — declare attackers/blockers with multi-block, combat damage,
  lethal-damage SBA (CR 508–510, 704.5g, #117/#118); game over is a first-class
  result (life, decking, concede) surfaced on `GameView.result` (#119); plus
  `GameSetup`, deck loading, seeded shuffle (ADR 0014), opening hands, and the
  London mulligan (#109/#111). Still true: the effect vocabulary is
  AddMana/DrawCard/Tap, only creatures are castable (sorcery speed only), there
  are **no keywords**, the replacement pipeline is a no-op scaffold, and the
  card pool is six fixture cards in a flat `cards.json` — ADR 0013's
  oracle/printing model is accepted but unimplemented.
- **Server/CLI** (`crates/rune-server`, `rune-cli`): the full ADR 0012 lobby is
  live — explicit rooms with config (2–8 seat plumbing), join by id, deck
  submission + ready gate, and session-token reconnect closing #48's one-way
  retirement (#110/#112/#113); rooms stop and are reclaimed on game over. The
  CLI speaks the lobby interactively and via `--agent` (#115), but the built-in
  agent only passes priority — it cannot fill requirements or win a game. Known
  gap: the engine's multi-select candidate sets (mulligan bottoming, combat
  declarations) are projected into the view **empty** (#140), so only the
  empty/first-choice forms round-trip today.
- **Web client** (`clients/web`): connection screen (#103), lobby UI
  (create/join/deck/ready, #114), battlefield/hand/tiles rendering, and
  targeting mode all work — a game genuinely runs. But `GameView.stack` is
  never rendered, there is no combat or mulligan-bottoming UX (blocked on
  #140), and the client neither mirrors `GameView.result` nor shows a game-over
  screen — the table just sits there after the final frame.
- **e2e** (ADR 0011): Playwright drives real Chromium in CI against the
  production bundle, asserting on the pure `TableScene` hook — but only the
  **mock-WS tier** exists (single client, canned frames). The ADR's
  real-`rune-server` smoke tier, any two-client test, and the scripted
  full-game run are still to be built.
- **Docs**: `protocol.md` covers the lobby and game phases (the "two messages"
  framing is amended); ADRs 0001–0014 accepted; `rules-coverage.md` is live and
  enforced by the definition of done.

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
      *Partial: the harness, CI job, and single-client mock-server specs
      shipped (#104); the real-server two-client flow is #144.*
- [x] `docs/protocol.md` documents the lobby contract; `rune-protocol`
      round-trips it; the "entire API is two messages" framing is amended.
      (#108)
- [x] Rooms are created explicitly with config (lobby/room plumbing supports
      2–8 seats even while the engine remains 2p); no auto-seating; no game
      runs before all seats are filled, decked, and ready. (#110, #112)
- [x] Disconnected players reconnect to their seat via session token (closing
      the one-way retirement left by #48). (#113)
- [x] Engine: `GameSetup` (players, starting life, hand size), deck loading,
      seeded shuffle via `rng_seed`, opening hands, London mulligan — pure and
      tested. (#109, #111; ADR 0014)
- [x] `docs/rules-coverage.md` exists listing every implemented CR rule; the
      CR-citation convention is in `docs/coding-standards.md`. (#107)
- [x] ADRs accepted for: e2e test strategy, lobby protocol, card identity vs
      printing. (ADR 0011/#102, ADR 0012/#105, ADR 0013/#106)

**Features (PR-sized, dependency order):** all shipped (#102–#115) except the
final row, now filed.

| Feature | Area | Issue | Depends on |
|---|---|---|---|
| ADR 0011: e2e browser test strategy | client | #102 ✅ | — |
| Connection screen: address entry, connect, status | client | #103 ✅ | — |
| Playwright harness + connect-to-battlefield smoke test | client, ci | #104 ✅ | #102, #103 |
| ADR 0012: lobby protocol (identity, rooms, ready, decks) | protocol | #105 ✅ | — |
| ADR 0013: card identity vs printing (sets model) | engine | #106 ✅ | — |
| `docs/rules-coverage.md` + CR citation convention | docs | #107 ✅ | — |
| Lobby message types in `rune-protocol` + protocol.md | protocol | #108 ✅ | #105 |
| Engine: GameSetup, deck loading, seeded shuffle, opening hands | engine | #109 ✅ | — |
| Server: explicit rooms — create with config, join by id | server | #110 ✅ | #108 |
| Engine: London mulligan | engine | #111 ✅ | #109 |
| Server: pre-game gate — deck submission + ready check | server | #112 ✅ | #109, #110 |
| Server: reconnect to a held seat via session token | server | #113 ✅ | #110 |
| Client: lobby UI — create/join, deck select, ready | client | #114 ✅ | #108, #110 |
| CLI: lobby flow (interactive + `--agent`) | cli | #115 ✅ | #108, #110, #112 |
| e2e: real-server smoke tier — two clients → first GameView | ci | #144 | #104, #112, #114 |

### M2 — Play to the win

**Outcome:** a full game of Magic plays end to end and someone wins. Two humans
(or a human and `rune-cli --agent`) play a complete real game in the web
client: untap, draw, play lands and creatures, attack and block, deal and take
damage, creatures die, a player wins by damage or decking (or concedes), and
the client shows a game-over screen. The stack is visible, so the game is
followable.

**Exit criteria:**

- [x] Turn-based actions are real: untap at untap, draw at draw, cleanup
      discard-to-7 and damage wipe (CR 502/504/514), with tests citing the
      rules. (#116)
- [x] Combat works: declare attackers, declare blockers (multi-block), combat
      damage, lethal-damage SBA destroys creatures (CR 508–510, 704.5g).
      (#117, #118 — engine-side; driving it over the wire needs #140)
- [ ] Game over is a first-class engine outcome (life ≤ 0, decking CR 704.5c,
      concede) surfaced through a `GameView` result field; the server stops
      the room loop; clients render a result screen.
      *Partial: engine/protocol/server shipped (#119); the client result
      screen is #141.*
- [ ] The web client renders the stack (spells + synthetic ability entries)
      and a combat flow driven purely by `valid_actions`. (#142, #143; the
      server-side requirements projection they consume is #140)
- [ ] An e2e test plays a scripted full game (two automated clients) from
      lobby to victory screen. (#145)

**Features (PR-sized, dependency order):** engine wave shipped (#116–#119);
the client/e2e wave is filed and open.

| Feature | Area | Issue | Depends on |
|---|---|---|---|
| Turn-based actions: untap, draw, cleanup | engine | #116 ✅ | — |
| Combat I: declare attackers and blockers | engine | #117 ✅ | #116 |
| Combat II: combat damage + lethal-damage SBA | engine | #118 ✅ | #117 |
| Game over: decking, win detection, GameView result | engine, protocol | #119 ✅ | #116 |
| Server: project multi-select `requirements` into the view | server | #140 | — |
| Client: game-over screen (mirror `result`, render it) | client | #141 | — |
| Client: stack panel (spells + synthetic ability render) | client | #142 | — |
| Client: multi-select UX — combat declarations + bottoming | client | #143 | #140 |
| e2e: scripted full game to victory | ci | #145 | #141, #142, #143, #144 |

### M3 — A real card pool

**Outcome:** you can build different decks from a starter set and they play
differently. The card-identity/printing model from ADR 0013 is implemented
(oracle card vs printing, set files; a reprint in a second set requires zero
logic changes — the model is designed to be compatible with real Scryfall-style
oracle data, while card images, frames, and WotC branding stay forbidden). A
legal-safe invented starter set (~100–150 cards) ships. The effect IR grows to
cover it: damage, destroy, counters placement, pump, counterspells; instants,
sorceries, auras, artifacts; ETB/dies triggers; core keywords. Spells can
target (extending ADR 0009 beyond abilities); the replacement-effect pipeline
gets its first real customers; deck validation runs against a format config;
prompt types `option`, `select_from_zone`, `order` land; a rule-based CLI
agent plays legally to a win.

**Exit criteria:**

- [ ] ADR 0013 implemented: `data/oracle.json` + `data/sets/<SET>.json` with a
      static embed manifest and a printing lookup; the six fixtures migrated;
      a CI test proves adding a reprint to a second set file changes zero
      rules logic. (#146; exercised on real content by #160)
- [ ] Every card type casts with correct timing: instants at instant speed,
      sorceries/artifacts/enchantments/creatures at sorcery speed
      (CR 117.1a, 304.1, 307.1). (#147)
- [ ] Spells target at cast (CR 601.2c) with the existing resolution re-check
      and fizzle (CR 608.2b), including stack-object targets — a counterspell
      (CR 701.5) works end to end. (#148)
- [ ] The effect IR covers the starter set: `DealDamage`, `Destroy`,
      `GainLife`/`LoseLife`, `PutCounters` (#149); until-end-of-turn pump
      expiring at cleanup per CR 514.2 (#150); `CounterSpell` (#148).
- [ ] Triggers grow beyond ETB: dies triggers fire from every death path
      through one seam (CR 603.6c, 700.4). (#151)
- [ ] Auras work: enchant targeting, attachment, static effect while attached,
      and the aura SBAs (CR 303.4, 704.5m/n). (#152)
- [ ] Core keywords are enforced where they bite: flying, reach, vigilance,
      haste at declaration (#153); first strike, trample, deathtouch, lifelink
      at damage (#154) — all CR 702-cited in `rules-coverage.md`.
- [ ] The replacement-effect pipeline has real customers: enters-tapped and
      enters-with-counters (CR 614.1c, 614.12) — no longer a no-op scaffold.
      (#155)
- [ ] Prompt types `option`, `select_from_zone`, `order` exist wire-to-UI:
      protocol types + server projection (#156), web-client UX (#157), and
      CLI/agent answers (#159).
- [ ] Deck validation runs in the pre-game gate against the server's format
      registry (ADR 0013 §4): minimum size and copy limits with the
      basic-land exemption, structured rejection reasons. (#158)
- [ ] `rune-cli --agent` plays a legal, seed-deterministic full game to a win
      with a rule-based policy — never submitting an id the view didn't
      offer. (#159)
- [ ] The starter set ships: ~100–150 invented oracle cards across two set
      files with at least one reprint and ≥4 preconstructed decks; two
      different starter decks play a complete, correct game. (#160)
- [ ] `docs/rules-coverage.md` covers every CR section named above.

**Features (PR-sized, dependency order):**

| Feature | Area | Issue | Depends on |
|---|---|---|---|
| Oracle/printing split, set files, reprint invariant (ADR 0013) | engine | #146 | — |
| Cast every card type with real timing | engine | #147 | — |
| Spell targets at cast + first counterspell | engine | #148 | #147 |
| Effect IR wave: damage, destroy, life, counters | engine | #149 | #148 |
| Until-end-of-turn pump + cleanup expiry | engine | #150 | #148 |
| Dies / leaves-battlefield triggers | engine | #151 | #149 |
| Auras: enchant, attachment, aura SBAs | engine | #152 | #147, #148 |
| Combat keywords I: flying, reach, vigilance, haste | engine | #153 | — |
| Combat keywords II: first strike, trample, deathtouch, lifelink | engine | #154 | #153 |
| Replacement pipeline: enters tapped / with counters | engine | #155 | — |
| Prompt types `option` / `select_from_zone` / `order` on the wire | protocol | #156 | — |
| Client prompt UX for the new prompt types | client | #157 | #156, #143 |
| Format registry + deck validation in the pre-game gate | server | #158 | — |
| Rule-based CLI agent plays to a win | cli | #159 | #156 |
| Invented starter set + preconstructed decks | engine | #160 | #146–#155 |

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
- The open queue right now: **#140–#145** finish M1/M2 (server projection,
  client game-over/stack/multi-select, the two e2e tiers), and **#146–#160**
  are the full M3 breakdown. M4+ milestones get issues when they come into
  range.
- The tracks stay parallel and mostly disjoint: client (#141 → #142/#143 →
  #157), server/protocol (#140 → #156 → #158), engine (#146–#155 in the
  table's dependency order), with content (#160) and the agent (#159) closing
  the milestone.
- When a milestone's exit criteria pass, tick them here, reconcile against
  GitHub (update the "last reconciled" date), and break the next milestone
  into issues.
