# RUNE roadmap

RUNE is progressing from a deterministic two-player rules implementation toward a clear,
reliable multiplayer product. Milestones describe user-visible outcomes; issue state alone
does not prove an outcome is complete.

## Current state

The engine can play a deterministic two-player creature-combat game through the real server
protocol to a win. It includes the turn and priority loops, mana and casting, targets and the
stack, attackers and blockers, combat damage, common combat keywords, counters, auras,
triggers, initial replacement effects, mulligans, terminal results, and a structured game log
recorded in `GameState` (ADR 0021).

The server provides explicit rooms, validated deck submission, a ready gate, per-tab reconnect,
optional decision timers, and server-owned priority automation — auto-pass of idle priority
holds with per-phase stop preferences that survive reconnect (ADR 0020). The catalog contains
36 functional definitions and a complete basic-land cycle; the bundled starter decks are shared
by the web client and the full-game agent test. A deterministic, CI-checked compatibility
report ([`generated/compatibility.md`](generated/compatibility.md), #258) names every
supported card and excluded mechanic, with a freshness gate that fails `make check` on drift.

The web client implements the lobby and game flow, targeting, combat selection, stack, game
over, card inspection, public-zone browsers, keyboard controls, and timer display, and has
shipped the tabletop overhaul's shell: the chrome visual system and tokens (#293), the
full-bleed adaptive table shell with its pure layout function (#295), player HUDs (#296),
the compact turn/phase indicator (#297), anchored decision staging (#298), the collapsible
stack/activity rail (#299), connection and lobby identity screens (#300), protocol-carried
display names (#294), and the capability-aware spatial focus model (#301). On that shell it
ships the comprehension layer — the game-log panel with clickable references and collapsed
step runs (#260) over structured, redacted log events carried in `GameView` (#259), per-phase
stop toggles with an auto-pass indicator (#264), and non-blaming rejected-action toasts with
distinct fizzle log entries (#265) — the battlefield-legibility batch: the procedural
rune glyph language (#317), type-grouped battlefield bands with land chips, ×N stacking, and
tapped-footprint reservation (#318), zone piles as findable spatial objects (#319), the
card-face information budget — keyword glyphs and activated-ability markers at battlefield
scale (#320), inspect without per-card chrome (#321), and a bundled OFL display face behind
`--rune-font-display` (#322) — and the state-visibility batch: declared attackers, blockers,
and marked damage rendered from the view contract, with combat participants never folding
into stacks (#332); aura attachment carried on `Permanent` and clustered with its host on
the board and in inspect (#333); and the reconciler's opt-in animate-the-diff layer — row
migrations, enters, exits, and tap transitions that honor reduced motion and never gate
input (#334).

Two presentation gaps remain from that batch's neighborhood. The scene computes
blocker→attacker relationships as `TableScene.combatLinks`, but no renderer consumes them,
so who-blocks-whom is still read from badges and the log rather than seen (#339). And a
player returning to a backgrounded tab gets no unread-activity signal, although server-owned
auto-pass means the game legitimately advances while they are away (#340). One rules-slice
deviation is now owned by an issue: combat damage among multiple blockers is assigned in
battlefield order with no player choice, though CR 510.1 grants the attacker's controller
that ordering (#346).

## Immediate priorities

With the state-visibility batch (#332–#334) landed, M4 is down to two residual gaps and the
project's weight shifts to M5 — the multiplayer engine, protocol, and client work. Ordered
by dependency and impact:

1. Combat-link rendering: draw the blocker→attacker relationships the scene already
   computes, with focus isolation on crowded boards
   ([#339](https://github.com/ninthworld/rune/issues/339)) — independent, and #347 builds
   on its treatment.
2. The M5 engine roots, in parallel: per-attacker attack targets
   ([#341](https://github.com/ninthworld/rune/issues/341)) and the elimination lifecycle
   ([#342](https://github.com/ninthworld/rune/issues/342)); then multi-defender blocker
   declaration in APNAP order ([#344](https://github.com/ninthworld/rune/issues/344),
   blocked by #341).
3. The multiplayer view contract: attack targets, eliminated state, and seat order carried
   in `GameView`, with per-attacker defender requirements
   ([#345](https://github.com/ninthworld/rune/issues/345), blocked by #341/#342).
4. Client multiplayer: the declare-and-render combat flow
   ([#347](https://github.com/ninthworld/rune/issues/347), blocked by #345) and the 3–4
   player table layout ([#348](https://github.com/ninthworld/rune/issues/348), fixtures
   from #345) — parallel tracks.
5. Free-for-all formats and rooms ([#349](https://github.com/ninthworld/rune/issues/349),
   blocked by #341/#344/#342), then the deterministic 4-player full-game test through the
   real server ([#350](https://github.com/ninthworld/rune/issues/350)).
6. Spectators: the view-model decision
   ([#343](https://github.com/ninthworld/rune/issues/343), ADR — can start any time) and
   its implementation ([#351](https://github.com/ninthworld/rune/issues/351), blocked by
   #343). #350 plus #351 together satisfy the M5 exit criterion.
7. Damage assignment order for multi-blocked attackers
   ([#346](https://github.com/ninthworld/rune/issues/346)) — after #341/#344 to avoid
   reworking the same combat code twice.
8. Unread-activity surfacing after returning to the tab
   ([#340](https://github.com/ninthworld/rune/issues/340)) — small and independent; any
   time.

Real-browser coverage (a smoke path through rendered turns,
[#279](https://github.com/ninthworld/rune/issues/279), reopened because the #290 canary was
reverted in #292) stays **deferred** with the rest of the E2E suite (ADR 0011): the M5
client batch (#347, #348, #351) keeps the in-game UI in flux, and the canvas render path
remains guarded by component-level tests in the meantime.

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

The deterministic, CI-checked compatibility report
([#258](https://github.com/ninthworld/rune/issues/258)) closes the last exit criterion:
[`docs/generated/compatibility.md`](generated/compatibility.md) is generated from the
catalog + a curated exclusion list by `make compat`, and a `cargo test` freshness gate
fails `make check` if it ever drifts. See
[`docs/compatibility-report.md`](compatibility-report.md) to regenerate or add an exclusion.

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
  the spatial focus model (#293–#301, #294);
- structured, redacted game events carried in `GameView`, recorded by the engine and
  projected per viewer (#259, ADR 0021);
- the client game-log panel — a readable, scrollable history composed client-side from the
  structured `GameView` log, with clickable entity/player references and collapsible step
  runs (#260);
- server-owned priority automation — auto-pass of idle priority holds with per-phase stop
  preferences, an auto-pass indicator, and reconnect-safe settings (#264, ADR 0020);
- rejection and fizzle feedback — non-blaming toasts for rejected in-game actions and
  distinct countered-versus-fizzled log entries (#265);
- battlefield-band legibility — type-grouped rows, land chips, ×N stacking of identical
  permanents, and tier-dependent tapped treatment that reserves the rotated footprint
  (#318);
- zone piles as findable spatial objects with a face-up slot for future reveals (#319);
- the card-face information budget — keyword glyphs, the latent activated-ability marker,
  and counter badges at battlefield scale, plus a marked-damage badge that lights up once
  #332 feeds it (#320);
- the inspect affordance redesign — no permanently visible per-card handles; hover-dwell
  peek, long-press, selection-surfaced preview, and focusable inspect surfaces (#321);
- the identity layer — the procedural rune glyph language (#317) and the bundled OFL
  display face behind `--rune-font-display` (#322);
- combat-state visibility — declared attackers, blockers, marked damage, and
  blocker→attacker relationships reconstructed from any single `GameView`, with combat
  participants never folded into ×N stacks (#332);
- attachment visibility — `attached_to` carried on `Permanent` (contract change), auras
  clustered with their hosts and never stack-folded, and the relationship inspectable from
  either side (#333); and
- the view-diff animation layer — row migrations, enters, exits, and tap transitions that
  honor reduced motion and never gate input on a live prompt (#334).

Remaining:

- combat-link rendering: the blocker→attacker links the scene computes as
  `TableScene.combatLinks`, drawn with focus isolation on crowded boards
  ([#339](https://github.com/ninthworld/rune/issues/339)); and
- unread activity: a visible signal for log entries that arrived while the tab was
  backgrounded ([#340](https://github.com/ninthworld/rune/issues/340)).

### M5 — More than two

**Outcome:** 3–4 players and spectators can complete free-for-all games.

**Exit:** a 4-player free-for-all plays to a single winner with a spectator watching
([#350](https://github.com/ninthworld/rune/issues/350) +
[#351](https://github.com/ninthworld/rune/issues/351)).

The groundwork is further along than the milestone label suggests: `GameState` players,
turn and priority rotation, mulligans, `GameResult`'s last-player-standing rule, the
opponent-list view projection, the 2–8-seat lobby (ADR 0012), and the client's per-opponent
HUD and battlefield bands are already seat-count-generic. The batch closes what is genuinely
two-player-bound:

- engine: per-attacker attack targets ([#341](https://github.com/ninthworld/rune/issues/341))
  and multi-defender blocker declaration in APNAP order
  ([#344](https://github.com/ninthworld/rune/issues/344));
- engine: the elimination lifecycle — turn/priority skip and CR 800.4a object cleanup
  ([#342](https://github.com/ninthworld/rune/issues/342));
- protocol/server: attack targets, eliminated state, and seat order in the view contract
  ([#345](https://github.com/ninthworld/rune/issues/345));
- client: multiplayer combat declaration and rendering
  ([#347](https://github.com/ninthworld/rune/issues/347)) and the 3–4 player table layout
  ([#348](https://github.com/ninthworld/rune/issues/348));
- server: free-for-all formats over the existing 2–8-seat rooms
  ([#349](https://github.com/ninthworld/rune/issues/349)) and the deterministic 4-player
  full-game test ([#350](https://github.com/ninthworld/rune/issues/350));
- spectators: the view-model ADR ([#343](https://github.com/ninthworld/rune/issues/343))
  and the redacted spectator implementation
  ([#351](https://github.com/ninthworld/rune/issues/351)); and
- combat completeness while the same code is open: player-chosen damage assignment order
  ([#346](https://github.com/ninthworld/rune/issues/346)).

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
