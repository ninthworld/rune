# RUNE roadmap

Milestones from the current playable-engine state toward the finished product described in
[`docs/brief.md`](brief.md). Each milestone is an **outcome checkpoint** — "you can now do
X" — with exit criteria, not a work phase. A milestone is done when its exit criteria pass,
regardless of what later-milestone work has already landed.

## Where we are

The engine plays a **complete, legal, deterministic game to a win** for a creature-combat
subset of Magic — proven by an agent-vs-agent game through the real server and wire
protocol (`crates/rune-cli/tests/agent_game.rs`). Concretely shipped:

- **Engine** (`crates/rune-engine`): zones, the full turn FSM, priority, the stack,
  mana/casting, deep combat (all 8 core keywords, multi-block, lethal-damage SBAs), win
  conditions (life ≤ 0, decking, concede), seeded-deterministic replay, and an effect IR
  covering damage / destroy / life / counters / pump / counterspell, ETB and dies triggers,
  auras, and the first replacement effects (enters tapped / with counters).
- **Server/CLI** (`crates/rune-server`, `rune-cli`): the lobby (rooms, deck submission,
  ready gate, session-token reconnect), and `rune-cli` playing a full game interactively or
  with `--agent`.
- **Web client** (`clients/web`): connection screen, lobby, battlefield/hand rendering,
  targeting, and the base for the in-game UI.
- **Cards**: a small hand-authored slice (~32) as functional definitions
  ([ADR 0018](decisions/0018-scalable-functional-card-definitions.md)); the server generates
  the rules text a player reads, so no Oracle prose is stored.

**The two things closest to making it *feel* playable** (near-term focus):

1. **Wire the non-functional spells.** Several instants/sorceries in
   `crates/rune-engine/data/catalog/` can be cast but resolve with no effect — the engine
   already implements their effects (damage, counter, destroy, life gain), the card JSON
   just needs its `effects` arrays. Highest leverage, lowest effort.
2. **Finish the in-game web UI** so a full game is playable in the browser: game-over
   screen, stack panel, and combat multi-select — all driven purely by `valid_actions`.

## Milestones

### M1 — Take a seat  *(shipped)*

Two people find each other and start a real game: launch the server, connect, create a room
with a config, a second player joins, both submit decks and ready up, and the game begins
with shuffled libraries, opening hands, and mulligans. A refreshed browser reconnects to
its seat.

### M2 — Play to the win  *(engine shipped; browser UI in progress)*

A full game plays end to end and someone wins. **Done:** turn-based actions (untap, draw,
cleanup), combat (declare attackers/blockers, damage, lethal SBAs), game over as a
first-class engine outcome. **Remaining (the near-term UI work above):** the web client
renders the stack and a combat flow from `valid_actions`, shows a game-over screen, and an
e2e test plays a scripted full game to the victory screen.

### M3 — A real card pool  *(engine shipped; card-data track in progress)*

Build different decks from real cards that play differently, with a card model that scales
past the slice. **Done:** casting timing per card type, spells targeting at cast with
resolution re-check and counterspells, the effect IR, dies triggers, auras, all eight combat
keywords, the replacement pipeline's first customers, functional card definitions with
server-generated rules text ([ADR 0018](decisions/0018-scalable-functional-card-definitions.md)).
**Remaining:** wire the shell spells' effects, grow the pool into real playable decks, and a
deterministic compatibility report naming every supported and excluded card.

Support is claimed only for the verified slice — never a claim that a full set works. No
exact Oracle text, flavor text, official image, frame, or branding is bundled anywhere; the
schema rejects them structurally, and the no-card-images / no-official-frames rule
(`AGENTS.md`, `docs/brief.md` Legal Considerations) holds until an explicit future decision.

### M4 — Readable games

A newcomer can follow and finish a game without asking what happened: game log, universal
inspect/oracle-text popover, graveyard/exile browsers, overview/focus modes, decision
timers, basic priority automation, fizzle/illegality feedback, keyboard parity.

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
