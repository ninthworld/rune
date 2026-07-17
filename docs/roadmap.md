# RUNE roadmap

Milestones from the current playable-engine state toward the finished product described in
[`docs/brief.md`](brief.md). Each milestone is an **outcome checkpoint** — "you can now do
X" — with exit criteria, not a work phase. A milestone is done when its exit criteria pass,
regardless of what later-milestone work has already landed.

## Where we are

The engine plays a **complete, legal, deterministic game to a win** for a creature-combat
subset of Magic — proven by an agent-vs-agent game through the real server and wire
protocol (`crates/rune-cli/tests/agent_game.rs`). The card pool is real: every bundled
card has a tested function, the full basic-land cycle exists, and the starter decks are
castable, mechanically distinct lists. The web client carries the complete two-player
in-game loop (lobby, mulligans, casting, targeting, combat multi-select, stack panel,
game over) plus the M4 comprehension surfaces (card inspect, zone browsers, phase/turn
ribbon, keyboard parity, decision timers) — all driven purely by `valid_actions`.

**But a live playability audit (2026-07-17, reproduced with a real two-browser game)
found the client is not usable as a game today:**

- Under the dev server the Pixi canvas is silently lost (React StrictMode remount
  detaches it), so **no cards render at all** — the table is a row of bare inspect
  circles (#276). No test anywhere renders the client in a real browser, so CI stayed
  green (#279).
- Even with a working canvas, nothing shows that a card is playable: the only visible
  button is "Pass priority", and a player who clicks it walks through their whole main
  phase without ever seeing that lands were castable (#277).
- The board is an unlabeled void — empty battlefields draw nothing, and library/
  graveyard/exile exist only as text counts in tiles, so there is no visible place cards
  go and no visible deck to draw from (#278).
- Games are joinable only by pasting a room id shared out-of-band; the lobby lists
  nothing (#280).

**Near-term focus — make the game visibly playable in the browser.** This gates
everything else: the UI is the make-or-break surface of the project, and it cannot be
evaluated until a person can see and play cards.

1. Fix the vanished canvas (#276), then pin it down with a real-browser smoke test that
   plays a land through the rendered UI (#279).
2. Make playable cards look playable (#277) and give the table a legible geography —
   labeled player areas and visible zone piles (#278).
3. Let players find games: a lobby room browser instead of an out-of-band id (#280).
4. Then the M4 comprehension backlog: the game log (#259 → #260), illegality/fizzle
   feedback (#265), and priority auto-pass (#264) — the log narrates what happened;
   auto-pass removes the pass-priority treadmill that currently *is* the draw step.

One bookkeeping caution: #258 (the M3 compatibility report) was closed as completed, but
no generator, make target, or generated report exists in-tree — verify or reopen before
claiming M3's reporting criterion.

## Milestones

### M1 — Take a seat  *(shipped; reopened — games must be discoverable)*

Two people find each other and start a real game: launch the server, connect, create a
room with a config, a second player joins, both submit decks and ready up, and the game
begins with shuffled libraries, opening hands, and mulligans. A refreshed browser
reconnects to its seat.

All of this is landed and test-proven at every layer — including session-token
persistence, so a hard page refresh reclaims its seat (#254, shipped). But "two people
find each other" still assumes the room id travels out-of-band: the lobby offers a bare
id field and lists nothing, which fails the milestone's own outcome statement for anyone
who wasn't handed an id.

- [x] #254 — persist the session token (per-tab) so a refreshed browser reconnects.
- [ ] #280 — lobby room browser: open rooms are listed and joinable in-client (with
      occupancy and status, in-progress games visible), no out-of-band id required.

### M2 — Play to the win  *(shipped; reopened — the dev client renders no cards)*

A full game plays end to end and someone wins — **in the browser**. Turn-based actions,
combat, the stack panel, combat multi-select, and the game-over overlay are all in and
component-/integration-tested, and the receiver sees their own life and library
(#255, shipped).

Reopened on evidence: in the dev client (StrictMode) the battlefield canvas is removed
from the DOM by its own cleanup and never returns, so hand and battlefield are invisible
— the game cannot actually be played in the environment the project is evaluated in. The
jsdom test suite cannot see this class of failure, and the browser E2E suite was removed
(#251), so it shipped green.

- [x] #255 — carry the receiver's own life (and library size) in `GameView` and render it.
- [ ] #276 — the table renders card faces in the dev client: fix the StrictMode canvas
      detach, and make any canvas failure visible instead of a silent blank board.
- [ ] #279 — a minimal real-browser smoke test (two contexts, real server) asserts the
      canvas is attached and non-blank and plays a land through the rendered UI, wired
      into `make verify`/CI.

> The full browser E2E suite (ADR 0011) stays parked; #279 restores only a one-spec
> canary, which is what would have caught #276.

### M3 — A real card pool  *(shipped, with one criterion to verify)*

Build different decks from real cards that play differently, with a card model that
scales past the slice.

**Done:** casting timing per card type, spells targeting at cast with resolution
re-check and counterspells, the effect IR (all ten opcodes wired end-to-end), dies
triggers, auras, all eight combat keywords, the replacement pipeline's first customers,
and the entire ADR 0018 infrastructure: versioned functional definitions, the sharded
per-card catalog with build-generated manifest, shared build/loader/test validation, and
server-generated rules text with compiler-enforced exhaustiveness.

**Exit criteria:**

- [x] Every bundled card has an observable, tested function — no castable shells — and a
      catalog guard test keeps it that way. → #256
- [x] The catalog carries the full basic-land cycle, and the bundled starter decks are
      real decks: every card castable from its own mana base, instants/sorceries present,
      archetypes mechanically distinct — proven by a deterministic agent-vs-agent game
      through the real server using the bundled lists verbatim. → #257
- [ ] A deterministic, generated compatibility report names every supported card and every
      considered-but-excluded card with its blocker; CI fails if it goes stale. Support is
      claimed only for the verified slice — never a claim that a full set works. → #258
      *(closed as completed on 2026-07-16, but no generator or generated report exists
      in-tree — verify what satisfied it, or reopen.)*

No exact Oracle text, flavor text, official image, frame, or branding is bundled anywhere;
the schema rejects them structurally, and the no-card-images / no-official-frames rule
(`AGENTS.md`, `docs/brief.md` Legal Considerations) holds until an explicit future decision.

### M4 — Readable games  *(in progress)*

A newcomer can follow and finish a game without asking what happened — which now
explicitly includes *knowing what they can do*: the playability audit showed the biggest
comprehension gap is not history but affordance (nothing marks a playable card, and the
board has no geography). Specified throughout
[`docs/design/ui-requirements.md`](design/ui-requirements.md).

**Shipped:** universal card inspect with keyboard access (#261), graveyard/exile zone
browsers for every player (#262), decision timers behind a room setting with live
countdown and server-side default on expiry (#263), keyboard parity for a full turn with
a shortcut reference (#266), and overview/focus modes with a persistent turn/phase/
active-player ribbon (#267).

**Exit criteria:**

- [x] Any card in any zone can be inspected — name, cost, type line, generated rules text,
      keywords, dynamic state — with keyboard access. → #261
- [x] Graveyard and exile browsers exist for every player, integrated with inspect. → #262
- [x] Decision timers work when a room enables them: the server emits `action_deadline`,
      enforces a default action on expiry, and the client shows a live countdown (default
      remains off). → #263
- [x] Keyboard parity: a full turn — cast, target, combat multi-select, mulligan — is
      playable without a pointer, with visible focus and a shortcut reference. → #266
- [x] Overview/focus modes and a persistent turn/phase/active-player indicator, derived
      purely from the current view + prompt. → #267
- [ ] Playable cards look playable: a card carrying subject-actions renders a visible,
      always-on affordance (distinct from selection/targeting, colorblind-safe) before any
      pointer interaction — no more invisible hotspots. → #277
- [ ] The table has a geography: labeled per-player battlefield areas (visible even when
      empty), a visually separated hand row, and library/graveyard/exile rendered as
      table objects with live counts — so it is obvious where cards go and where draws
      come from. → #278
- [ ] A structured, redacted game log rides `GameView` (engine events → protocol → server
      projection, with an ADR for the shape). → #259
- [ ] The client renders the log: collapsible, entity references click-to-highlight,
      reconstructable from a single view. → #260
- [ ] Basic priority automation exists behind an accepted ADR: auto-pass with per-phase
      stops, never skipping a decision the rules entitle a player to, deterministic under
      replay. → #264
- [ ] Illegality and fizzle feedback: a fizzled spell explains itself in the log, and a
      rejected action produces a non-blaming toast instead of silence. → #265

Suggested order: #277 → #278 first (they, with M2's #276/#279, are what make the game
playable at all); then #259 → #260 unlock #265 and give every feature a place to explain
itself; #264 starts with a design note and removes the pass-priority treadmill.

### M5 — More than two

3–4 players and spectators: engine multiplayer (APNAP priority, turn order, per-attacker
targets, elimination), FFA lobby, multiplayer client layouts, and spectator mode fed by
fully redacted GameViews.

### M6 — Formats at scale

Commander night: command zone with cast tax, commander-damage matrix, 5–8 player
hub-and-spoke layouts, the full automation suite, more prompt types, controller input, and
an expanded card pool.

### M7 — Beyond the browser

Same game, more places: Tauri desktop bundle (client + server child process), WASM
in-browser offline engine, LLM agents as polished opponents, mobile last. Non-goals stay
non-goals: 2HG/teams, Archenemy/Planechase, deck builder, monetization.
