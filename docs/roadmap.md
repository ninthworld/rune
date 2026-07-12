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

> Last reconciled against GitHub issues + `main`: 2026-07-12 (third pass, audited
> against `main` @ `4f1514b`, after ADR 0018 replaced the invented-set content
> strategy and #223 landed its schema).
>
> **Exit-criterion status is not settled here.** Ticking M1–M3 boxes is the job of
> the structured milestone audit (#188, [ADR 0017](decisions/0017-milestone-stewardship-cycle.md)),
> which requires executable evidence per criterion — a closed issue is not evidence.
> This pass updates the narrative below and M3's content strategy; the boxes are left
> as the audit finds them.

## Where we are (2026-07-12, third reconciliation)

M1 and M2 have shipped end to end, and M3's engine, server, and client waves are in.
What M3 still owes is the **card-data track**: the catalog's file layout, the
generated rules text a player reads, and the real cards themselves.

- **Engine** (`crates/rune-engine`): the M2 rules surface is complete (turn-based
  actions, combat with multi-block and lethal-damage SBA, game over as a first-class
  result), and so is the M3 engine wave — casting timing for every card type, spells
  targeting at cast with the resolution re-check and counterspells, the effect IR
  (damage, destroy, life gain/loss, counters, until-end-of-turn pump expiring at
  cleanup), dies triggers, Auras with their state-based actions, all eight combat
  keywords, and the replacement pipeline's first real customers (enters tapped, enters
  with counters). Card data is 32 cards in `data/oracle.json` with printings in
  `data/sets/{FIX,FIX2}.json`, now authored as ADR 0018 **functional definitions**: a
  stable `functional_id`, a required `schema_version`, explicit `colors`, and a closed
  schema that structurally rejects presentation assets (#223, `docs/card-schema.md`).
- **Server/CLI** (`crates/rune-server`, `rune-cli`): the ADR 0012 lobby is live
  (explicit rooms, join by id, deck submission + ready gate, session-token reconnect);
  multi-select requirements are projected into the view; prompt types `option`,
  `select_from_zone`, and `order` round-trip; deck validation runs in the pre-game gate
  against the format registry; and `rune-cli --agent` plays a legal, seed-deterministic
  game to a win. `CardView` carries **server-generated** rules text and the stable
  `functional_id` presentation identity (#194): no rules prose is stored anywhere, and
  what a player reads is composed from the same IR the engine executes.
- **Web client** (`clients/web`): connection screen, lobby UI, battlefield/hand/tiles
  rendering, targeting, the stack panel, the game-over overlay, multi-select UX
  (combat declarations, mulligan bottoming), and the prompt UX for the new prompt
  types.
- **e2e** (ADR 0011): the mock-WS tier, the real-`rune-server` smoke tier, and a
  scripted full-game run all exist as specs under `clients/web/e2e/`.
- **Docs**: ADRs 0001–0018 accepted; `protocol.md` covers the lobby and game phases;
  the authored card schema is `docs/card-schema.md`; `rules-coverage.md` is live and
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

**Outcome:** you can build different decks from **real** cards and they play
differently — and the card model scales past the slice they come from. Cards are
authored as structured **functional definitions** ([ADR 0018](decisions/0018-scalable-functional-card-definitions.md)):
one per card, under a stable `functional_id`, versioned by `schema_version`, holding
only what the engine executes. The rules text a player reads is **generated by the
server** from that definition, so no prose is stored and what is displayed cannot
drift from what the card does. ADR 0013's oracle-vs-printing split stands underneath
(a reprint is one printing record and zero rules-logic changes). The effect IR covers
the cards in scope: damage, destroy, counters placement, pump, counterspells;
instants, sorceries, auras, artifacts; ETB/dies triggers; core keywords. Spells can
target (extending ADR 0009 beyond abilities); the replacement-effect pipeline gets its
first real customers; deck validation runs against a format config; prompt types
`option`, `select_from_zone`, `order` land; a rule-based CLI agent plays legally to a
win.

What ships as content is a **human-approved compatibility slice of real cards**, every
card in it verified — not an invented set (that plan is retired: #160 is superseded by
#191, not completed), and never a claim that a full set is supported. **Support is
claimed for the slice.** A selected set counts as complete only when every card in it
is verified; short of that, the project says "these cards work," names the excluded
ones, and says why.

**Not in this milestone, and not implied by it:** exact Oracle text, flavor text, and
official images/frames/branding are **not bundled** — the schema rejects them
structurally, not by review — and the no-card-images/no-official-frames hard rule
(`AGENTS.md`, `docs/brief.md` Legal Considerations) stays in force until an explicit
future decision changes it. A future *client-local* cache that enriches a card with
exact text or art is recorded as a coarse deferred capability only (ADR 0018 §9);
nothing in this milestone builds, promises, or depends on it, and the server and
engine never fetch, store, or require such data.

**Exit criteria:**

- [ ] The card model is ADR 0013's oracle/printing split with ADR 0018's functional
      definitions on top: a stable `FunctionalId`, a versioned schema that rejects
      presentation assets, printings that reference cards by that identity (#146,
      #223), and a catalog sharded one file per card behind a generated manifest
      (#193). A CI test proves adding a reprint to a second set file changes zero
      rules logic; the authored schema is documented in
      [`docs/card-schema.md`](card-schema.md).
- [ ] The server **generates** deterministic fallback rules text from the functional
      definition, with compiler-enforced coverage of the IR, and projects it on
      `CardView` alongside the stable `functional_id` presentation identity — with no
      authored prose left anywhere in the catalog. (#194)
- [ ] Every card type casts with correct timing: instants at instant speed,
      sorceries/artifacts/enchantments/creatures at sorcery speed
      (CR 117.1a, 304.1, 307.1). (#147)
- [ ] Spells target at cast (CR 601.2c) with the existing resolution re-check
      and fizzle (CR 608.2b), including stack-object targets — a counterspell
      (CR 701.5) works end to end. (#148)
- [ ] The effect IR covers the cards in scope: `DealDamage`, `Destroy`,
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
- [ ] A human-approved **real-card compatibility slice** is playable (#195): the slice,
      its selection rationale, and its explicit non-goals are approved; every committed
      card is expressed in the versioned schema or a reviewed scripted exception; a
      deterministic compatibility report names every excluded card and the mechanic or
      engine capability blocking it; and two approved decks built only from the
      supported slice play a complete, correct game. No full-set support is claimed
      unless every card in that set passes.
- [ ] No exact Oracle text, flavor text, official image, frame, watermark, or artist
      credit is committed anywhere — enforced by the schema, not by review. (#191,
      #223)
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
| ADR 0018: functional card definitions + generated rules text | engine, protocol, server | #191 ✅ | #146 |
| Engine: `FunctionalId`, `schema_version`, tightened card schema | engine | #223 ✅ | #191 |
| Engine: shard the catalog into per-card files + generated manifest | engine | #193 | #223 |
| Protocol/server: generated `rules_text` + `functional_id` on `CardView` | protocol, server, client | #194 | #223 |
| First real-card compatibility slice (tracking) | engine, server, client | #195 | #193, #194 |
| ~~Invented starter set + preconstructed decks~~ | engine | #160 — **superseded** by #191, closed unimplemented; replaced by #223 → #193 → #194 → #195 | — |

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
- The open queue right now is the **M3 card-data chain** — #193 (shard the catalog)
  → #194 (generate the rules text and project the presentation identity) → #195
  (the approved real-card slice, a tracking issue whose children are planned only
  once #193 and #194 land). Alongside it: the milestone-stewardship tooling wave
  #224–#228 (ADR 0017) with its first audit #188 in review, and the docs/CI queue
  (#190, #197–#202). M4+ milestones get issues when they come into range.
- **Deferred, deliberately unplanned:** a client-local cache that enriches a card with
  exact Oracle text or official art (ADR 0018 §9). It is recorded here as a coarse
  capability so it is not re-litigated, not as work in any milestone — it would need
  its own decision, and the no-card-images/no-official-frames hard rule holds until
  one is made.
- When a milestone's exit criteria pass, tick them here, reconcile against
  GitHub (update the "last reconciled" date and the audited `main` SHA), and break the
  next milestone into issues. **Criterion-level status comes from the structured audit
  (#188, ADR 0017) — executable evidence per criterion, not issue closure.**
