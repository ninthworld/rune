# RUNE roadmap

RUNE is progressing from a deterministic multiplayer rules implementation toward a product
where players build their own decks and play format-specific games. Milestones describe
user-visible outcomes; issue state alone does not prove an outcome is complete.

## Current state

The engine plays deterministic games of two to four players through the real server protocol
to a single winner. It includes the turn and priority loops, mana and casting, targets and
the stack, multiplayer combat — per-attacker attack targets (#341), multi-defender blocker
declaration in APNAP order (#344), and the attacking player's combat-damage assignment order
among multiple blockers (CR 510.1, #346) — combat damage, common combat keywords, counters,
auras, triggers, initial replacement effects, mulligans, the elimination lifecycle for
players who lose while others remain (CR 800.4a, #342), terminal results, and a structured
game log recorded in `GameState` (ADR 0021).

The server provides explicit rooms, validated deck submission, a ready gate, per-tab
reconnect, optional decision timers, server-owned priority automation (ADR 0020), free-for-all
formats that seat 3–4 players on the engine's multiplayer rules (#349), and spectators: an
observer joins an in-progress room and receives a structurally redacted `SpectatorView`
(ADR 0022, #343/#351). The multiplayer view contract carries per-attacker attack targets,
eliminated state, and explicit seat order (#345). The catalog contains 36 functional
definitions and a complete basic-land cycle; the deterministic, CI-checked compatibility
report ([`generated/compatibility.md`](generated/compatibility.md), #258) names every
supported card and excluded mechanic, with a freshness gate that fails `make check` on drift.

The web client implements the lobby and game flow on the tabletop shell (#293–#301) with the
comprehension layer (#259–#265), the battlefield-legibility batch (#317–#322), and the
state-visibility batch (#332–#334). On top of those it renders drawn blocker→attacker combat
links with focus isolation (#339), surfaces unread log activity after returning to a
backgrounded tab (#340), declares and renders multiplayer combat — whom each attacker
attacks — through the real UI (#347), lays out a seat-ordered table for 3–4 players with
every opponent area keyboard-reachable (#348), and mounts a read-only spectator table with
a lobby spectate affordance (#351). The UI catch-up batch
([`design/ui-redesign-plan.md`](design/ui-redesign-plan.md)) then closed the gap between
the shipped table and the agreed design: un-clipped phase expansion, display-named bands,
identically-actionable ×N stacking (lands finally stack), a viewport-derived scene scale
with centered rows and vertical fill, zone piles as table furniture (graveyard top card
face-up), a centered tray with a primary pass affordance, a game menu holding
confirm-stepped concede, and the identity layer's accents, life crests, and table
vignette. The design investigation that followed locked the client's target anatomy in
[`design/ui-blueprint.md`](design/ui-blueprint.md) (ADR 0023): a fixed shell with one
action home, proven by hostile-state mocks at laptop, tablet, and phone-portrait
geometries. The blueprint's core anatomy is now implemented: the client shell is the
carved fixed layout (top bar, per-player panels, stack+activity rail, bottom shell
owning identity, piles, hand, and the single action dock), per-card action popups are
retired in favor of dock routing, tap is the uniform ~25° treatment at every tier,
the per-panel density ladder steps crowded panels down a card tier, and the compact
(phone-portrait) composition condenses the same anatomy to a turn pill, stack/log
sheets, a hand fan, and a fixed action bar. Still open from the blueprint: drag-to-play
(the pointer enhancement), zone-travel animations, the front-door screens, and the
4-player phone summary-tile composition.

A deterministic, seeded 4-player free-for-all full game runs against the real server in the
normal test gate, with a mid-game elimination and a single winner (#350). Deck selection is
still limited to the two bundled starter decks; there is no deck construction, no saved
decks, and no format beyond the starter duel, the permissive defaults, and the free-for-all.

## Immediate priorities

M4 and M5 are complete. The project's weight shifts to M6 — deck construction and
format-specific play. Ordered by dependency and product impact:

1. The card catalog and format deck rules over the wire
   ([#367](https://github.com/ninthworld/rune/issues/367)) — independent; the deck
   builder and commander format consume it.
2. The deck builder: construct and submit a legal deck from the browsable catalog
   ([#368](https://github.com/ninthworld/rune/issues/368), blocked by #367).
3. The saved-decks decision — durable identity and where deck lists live
   ([#366](https://github.com/ninthworld/rune/issues/366), ADR — can start any time);
   saved deck lists ([#369](https://github.com/ninthworld/rune/issues/369), blocked by
   #366 and #368) follow it.
4. The commander engine roots, in parallel with the deck track: the command zone,
   commander casting, and tax ([#370](https://github.com/ninthworld/rune/issues/370));
   then commander damage ([#371](https://github.com/ninthworld/rune/issues/371), blocked
   by #370).
5. The commander format — singleton and color-identity validation, 40 life, 2–4 seats,
   command-zone and damage presentation
   ([#372](https://github.com/ninthworld/rune/issues/372), blocked by #370/#371, after
   #367).
6. Catalog growth while the format work proceeds: double strike
   ([#373](https://github.com/ninthworld/rune/issues/373)) and continuous
   keyword-granting effects ([#374](https://github.com/ninthworld/rune/issues/374)) —
   both independent, each removes a compatibility-report exclusion.
7. The real-browser smoke path ([#279](https://github.com/ninthworld/rune/issues/279)) —
   **no longer deferred**: the M5 client batch has landed and the in-game UI is stable;
   the next client work (#368) touches lobby screens, not the table render path the
   canary guards. The full E2E suite beyond the canary stays deferred (ADR 0011).

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
discoverability and table geography (#277, #278). The real-browser smoke path (#279) is the
one open item; it is now unblocked (see Immediate priorities).

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
and complete a game without hidden interaction knowledge. **Complete.**

Shipped:

- universal card inspection;
- graveyard and exile browsers;
- decision timers with server enforcement and client countdowns;
- keyboard access for the core play flow;
- turn, phase, active-player, and priority presentation;
- visible action affordances and table geography (#277–#278);
- the client UI overhaul shell — visual system and tokens, full-bleed tabletop shell,
  player HUDs, decision staging, stack/activity rail, identity screens, display names, and
  the spatial focus model (#293–#301, #294);
- structured, redacted game events carried in `GameView`, recorded by the engine and
  projected per viewer (#259, ADR 0021);
- the client game-log panel with clickable references and collapsible step runs (#260);
- server-owned priority automation with per-phase stops and reconnect-safe settings
  (#264, ADR 0020);
- rejection and fizzle feedback (#265);
- battlefield-band legibility — type-grouped rows, land chips, ×N stacking, and
  tapped-footprint reservation (#318);
- zone piles as findable spatial objects (#319);
- the card-face information budget at battlefield scale (#320);
- the inspect affordance redesign (#321);
- the identity layer — the procedural rune glyph language (#317) and the bundled OFL
  display face (#322);
- combat-state visibility — declared attackers, blockers, and marked damage reconstructed
  from any single `GameView` (#332);
- attachment visibility — `attached_to` on `Permanent`, auras clustered with their hosts
  (#333);
- the view-diff animation layer honoring reduced motion (#334);
- drawn blocker→attacker combat links with focus isolation on crowded boards (#339); and
- unread-activity surfacing after returning to a backgrounded tab (#340).

### M5 — More than two

**Outcome:** 3–4 players and spectators can complete free-for-all games. **Complete.**

**Exit satisfied:** a deterministic 4-player free-for-all plays to a single winner with a
mid-game elimination through the real server in the standard test gate (#350), and a
spectator watches a live game through a structurally redacted `SpectatorView` (#351,
ADR 0022).

Shipped: per-attacker attack targets (#341), the elimination lifecycle (#342), multi-defender
blocker declaration in APNAP order (#344), the multiplayer view contract — attack targets,
eliminated state, seat order (#345), player-chosen combat-damage assignment order
(CR 510.1, #346), client multiplayer combat declaration and rendering (#347), the 3–4 player
table (#348), free-for-all formats over the existing 2–8-seat rooms (#349), and spectators
(#343, #351).

### M6 — Formats at scale

**Outcome:** players can build decks and play format-specific games on the multiplayer
foundation.

Active. The first batch covers deck construction and the commander foundation:

- protocol/server: the card catalog and per-format deck rules over the wire
  ([#367](https://github.com/ninthworld/rune/issues/367));
- client: the deck builder ([#368](https://github.com/ninthworld/rune/issues/368));
- decision: durable identity and deck persistence
  ([#366](https://github.com/ninthworld/rune/issues/366), ADR), then saved deck lists
  ([#369](https://github.com/ninthworld/rune/issues/369));
- engine: the command zone, commander casting, and tax
  ([#370](https://github.com/ninthworld/rune/issues/370)) and commander damage
  ([#371](https://github.com/ninthworld/rune/issues/371));
- server/protocol/client: the commander format — singleton, color identity, 40 life,
  2–4 seats ([#372](https://github.com/ninthworld/rune/issues/372)); and
- catalog: double strike ([#373](https://github.com/ninthworld/rune/issues/373)) and
  continuous keyword-granting effects
  ([#374](https://github.com/ninthworld/rune/issues/374)).

Later M6 capabilities stay at outcome level until the first batch lands: team seating and
shared-team state for formats such as Two-Headed Giant, larger player layouts, more prompt
types, expanded automation, and a substantially larger verified card catalog.

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
