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
over, card inspection, public-zone browsers, keyboard controls, and timer display, and has
shipped the tabletop overhaul's shell: the chrome visual system and tokens (#293), the
full-bleed adaptive table shell with its pure layout function (#295), player HUDs (#296),
the compact turn/phase indicator (#297), anchored decision staging (#298), the collapsible
stack/activity rail (#299), connection and lobby identity screens (#300), protocol-carried
display names (#294), and the capability-aware spatial focus model (#301).

The presentation gap has moved from the shell to the surfaces on it. Inside each player's
lane the battlefield is still one undifferentiated row of same-size cards: no type-grouped
rows or land chips, no stacking of identical permanents, and tapped cards can collide with
their neighbors. Zone piles read as header chips rather than findable objects, every card
carries a permanently visible inspect handle, card faces stop at cost and P/T (no keyword
or ability indicators), and the identity layer — display face, glyph language — is not yet
built. The targets for all of these are specified in
[`design/ui-design-notes.md`](design/ui-design-notes.md).

## Immediate priorities

With the shell landed (#293–#301), priorities move inside the battlefield and toward
comprehension:

1. Board legibility: implement the battlefield-band interior specified in
   [`design/ui-design-notes.md`](design/ui-design-notes.md) — type-grouped rows with land
   chips, ×N stacking of identical permanents, tier-dependent tapped treatment that
   reserves the rotated footprint, and zone piles as findable spatial objects.
2. Card-face information budget: keyword glyphs and the activated-ability marker at
   battlefield scale, so reading the board doesn't require serial inspection.
3. Inspect without clutter: retire the permanent per-card inspect handles in favor of
   selection-surfaced preview, hover-dwell peek, and long-press.
4. Comprehension: structured game events in `GameView`
   ([#259](https://github.com/ninthworld/rune/issues/259)), the client game-log panel
   ([#260](https://github.com/ninthworld/rune/issues/260)), server-owned priority
   automation and stops ([#264](https://github.com/ninthworld/rune/issues/264)), and
   rejection/fizzle feedback ([#265](https://github.com/ninthworld/rune/issues/265)).
5. Identity: the bundled OFL display face behind `--rune-font-display` and the procedural
   rune glyph language for zones, phases, keywords, and tap state.

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
- keyboard access for the core play flow;
- turn, phase, active-player, and priority presentation;
- visible action affordances and table geography (#277–#278); and
- the client UI overhaul shell — visual system and tokens, full-bleed tabletop shell,
  player HUDs, decision staging, stack/activity rail, identity screens, display names, and
  the spatial focus model (#293–#301, #294); and
- the client game-log panel — a readable, scrollable history composed client-side from the
  structured `GameView` log, with clickable entity/player references and collapsible step
  runs ([#260](https://github.com/ninthworld/rune/issues/260)).

Remaining:

- battlefield-band legibility: type-grouped rows, land chips, ×N stacking of
  identical permanents, tier-dependent tapped treatment without overlap, and findable
  zone piles;
- the card-face information budget: keyword glyphs and activated-ability markers at
  battlefield scale;
- the inspect affordance redesign: no permanently visible per-card handles;
- the identity layer: a bundled OFL display face and the procedural rune glyph language;
- structured, redacted game events in `GameView`
  ([#259](https://github.com/ninthworld/rune/issues/259));
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

Original, licensed card artwork may arrive alongside the larger catalog. The card frame is
designed around an art window from M4 onward (see
[`design/ui-design-notes.md`](design/ui-design-notes.md)) so art drops into the reserved
region without a frame redesign; official imagery remains excluded permanently.

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
