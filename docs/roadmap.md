# RUNE roadmap

RUNE is progressing from a deterministic multiplayer rules implementation toward a product
where players build their own decks and play format-specific games. Milestones describe
user-visible outcomes; issue state alone does not prove an outcome is complete.

## Current state

The engine plays deterministic games of two to four players through the real server protocol
to a single winner. It includes the turn and priority loops, mana and casting, targets and
the stack, multiplayer combat — per-attacker attack targets (#341), multi-defender blocker
declaration in APNAP order (#344), and the attacking player's combat-damage assignment order
among multiple blockers (CR 510.1, #346) — combat damage, common combat keywords including
double strike (CR 702.4, #373), continuous keyword-granting effects through the layer system
(#374), counters, auras, triggers, initial replacement effects, mulligans, the elimination
lifecycle for players who lose while others remain (CR 800.4a, #342), terminal results, and a
structured game log recorded in `GameState` (ADR 0021). The commander mechanics are in: a
designated commander starts in its owner's command zone, casts from there for {2} more per
prior cast, offers the return-to-command-zone choice when it would hit the graveyard (#370),
and 21 combat damage from one commander loses the game through the normal elimination
lifecycle (CR 903.10a, #371). The return-from-exile half of #370 is unreachable until the
engine gains a battlefield→exile seam (#397).

The server provides explicit rooms, validated deck submission, a ready gate, per-tab
reconnect, optional decision timers, server-owned priority automation (ADR 0020), free-for-all
formats that seat 3–4 players on the engine's multiplayer rules (#349), and spectators: an
observer joins an in-progress room and receives a structurally redacted `SpectatorView`
(ADR 0022, #343/#351). The multiplayer view contract carries per-attacker attack targets,
eliminated state, and explicit seat order (#345). A lobby-phase client can fetch the full
card catalog with server-generated rules text and each format's advertised deck rules (#367),
and a registered `commander` format enforces 100-card singleton construction, a designated
legendary-creature commander, color-identity containment computed from structured card data,
40 starting life, and 2–4 seats (#372) — though rejected decks are re-sent an unchanged view
with no reason attached (#395), and the advertised format metadata does not yet name the
commander-specific rules (#394). The catalog contains 37 functional definitions — real Core
Set 2019 cards (ADR 0026) including the basic-land cycle; the deterministic, CI-checked
compatibility report ([`generated/compatibility.md`](generated/compatibility.md), #258) names
every supported card and excluded mechanic, with a freshness gate that fails `make check` on
drift.

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
sheets, a hand fan, and a fixed action bar. Drag-to-play (the pointer enhancement)
is in: a playable hand card drags to a gold battlefield inset (untargeted) or onto an
orange-ringed server candidate (targeted), firing the same offered action the dock
routes; the dock now gives a selection's routed actions the primary weight (pass
demotes while a card is selected), and each ability activation is labeled with its
own generated rules sentence. The front-door screens have landed in the blueprint's
language (`design/ui-design-notes.md` §Front door): a Play-first landing with the
server address as an advanced affordance, and the lobby flow — directory, room,
accented seat roster, starter-deck tiles — in the table's visual system. On the deck
track, a seated player can open the deck builder, browse the wire-carried catalog with
rules text and the format's advertised constraints, assemble an arbitrary list, and
submit it through the unchanged `submit_deck` gate (#368); built decks save device-local
under a name with portable JSON export/import (ADR 0027, #369). The commander table
chrome renders the command zone as a pile, the growing recast tax, and per-commander
damage tallies for every seat (#372). Still open from the blueprint: the bespoke
zone-to-zone travel choreography (the generic view-diff tween of #334 ships; cards do
not yet travel between fixed zone homes) and the 4-player phone summary-tile
composition (#400).

A deterministic, seeded 4-player free-for-all full game runs against the real server in the
normal test gate, with a mid-game elimination and a single winner (#350); no equivalent
full-game test exists yet for the commander format (#398). The primary commander path has
one missing link: the deck builder cannot designate a commander (#396), so commander games
currently require the bundled sample deck, and a rejected deck gives the player no reason
to correct against (#395). There are no server-side saved decks and no formats beyond the
starter duel, the permissive defaults, the free-for-all, and commander.

On the presentation side, a direction decision has been made: the client pivots from the
graphics-light tabletop toward a **polished 2.5D presentation** — illustrated, tactile,
animated — anchored on the approved baseline image
([`ui-concepts/rune-2.5d-interface-baseline.jpg`](ui-concepts/rune-2.5d-interface-baseline.jpg)),
recorded in [ADR 0029](decisions/0029-2-5d-presentation-direction.md) and tracked by the
master issue [#464](https://github.com/ninthworld/rune/issues/464) (milestone M7 below).
The playtest findings of [#450](https://github.com/ninthworld/rune/issues/450) feed the
same effort.

## Immediate priorities

Two tracks run in parallel: the M6 commander batch below, and M7's Phase 0 — the
direction-and-feasibility work of the 2.5D presentation pivot
([#464](https://github.com/ninthworld/rune/issues/464)):

- ~~Supersede the graphics-light direction in the docs~~
  ([#466](https://github.com/ninthworld/rune/issues/466)) — **landed**
  ([ADR 0029](decisions/0029-2-5d-presentation-direction.md)).
- ~~The rendering and animation architecture spike~~
  ([#467](https://github.com/ninthworld/rune/issues/467)) — **concluded**:
  [ADR 0030](decisions/0030-2-5d-presentation-architecture.md) selects the DOM-scene /
  WebGL-effects architecture on the spike's evidence
  ([`design/spike-2-5d-findings.md`](design/spike-2-5d-findings.md)).
- ~~Performance, device, animation, and accessibility budgets~~
  ([#468](https://github.com/ninthworld/rune/issues/468)) — **documented** in
  [`design/presentation-budgets.md`](design/presentation-budgets.md), grounded in the
  spike's measurements plus CPU-throttled runs; real-hardware re-validation is owed
  before the Phase 2 exit.
- The 2.5D visual system and motion grammar
  ([#469](https://github.com/ninthworld/rune/issues/469)).
- Layout concepts for two- through six-player, mobile, and stress-case boards
  ([#470](https://github.com/ninthworld/rune/issues/470)).
- Asset and effects pipeline groundwork
  ([#471](https://github.com/ninthworld/rune/issues/471)) — licensing, effect
  taxonomy, and delivery research ahead of any production assets.

M6's first batch — the deck track and the commander foundation — has landed. This batch
completes the commander user path end to end and backfills the evidence the first batch
deferred. Ordered by dependency and product impact:

1. Advertise the commander deck rules in format metadata
   ([#394](https://github.com/ninthworld/rune/issues/394)) — independent; removes the
   client's hardcoded format-name check and unlocks the builder work.
2. A rejected deck tells the player why
   ([#395](https://github.com/ninthworld/rune/issues/395)) — independent; makes the
   correct-and-resubmit loop usable, especially for commander's five rejection classes.
3. Commander designation in the deck builder and saved decks
   ([#396](https://github.com/ninthworld/rune/issues/396), blocked by #394) — the last
   missing link in build → save → play commander.
4. A deterministic commander full game through the real server
   ([#398](https://github.com/ninthworld/rune/issues/398)) — independent; the
   format-level proof #350 gave free-for-all.
5. The battlefield→exile seam so a commander can actually return from exile
   ([#397](https://github.com/ninthworld/rune/issues/397)) — independent; closes the
   unreachable half of #370.
6. Double strike proven against the multi-blocker assignment order
   ([#399](https://github.com/ninthworld/rune/issues/399)) — independent, small; the one
   untested #373 acceptance surface.
7. Catalog growth: a curated M19 slice on shipped mechanics
   ([#401](https://github.com/ninthworld/rune/issues/401)) — independent; gives the
   builder and commander a real pool, and may supply #397's exile card.
8. The 4-player phone summary-tile composition
   ([#400](https://github.com/ninthworld/rune/issues/400)) — independent; the
   blueprint's remaining compact-layout item, and the home for commander chrome on
   phones.
9. The real-browser smoke path ([#279](https://github.com/ninthworld/rune/issues/279)) —
   still queued and unblocked; one canary spec, with the full E2E suite beyond it
   deferred (ADR 0011).

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

Active. The first batch shipped deck construction and the commander foundation: the card
catalog and per-format deck rules over the wire (#367), the deck builder (#368), the
deck-persistence decision (ADR 0027, #366) and device-local saved deck lists with portable
export (#369), the command zone with commander casting and tax (#370), commander damage
(CR 903.10a, #371), the commander format — singleton, color identity, 40 life, 2–4 seats,
command-zone and damage presentation (#372) — and two catalog mechanics that each removed a
compatibility-report exclusion: double strike (#373) and continuous keyword-granting
effects (#374).

The second batch (see Immediate priorities) completes the commander user path — advertised
commander rules (#394), rejection reasons (#395), builder designation (#396) — and
backfills deferred evidence and mechanics: the commander full-game test (#398), the exile
seam (#397), the double-strike order test (#399), catalog growth (#401), and the phone
summary tiles (#400).

Later M6 capabilities stay at outcome level until this batch lands: team seating and
shared-team state for formats such as Two-Headed Giant, larger player layouts, more prompt
types, expanded automation, and a substantially larger verified card catalog.

The client's card-art pipeline shipped with ADR 0024: the frame's art window renders a
player-selected source — procedural (default), bundled project-owned art, or an opt-in,
device-local Scryfall download (see
[`design/ui-design-notes.md`](design/ui-design-notes.md) and
[ADR 0024](decisions/0024-user-side-card-art.md)). Two follow-ups ride the M6+ catalog
work:

- **The real-card catalog migration** — *done* ([ADR 0026](decisions/0026-real-functional-card-data.md)):
  the bundled catalog ships real Core Set 2019 functional definitions (names + matching
  functional data, no Oracle text/art/branding), so external art resolves by the card's own
  name and the client-side art mapping (`artMap.json`) is empty by default. The catalog is
  delivered over the wire (#367); growing the pool is owned by #401.
- **The bundled RUNE art set** — original, project-owned illustrations under
  `clients/web/public/card-art/` filling the bundled source's manifest.
- **Server-computed cost payment (auto-tap)** — the engine proposing a payment
  plan so casting taps the lands in one action. ADR 0025's direct activation
  (one-click tap-for-mana, entity-entry combat declarations) removes most of the
  urgency; the engine feature remains the eventual completion of the flow.

Official imagery remains excluded from the project's own distribution permanently.

### M7 — A living table

**Outcome:** RUNE looks and feels like a game — a polished 2.5D presentation where cards
and actions are tactile and consequential, four-player Commander is the primary staged
experience, and rules clarity comes from motion, staging, and spatial relationships
rather than a debug panel.

The master issue is [#464](https://github.com/ninthworld/rune/issues/464); the direction
decision is [ADR 0029](decisions/0029-2-5d-presentation-direction.md), anchored on the
approved baseline
([`ui-concepts/rune-2.5d-interface-baseline.jpg`](ui-concepts/rune-2.5d-interface-baseline.jpg)).
The engine, protocol, and server are out of scope: engine speed stays
presentation-independent, headless and AI-only games never wait for animation, and the
whole UI still reconstructs from one view.

Delivery is phased (see #464 for the full phase plan):

- **Phase 0 — direction and feasibility** (the current batch, child issues
  [#466](https://github.com/ninthworld/rune/issues/466)–[#471](https://github.com/ninthworld/rune/issues/471)):
  supersede the graphics-light requirements, run the rendering/animation architecture
  spike, define budgets, design the visual system and motion grammar, produce the
  2–6-player and mobile layout concepts, and ground the asset/effects pipeline.
- **Phase 1 — visual foundation:** tokens, scene composition, cards, zones, controls,
  and the basic motion system; a fixture-driven battlefield reproducing the baseline
  composition.
- **Phase 2 — playable vertical slice:** a real match on the new presentation through
  one complete action loop, with reconnect fast-forward verified.
- **Phase 3 — multiplayer and stress cases:** four-player Commander as the primary
  experience; two- through six-player layouts and large boards validated.
- **Phase 4 — full-client migration and polish:** lobby, deckbuilding, settings, and
  postgame in the same visual system; superseded components and docs retired.

Implementation issues beyond Phase 0 are split only after the spike and designs land —
the milestone deliberately avoids committing to a rendering library or component plan
before the evidence exists. The playtest findings of
[#450](https://github.com/ninthworld/rune/issues/450) that are presentation-shaped
(turn/phase comprehension, combat staging, animation and feedback, land interaction)
resolve inside this milestone; its engine bugs stay independent fixes.

### M8 — Beyond the browser

**Outcome:** the same engine and protocol support additional shells without forking rules.

Potential targets are a desktop bundle, an offline browser engine, and polished automated
opponents. Mobile comes after the desktop and multiplayer interaction models stabilize.

## Persistent exclusions

- Collection ownership, trading, and marketplace features
- Official card images, frames, branding, or exact Oracle text in the project's own
  distribution (player-side, device-local art downloads are governed by ADR 0024)
- Monetization
- Ante, subgames, and novelty mechanics until explicitly added through an architectural
  decision
