# RUNE roadmap

Milestones from the current playable-engine state toward the finished product described in
[`docs/brief.md`](brief.md). Each milestone is an **outcome checkpoint** — "you can now do
X" — with exit criteria, not a work phase. A milestone is done when its exit criteria pass,
regardless of what later-milestone work has already landed.

## Where we are

The engine plays a **complete, legal, deterministic game to a win** for a creature-combat
subset of Magic — proven by an agent-vs-agent game through the real server and wire
protocol (`crates/rune-cli/tests/agent_game.rs`) — and that game is now **playable and
followable in the browser**. Concretely shipped:

- **Engine** (`crates/rune-engine`): zones, the full turn FSM, priority, the stack,
  mana/casting, deep combat (all 8 core keywords, multi-block, lethal-damage SBAs), win
  conditions (life ≤ 0, decking, concede), seeded-deterministic replay, and an effect IR
  covering damage / destroy / life / counters / pump / counterspell, ETB and dies triggers,
  auras, and the first replacement effects (enters tapped / with counters).
- **Server/CLI** (`crates/rune-server`, `rune-cli`): the lobby (rooms, deck submission,
  ready gate, session-token reconnect), and `rune-cli` playing a full game interactively or
  with `--agent`.
- **Web client** (`clients/web`): connection screen, lobby, battlefield/hand rendering,
  targeting, mulligans, the stack panel, combat multi-select (attackers, and blockers
  per-attacker), and the game-over overlay — the complete two-player in-game loop, all
  driven purely by `valid_actions`.
- **Cards**: a small hand-authored slice (32) as functional definitions
  ([ADR 0018](decisions/0018-scalable-functional-card-definitions.md)); the server generates
  the rules text a player reads, so no Oracle prose is stored.

**Near-term focus** — finish M3's content deliverables, then start M4:

1. **Give every card a function** (#256): four catalog cards are castable shells that
   resolve doing nothing and render blank generated rules text.
2. **Make the bundled decks real** (#257): the starter decks carry zero spells, and one
   can't cast most of its own creatures (blue/red cards over an all-Forest mana base);
   the catalog is missing four of the five basic lands.
3. **The compatibility report** (#258): M3's support-claim artifact doesn't exist yet —
   and the hand-maintained coverage ledger was removed (#252) without a generated
   replacement.

Two small closeout fixes are also open against shipped milestones: a refreshed browser
loses its seat because the session token lives only in memory (#254, M1), and a player
cannot see their own life total (#255, M2).

## Milestones

### M1 — Take a seat  *(shipped; one closeout fix open)*

Two people find each other and start a real game: launch the server, connect, create a room
with a config, a second player joins, both submit decks and ready up, and the game begins
with shuffled libraries, opening hands, and mulligans. A refreshed browser reconnects to
its seat.

All of this is landed and test-proven at every layer, with one literal gap: seat
reclamation by session token works over the wire
(`crates/rune-server/tests/lobby.rs`) and across in-page socket drops, but the web client
deliberately keeps the token only in memory, so a **hard page refresh** gets a fresh
identity instead of its seat.

- [ ] #254 — persist the session token (per-tab) so a refreshed browser reconnects.

### M2 — Play to the win  *(shipped; one closeout fix open)*

A full game plays end to end and someone wins — **in the browser**. Turn-based actions
(untap, draw, cleanup), combat (declare attackers/blockers, damage, lethal SBAs), and game
over as a first-class engine outcome were already in; the web client now renders the stack
(`StackPanel`), drives the full combat flow from `valid_actions` (attacker multi-select,
per-attacker blocker assignment), and shows a game-over overlay with winner and reason,
leaving the final board readable underneath. All three are component- and
integration-tested.

One followability gap survived the milestone: `GameView` redacts opponents down to stats
that include their life, but carries no life total for the receiver — so players can see
everyone's life except their own.

- [ ] #255 — carry the receiver's own life (and library size) in `GameView` and render it.

> The browser end-to-end suite (ADR 0011) is removed for now to keep the loop fast; it
> returns once the in-game UI settles.

### M3 — A real card pool  *(engine shipped; content and reporting remain)*

Build different decks from real cards that play differently, with a card model that scales
past the slice.

**Done:** casting timing per card type, spells targeting at cast with resolution re-check
and counterspells, the effect IR (all ten opcodes wired end-to-end), dies triggers, auras,
all eight combat keywords, the replacement pipeline's first customers, and the entire
ADR 0018 infrastructure: versioned functional definitions (`schema_version`,
`FunctionalId`, `deny_unknown_fields`), the sharded per-card catalog with build-generated
manifest, shared build/loader/test validation, and server-generated rules text with
compiler-enforced exhaustiveness.

**Exit criteria:**

- [ ] Every bundled card has an observable, tested function — no castable shells — and a
      catalog guard test keeps it that way. Today four cards fail this (Quickfire Bolt,
      Hurried Study, Verdant Blessing, Copper Lodestone). → #256
- [ ] The catalog carries the full basic-land cycle, and the bundled starter decks are
      real decks: every card castable from its own mana base, instants/sorceries present,
      archetypes mechanically distinct — proven by a deterministic agent-vs-agent game
      through the real server using the bundled lists verbatim. → #257
- [ ] A deterministic, generated compatibility report names every supported card and every
      considered-but-excluded card with its blocker; CI fails if it goes stale. Support is
      claimed only for the verified slice — never a claim that a full set works. → #258

No exact Oracle text, flavor text, official image, frame, or branding is bundled anywhere;
the schema rejects them structurally, and the no-card-images / no-official-frames rule
(`AGENTS.md`, `docs/brief.md` Legal Considerations) holds until an explicit future decision.

### M4 — Readable games  *(decomposed; not started)*

A newcomer can follow and finish a game without asking what happened. The wire contract
already anticipates much of this — `CardView.rules_text`/`keywords`, full public
`graveyards`/`exile` piles, `phase` "for overview/focus rendering", and `action_deadline`
are all carried today — but almost nothing consumes it: there is no log, no inspect UI, no
zone browsers, the server never sets a deadline, and keyboard support is a single Escape
binding. Specified throughout [`docs/design/ui-requirements.md`](design/ui-requirements.md).

**Exit criteria:**

- [ ] A structured, redacted game log rides `GameView` (engine events → protocol → server
      projection, with an ADR for the shape). → #259
- [ ] The client renders the log: collapsible, entity references click-to-highlight,
      reconstructable from a single view. → #260
- [ ] Any card in any zone can be inspected — name, cost, type line, generated rules text,
      keywords, dynamic state — with keyboard access. → #261
- [ ] Graveyard and exile browsers exist for every player, integrated with inspect. → #262
- [ ] Decision timers work when a room enables them: the server emits `action_deadline`,
      enforces a default action on expiry, and the client shows a live countdown (default
      remains off). → #263
- [ ] Basic priority automation exists behind an accepted ADR: auto-pass with per-phase
      stops, never skipping a decision the rules entitle a player to, deterministic under
      replay. → #264
- [ ] Illegality and fizzle feedback: a fizzled spell explains itself in the log, and a
      rejected action produces a non-blaming toast instead of silence. → #265
- [ ] Keyboard parity: a full turn — cast, target, combat multi-select, mulligan — is
      playable without a pointer, with visible focus and a shortcut reference. → #266
- [ ] Overview/focus modes and a persistent turn/phase/active-player indicator, derived
      purely from the current view + prompt. → #267

Suggested order: #259 → #260 unlock #265 and give every other feature a place to explain
itself; #261/#262 and #266/#267 are independent client tracks; #263 and #264 each start
with a design note.

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
