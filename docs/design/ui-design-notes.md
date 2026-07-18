# UI design decisions

> **Partially superseded (July 2026).** The screen anatomy, shell layout, and
> action-placement model in this document — the floating-chrome tabletop shell,
> the scene-drawn hand, per-card action rendering, and anchored prompt overlays
> — are **superseded by [`ui-blueprint.md`](ui-blueprint.md)** (see
> [ADR 0023](../decisions/0023-fixed-shell-anatomy.md)). Where the two
> disagree, the blueprint wins. This document remains authoritative for the
> card tokens and palette, the card-face information budget, combat indicator
> shapes, the identity layer, zone-pile semantics, and legal constraints.

The design source of truth for the web client's presentation vocabulary. The full
capability list is in [`ui-requirements.md`](ui-requirements.md); the screen
anatomy and interaction model live in [`ui-blueprint.md`](ui-blueprint.md);
delivery status belongs in [`../roadmap.md`](../roadmap.md).
Change tokens in [`clients/web/src/tokens.ts`](../../clients/web/src/tokens.ts) only in
lockstep with this document.

## Design stance

The first shipped table proved the protocol: every flow works, but it reads as a
vertically stacked dashboard — uniform panels in one document column, with the
battlefield as just another row. The target presentation is a **procedural
tabletop**: the battlefield owns the viewport, chrome docks around its edges, and
`valid_actions[]` is the *emphasis* primitive, not just the legality primitive —
the only prominent, glowing, or interactive elements are the interactions the
server is currently offering. Everything else is quiet state display.

Two constraints shape every layout decision:

- **No guaranteed screen.** Size, ratio, and orientation vary; layout derives from
  available geometry, never from an enumerated device list.
- **No privileged input.** Pointer, touch, keyboard, and (eventually) controller
  focus drive the same abstract verbs: move focus / select, confirm, inspect,
  back or cancel, pass.

Personality is a stated goal, but it is carried by tokens and procedural
geometry, never by surface weight. The concept boards that informed this
direction lean on painted texture, engraved panels, and ornamental corners; RUNE
takes their *mood* — a near-black table, jewel-tone identity accents, thin-line
framing, gold reserved for "you can act" — and rejects the ornament. Heavy
texture ages badly against future card art, eats space at small card tiers, and
hard-codes a look that tokens could otherwise re-theme. The shipped danger is the
opposite failure: structure without identity, a functional table that reads as a
gray dashboard. The target is the intersection — restrained surfaces with a real
display face, a glyph language, and disciplined accent color.

## Tabletop shell

> **Superseded by [`ui-blueprint.md`](ui-blueprint.md) (ADR 0023).** The
> region model below — floating chrome overlaying the battlefield, the hand
> drawn inside the scene, the floating tray — is the shipped state, kept here
> as a record; the blueprint's fixed-shell anatomy replaces it.

Full-bleed regions replace the stacked column. Regions never reorder; they scale,
condense, or collapse.

- **Battlefield** — center, owns most of the viewport. The Pixi scene sizes to its
  region; the board never scrolls horizontally at supported geometries.
- **Opponent HUD** — a strip across the top: per-opponent identity, life
  (prominent), hand count, and statuses. Tiles reflow by player count
  (1 wide tile at 2p → compact grid at 8p).
- **Turn/phase indicator** — compact, top center: turn number, whose turn, and the
  current step, with phase-group progress. The full 12-step strip appears only on
  demand (and later becomes the stops/automation surface, issue #264). The
  always-on 12-pill ribbon is retired.
- **Local player dock** — bottom left: identity, life, floating mana, statuses.
- **Hand** — anchored along the bottom center, the largest card tier.
- **Action tray** — floats above the hand: global actions plus the contextual echo
  of the selected entity's actions (ADR 0004 unchanged — the tray stays O(1); per-
  card actions render on the card).
- **Stack & activity rail** — right edge, collapsible: the stack in resolution
  order, later the game log (issue #260). Auto-expands while the stack is
  populated; collapses to a badge on narrow geometry.
- **Prompt overlays** — a pending decision (targets, multi-select, options,
  ordering) presents as a focused overlay visually anchored to its subjects on the
  board, not as a detached banner row. Focus mode dims non-decision chrome.
- **Zone piles** — library / graveyard / exile live once, in each player's board
  region (lanes keep their labeled geography). HUD surfaces do not repeat pile
  counts they merely summarize; a count appears in exactly one home. Piles are
  card-shaped spatial objects with a count, parked in a consistent corner of every
  player's region — findable at a glance, not text chips in a header row. Zones
  are *places where cards can be shown*: a server-revealed library top card
  renders face-up on the library pile. (The protocol carries no such reveal
  today; adding one is a contract change, but the layout assumes the pile can
  host it.) Delivered by issue #319 and the UI catch-up batch: the `ZonePile`
  component (`clients/web/src/table/ZonePile.tsx`) renders each zone as a
  card-shaped pile in a **reserved pile column** on the band's right edge
  (`Band.pileRect`; card rows wrap short of it) — a stacked-edge card back for the
  library, an outlined slot when empty, the count worn as a corner badge (still its
  single home, the HUD stopped repeating it in #296). The **graveyard fills its
  `faceUp` slot today** with the pile's public top card (already in
  `GameView.graveyards` — no protocol change); the library's slot stays reserved
  for the future server reveal.

One pure function `layout(viewport, mode, playerCount)` positions every region for
both renderers. It keys on measured geometry (width, height, aspect) and input
capabilities (pointer precision, hover availability) — not user-agent sniffing or
desktop/mobile breakpoints. Portrait, landscape, and ultrawide all resolve from
the same function. Two modes — **overview** and **focus** — differ in emphasis and
density only; regions never move between modes.

Adaptation runs in both directions. Small screens condense and collapse; large
screens are *spent*, not left over — card tiers and region breathing room scale
up and the table stays centered, rather than content anchoring to one corner of a
mostly empty viewport. A 27-inch monitor should feel like a bigger table, not a
phone layout with margins.

Delivered (UI catch-up batch): `layout()` derives a **`sceneScale`** (≥ 1, clamped
and quarter-quantized) from the battlefield region, and `buildTableScene` multiplies
its card footprints and gaps by it — the reconciler applies the same factor to each
Pixi display object, so both renderers stay rect-aligned. The scene spans the full
wrap budget, **centers each card row**, and stretches to the region height by
distributing slack into the gaps *between* bands (opponents justify from the top;
the local band and hand sink together to the bottom), so open table space pools
along the battle line. The expanded phase indicator now floats as a viewport-clamped
overlay below the compact bar — the fixed-height strip can no longer clip it. A
**game menu** (top-right, restrained drawer) holds session-level actions: the
shortcut reference and — only when the server offers it — concede behind a confirm
step, so the highest-stakes action never sits beside Pass priority in the tray;
the tray centers above the hand and gives the offered pass-priority action the
primary (gold, keybind-hinted) treatment.

## Visual hierarchy

The uniform panel look (one background, one border color, one radius everywhere)
is replaced by explicit surface tiers:

1. **Table** — the deepest layer: the board surface, with subtle procedural
   texture/vignette so it reads as a table, not an app background. Delivered (UI
   catch-up batch): the shell wears a radial vignette and a faint `RuneMark` motif
   under a now-transparent battlefield canvas — tokens and geometry only, no
   texture assets.
2. **Board regions** — player lanes, hand row, zone piles: bounded areas *on* the
   table, delineated by geometry and tint, not chrome boxes.
3. **Floating chrome** — HUDs, tray, rail, indicator: elevated above the table
   (shadow/contrast), visually lighter than the board they frame.
4. **Overlays** — prompts, inspect, zone browsers, game over: topmost, with the
   layers beneath dimmed.

Typography gets a scale (display / heading / body / caption) and a distinctive
display face for identity moments — the wordmark, victory/defeat, phase names.
Body text stays a legible system stack. `--rune-font-display` is the swap point:
ADR 0019 shipped it as a geometric system stack with no bundled binary, and issue
#322 made the anticipated swap — a bundled **OFL display face** now leads the token,
with that system stack kept as the fallback. The bundled face is "RUNE Display" (a
subset of Rajdhani, SIL OFL 1.1; angular, geometric, rune-adjacent), served with the
client bundle as a ~14 KB WOFF2 (no network fetch), `font-display: swap` so identity
text is never invisible and there is no blocking layout shift. The asset, its OFL
license text, and a provenance note live in
[`clients/web/src/chrome/fonts/`](../../clients/web/src/chrome/fonts/). Effects remain restrained per the
brief (no 3D, no particle noise): elevation, tint, and motion that always honors
`prefers-reduced-motion`.

## Identity

RUNE's look is procedural geometry: the rune/monogram motif from the card
renderer extends to the pre-game screens and sparse table accents. Connection and
lobby are the product's front door and share the table's visual system — not
generic dark forms. Players read as people, not seat ids: display names ride the
protocol (a contract change, issue #294; seat-derived fallbacks like "Player 2"
until then). Never compass directions or seat numbers as headers.

Each player gets an identity color accent — on their region border, nameplate,
and HUD tile, **never on their cards**. A permanent's frame color is game
information (protection, devotion, and targeting restrictions all read it), so
identity accents and card frames are separate channels; the WUBRG frame tokens in
`src/tokens.ts` stay owner-independent. At a glance, the region answers "whose
stuff" and the card answers "what stuff". Delivered (UI catch-up batch):
`clients/web/src/table/identityAccents.ts` assigns a muted jewel-tone accent per
seat, deterministically from `GameView.seat_order` (every client and a fresh mount
derive the same color); the scene carries it as `Band.accent` for the region border
and nameplate, and the HUD tiles wear it as an edge stripe plus a hexagonal
**life crest** — the total in a rune-framed badge with display-face numerals.
Band labels use display names (never seat ids), completing #294's reach.

The monogram motif grows into a small procedural **glyph language** — inline SVG
in the `RuneMark` mold, no raster assets — for the places a repeated symbol beats
a repeated word: zone piles, phase names, keyword badges (flying, deathtouch, …),
tap state, and seat/ready markers. It ships in
[`clients/web/src/chrome/glyphs/`](../../clients/web/src/chrome/glyphs/) (issue #317):
one authored geometry source (`geometry.ts`) rendered by both a DOM `<Glyph>`
component and a Pixi `buildGlyphDisplay` drawer (ADR 0003), tinted from tokens /
`currentColor` and never a baked hex. Keyword-glyph coverage is asserted against the
engine catalog's shipped keyword set, so a new keyword can never render an empty gap.
Still: no card images, official frames, symbols, or WotC branding, anywhere.

## Palette (dark board)

Board `#15171A`, card body `#23262B`, name text `#E8E6E1`.
Frame accents — W `#CFC7AC`, U `#4E86C1`, B `#77688C` (violet-gray; pure black is
invisible on a dark board), R `#C05B4D`, G `#57935F`, multicolor `#C9A84C`,
colorless `#8C949C`, land `#A08A6E`. Header tint = accent at 14–18% alpha; art
monogram = accent at ~22%. Selection `#7FB2E5` (blue); targeting `#E0784A`
(orange). The two never share a hue — they co-occur on screen.
Playable affordance (a card with an offered action) `#F2C94C` (gold), drawn as a
solid bottom-**edge bar**, not a ring — a different *shape* from selection/
targeting so it stays legible without color vision (ui-requirements §10). It is
suppressed in targeting mode (the only interaction there is picking a target).

Chrome (surfaces, borders, elevation, spacing, typography scale) gets its own
token set alongside the card tokens, consumed by whatever styling layer the
implementation ADR selects — chrome values stop living as ad-hoc hex literals in
per-element inline styles. Delivered by [ADR 0019](../decisions/0019-chrome-styling-layer.md):
the chrome tokens live in [`clients/web/src/chrome/tokens.css`](../../clients/web/src/chrome/tokens.css)
as CSS custom properties, consumed by a CSS-module styling layer; the card tokens
in `src/tokens.ts` stay separate and untouched.

## Card render

The frame is designed around an **art window**: a reserved region of the face
that holds procedural fill (the accent-tinted initial monogram) by default and an
illustration when the player's chosen art source has one (ADR 0024); the name
band, cost pips, type line, and P/T pill keep their positions either way. The
monogram is a placeholder for the art box, not the card's identity. Window-mode
art renders at the **field and hand tiers only** — chip/mini/support keep their
full dense information budget — cover-cropped inside a rounded mask (`ART`
tokens), with a card-body scrim behind the keyword-glyph strip so glyphs stay
legible on any illustration.

**Full-card mode** (ADR 0024, Scryfall source only) replaces the whole face with
the entire official card image at every full-face tier (mini through hand):
RUNE's name band, pips, type line, and keyword strip are suppressed (the image
carries them), while every server-computed overlay — effective P/T pill, counter
and damage badges, combat bars, rings, the playable edge bar, tap — draws on top
unchanged, so the game-state vocabulary is identical across all three faces
(procedural, window, full). Chips stay procedural digests in every mode.

Sources are player-selected per device (procedural default, bundled
project-owned art, or opt-in Scryfall download, device-cached only); the
project itself ships no official imagery (see the brief).

Size tiers: chip 44×60 (lands/digest), mini 54×76 (the stepped-down dense tier
the blueprint's density ladder engages), support 66×92, field 84×118, hand
104×146, plus a "full" inspect tier with type line and rules text. Which tier a
surface uses is the shell layout's call (blueprint §Card vocabulary): the
receiver's battlefield runs one step larger than the opponents', and a crowded
panel steps its own tier down one rung.

Each tier carries a fixed **information budget** — what a card must answer at
that size without opening an inspector. Cost + P/T alone is not a playable card
face; a player has to see the board's keywords and latent abilities without
twenty inspect round-trips.

- **chip** — frame color, name or basic-land glyph, tap state.
- **mini / support / field** — adds cost pips, computed P/T (never printed values; pill
  only), counter and damage badges, keyword glyphs from the identity glyph
  language, and an **ability marker**: a quiet persistent dot for "this permanent
  has an activated ability". The marker is deliberately distinct from the gold
  edge bar, which keeps its exact meaning of "the server is offering an action
  *right now*" — one says latent, the other says live.
- **hand** — the field set at a size where names stay readable.
- **inspect** — everything the server supplies: full rules text, keywords,
  current-vs-printed, counters, attachments, linked objects.

Delivered by issue #320 in the card factory: a capped keyword-glyph strip (from
`CardView.keywords`, overflowing to `+N` rather than shrinking below legibility),
the latent-ability marker dot (distinct in *shape* from the gold playable bar; read
off the printed rules text as a presentation heuristic — the swap point for a future
`has_activated_ability` view field), and the existing counter badge, all at
support/field/hand and never on chips.

**Combat indicators** delivered by issue #332, all straight from the view (the
`Permanent` contract already carried `attacking`, `blocking`, and `damage`; only the
TS mirror lacked them). A declared **attacker** wears a bar on the *top* edge and
keeps full opacity even while tapped (it is in combat, not inert); a declared
**blocker** wears a bar on the *left* edge; a defended attacker shows a `blocked ×N`
badge counting the blockers that name it; and the **marked-damage badge** now renders
from `damage`. Each indicator is a distinct *edge/shape*, so combat state stays legible
apart from selection (ring), targeting (ring), and playable (bottom bar) without hue.
The blocker→attacker relationship is carried as `TableScene.combatLinks` — reconstructed
from the view alone, so a client that mounts mid-combat shows the same links as one that
watched declaration. Combat participants never fold into an ×N stack, so each keeps its
own treatment and link.

**Delivered (issue #339):** the combat links now render as a **canvas-layer overlay**
(`SceneReconciler`), a connector drawn between each blocker and the attacker it blocks.
The connector is a *doubled* (two-parallel) stroke with a small node at the blocker end
— a distinct *shape* from the selection ring, the targeting ring/arrow, and the playable
edge bar, so it reads apart from them without relying on hue. The overlay is **passive**
(`eventMode = 'none'`): a link is never a hit-target and never delays a live prompt.
Density behavior: with few links all draw at full emphasis; on a crowded board
(`COMBAT_LINK.crowdedThreshold`) they calm to a lower alpha until a participant is
focused/selected/hovered, which **isolates** just that object's links (fed from the
table's `highlightedId`/`selectedId` via `BattlefieldCanvas`'s `isolatedId`). Links
**track their endpoints** while the #334 view-diff animation is in flight — the
reconciler redraws them each frame from the cards' current centers — and render
statically at the final positions under reduced motion. The which-to-draw and geometry
policy is a pure module (`combatLinks.ts`); the reconciler only strokes what it returns.

**Multiplayer combat — declaration and target rendering, delivered (issue #347).**
With more than one opponent, an attacker chooses *whom* it attacks (CR 508.1a). The
declaration flow stays server-driven: the `declare_attackers` action carries the
`attackers` subset slot plus one `defend_<permId>` slot per attacker candidate
(server `attacker_requirements`, offered only with several opponents), so the client
computes no legality. The client walks the attacker pick, then — for **each declared
attacker only** — a single defending-player pick made from that opponent's HUD tile
(the same tile affordance as player targeting), and submits one atomic answer. The
common two-player case is the fast path unchanged: a sole opponent means the server
offers no `defend_*` slot, so no extra step appears (`multiSelect.ts` gates a
`defender` slot on its attacker being declared; keyboard, pointer, and touch each
complete the flow). Target **rendering** is reconstructed from the view alone
(`Permanent.attacking_player`): each attacker's face carries whom it attacks
(`CardDisplayData.attackingPlayer`), the scene exposes the attacker→defender
assignments (`TableScene.attackTargets`), and each attacked player's HUD tile shows an
`Attacked ×N` treatment in the targeting accent — so the attack points *toward the
attacked player's tile* and any player, including a bystander not under attack, reads
who-attacks-whom on a fresh mount. Composes with the #339 blocker→attacker links.

## Battlefield bands

Per player, ordered toward the center line: creatures/planeswalkers/battles
(field tier) nearest the opponent, artifacts/enchantments (support tier) behind,
lands as chips at the back. Basics render as glyph chips; nonbasics as small named
cards. Identical-state permanents collapse to one render with an ×N badge
(grouping key = full state identity). Attachments cluster with their host.

The rows are a **sorting convention, not zones**. The game has one battlefield;
the grouping exists purely so a board reads at a glance, and it never earns
rule-implying labels ("frontline", "support" — rejected, see Concept-board
decisions). "Lands" is the only honest row label. Row membership derives from the
server-computed type line, so a permanent migrates rows when its types change —
an animated land moves up among the creatures, a crewed Vehicle likewise — and a
migration gets a subtle transition (honoring reduced motion) so the card doesn't
appear to teleport.

Tapping is **one treatment at every tier** (blueprint §Card vocabulary,
superseding the earlier tier-dependent scheme): a ~25° partial rotation plus a
slight dim (`FRAME.tappedAlpha`), identical for full faces and land chips, with
the row layout reserving the *rotated bounding box* so a tapped card never
overlaps its neighbors. Partial rotation is what keeps small cards legible; the
row gap absorbs the swept corners; the live client renders it as a tween
(reduced motion snaps). Tap state is part of the ×N grouping key, so "four
Plains, one tapped" reads as an untapped ×3 stack beside a tapped single — the
tapped count stays legible at the size where it matters most.

**Actionable permanents stack** (UI catch-up batch). The original grouping refused
to fold any card with an offered action — but every untapped land always carries
its tap-for-mana action, so lands never stacked and boards read as rows of
duplicates. The grouping key now includes the offered-action *fingerprint*
(type + label, never entity-bound ids): permanents whose full visual state and
action shapes are identical are interchangeable and fold into one activatable
stack; activating it submits the representative's action. Pick-specific
affordances — target candidacy, a multi-select pick, the current selection, combat
participation, attachments — still force individual renders, so every physical
object stays addressable in prompts (ui-requirements §Table and zones).

Delivered by issue #318 in the pure `buildTableScene` layout (`clients/web/src/table/scene.ts`)
plus a chip renderer (`buildChipDisplay`) and the `×N` badge in the card factory.

**Aura clustering** delivered by issue #333: `Permanent` now carries `attached_to`
(the host's entity id, projected like `blocking`), so `buildTableScene` pulls an
attachment out of its own type row and lays it adjacent to its host, host first, in
the host's row. A host that carries an attachment — and every attachment — is kept its
own render (never folded into an `×N` stack), so the cluster stays coherent and each
object stays individually addressable in prompts; the inspector names the relationship
from either side. An attachment whose host is not in the same band (e.g. an aura on an
opponent-controlled creature, or a host the viewer cannot see) degrades to its own row.

The **row-migration transition** delivered by issue #334 as the reconciler's opt-in
animate-the-diff layer (`sceneReconciler.ts`): a card that migrates rows/positions
eases to its new spot, an entering card fades up, and a leaving card fades out before
it is destroyed. Row *membership* already follows the server type line, so a permanent
moves rows the instant its types change; the layer now tweens that move. The layer
interpolates strictly between two authoritative scenes and never gates input — hit
targets come from the `TableScene` rects the DOM overlay reads, so a card is
addressable the instant its scene arrives, whatever its pixels are doing. It is
opt-in and honors `prefers-reduced-motion` (which snaps, with no layout or state
difference), so the reconnect/replay determinism invariant and every existing test
hold unchanged.

## Multiplayer table (3–4 players)

The composition above was designed, tuned, and tested at exactly one opponent. A
free-for-all seats three or four (issue #349), so the table must lay out two or
three opponent areas readably without displacing the receiver — the M5 outcome
"2–8 player tiles without moving the receiver from the bottom interaction area"
(`ui-requirements.md` §Players). This is the in-repo concept decision for the 3–4
player arrangement (issue #348); the visual system and shell architecture are
unchanged, so no ADR.

**Arrangement.** The receiver is always anchored at the bottom — its hand row and
local dock keep the bottom interaction area at every seat count — and opponents are
added *toward the top*, never by pushing the receiver up. Each opponent is a full
battlefield **band** (the same type-grouped rows, zone piles, and combat treatments
as the two-player board), stacked vertically inside the battlefield region above the
receiver's own band. Bands are stacked in the table's **seat order**
(`GameView.seat_order`, issue #345), so opponent areas keep a **stable relative
position** across every view update: a bystander who mounts mid-combat reads the same
seating as one who watched the game fill, and an opponent never reshuffles because a
life total changed. Every seated player gets a band even with no permanents, so a
three-opponent table always shows three opponent areas; an eliminated seat (issue
#342) keeps its band and its public piles while its permanents leave the game.

The **opponent HUD strip** across the top carries one identity/life/hand tile per
opponent, reflowing by count (a wide tile at 2p → a compact grid as opponents are
added) without moving the receiver, who lives in the bottom-left dock. Each band
still owns its own zone piles (library/graveyard/exile) parked in its lane — the
count lives once, on the board, and the HUD never repeats it.

**Density and collapse.** Three opponents at a small viewport is the hard case. The
board never scrolls horizontally at any supported geometry (the pure `layout()`
sizes the scene to the battlefield width and the scene wraps within it); instead the
stacked bands grow downward and the battlefield region scrolls vertically if the
whole table overflows. Degradation is graceful before anything becomes unreadable or
untappable: the HUD strip reflows to a grid, bands condense toward their chip tiers,
and every interactive target stays ≥ 44 px. The two-player composition is left
exactly as tuned — the multiplayer arrangement is additive and only engages once
there is more than one opponent.

**Focus and combat legibility.** The spatial focus model (#301) reaches every
opponent area: each opponent's board permanents and zone piles are focusable surfaces
in the battlefield region, and on a multiplayer table each opponent's HUD tile
becomes a keyboard/controller focus anchor (in a duel the single opponent tile stays
quiet display, so the finely-tuned two-player focus order is unchanged). Combat
treatments and the #339 blocker→attacker links compose across opponent areas: a split
attack (one attacker at each of two opponents, issue #347) renders each attacker's
treatment in its own band and its link to the blocker in the attacked opponent's
area, so who-attacks-whom stays legible on a crowded multi-opponent board.

## Action routing

Every entity-owned action has a `subject`. Entity-subject actions render as the entity's
interactivity (the card is the button); the tray holds only global actions plus a
contextual echo of the selected entity's actions. The tray is O(1) regardless of
hand size. Interaction is select-then-confirm everywhere (tap/click twice, or
Enter), which unifies mouse, touch, keyboard, and controller focus.

## Input capability model

Capabilities, not devices. The client detects what the environment offers —
pointer precision, hover, a physical keyboard, a gamepad — and adapts affordances,
never layout order:

- Every interactive target is at least 44 CSS px; nothing requires drag or hover.
- Keyboard/controller share one spatial focus model: focus moves *between regions*
  and *within a region's items*, rather than tabbing a flat list of every button on
  screen. Focus is always visible.
- Hover adds transient previews for precise pointers; touch and controller reach
  the same information through select or inspect.

## DOM/canvas split ([ADR 0003](../decisions/0003-dom-canvas-split.md))

Pixi canvas: battlefield, hand, targeting arrows, animations.
React DOM: prompt overlays, action tray, HUDs and the stack/activity rail, log,
zone browsers, inspect. Rule of thumb: text you read or click outside a rendered
card is DOM; things that move are canvas. DOM anchors to canvas objects only via
reported rects, never by reaching into the scene.

## Prompts, zones, inspect

All choices (targets, modes, X, ordering, searches) use one prompt queue with an
anchored overlay, spotlight, and progress. The client collects server-supplied
choices and submits them atomically.
One zone-browser component serves graveyard/exile/search/reveal/select-from-zone.
Inspect has two intensities: transient peek (hover dwell) and pinned panel
(right-click / long-press / select+I) showing current-vs-printed state and
related cards. The library is never present client-side beyond counts and
server-revealed subsets.

**No card carries a permanently visible inspect button.** An always-on overlay
control on every object is board noise that scales with the board (the per-card
handles shipped under issue #261 are retired by this model). Inspect rides
interactions the player is already making: selecting a card also surfaces its
preview in one consistent home, hover dwell peeks on precise pointers, long-press
peeks on touch — each satisfying the pointer/keyboard/touch requirement without
adding chrome per card. Delivered by issue #321: the per-card handles are gone;
an actionable card hosts the inspect gestures on its select/target hotspot, and any
other card (an opponent's permanent, an inert hand card) on a transparent, focusable
**inspect surface** — invisible, but keyboard/AT-reachable, so no visible control
scales with the board. A peek renders as a non-blocking, `pointer-events: none`
preview (the transient `CardInspect`) in a fixed home, honoring `prefers-reduced-motion`;
right-click / select+I / activating a surface pins the full panel. Hover and
long-press are suppressed mid-pick (targeting); pinning stays reachable.

## Spectate mode

A spectator (ADR 0022, issue #351) is a non-seated observer watching a live game.
Its view is a `SpectatorView` — the public intersection only, redacted **by
construction** (the type has no hand, mana pool, or `valid_actions` field), so the
spectate shell cannot render hidden information even by mistake. It **reuses the
board, stack, log, HUD, phase indicator, and inspect renderers** and differs only in
the shell: there is no hand row, no action tray, no local dock, and no interactive
affordance. Every seat renders as a player HUD tile and a battlefield band — there is
no privileged "self" (the scene builds with no receiver, so every band is an opponent
band). Read-only inspection stays: cards peek/pin-inspect and public graveyard/exile
piles open the zone browser, but nothing is selectable, targetable, or submittable.
Where the receiver's dock would sit, a quiet "Spectating" badge marks the mode. The
whole UI reconstructs from one `SpectatorView`, so a spectator that joins mid-game (or
reconnects) renders the complete public board from its first frame. The terminal
verdict shows with no personal win/lose framing. Delivered by `SpectatorTable`
(`clients/web/src/table/SpectatorTable.tsx`); the lobby directory offers a **Spectate**
button on any in-progress room and advertises the spectator count.

## Concept-board decisions

The 2026 concept boards (pre-game screens, menus, table, multiplayer
overview/focus, mobile portrait) were mined for direction. Recording what was
adopted and what was rejected so the reasoning isn't relitigated:

Adopted: the dark-tabletop composition (it matches the shell region-for-region);
the right-edge stack / activity / phase rail; a state legend built on the
existing selected / targeting / playable accents; compressed opponent rows at
portrait geometry; the overview/focus mode pair (#301); an identity built on
runes, a display face, and disciplined gold; the menus board's drawer — adopted
in restrained form as the game menu (shortcuts + confirm-stepped concede), minus
its ornament; the hexagonal life badge — translated to the procedural life crest.

Rejected:

- **Labeled pseudo-zones ("FRONTLINE", "SUPPORT").** No basis in the game's
  rules; the type-grouped rows stay a sorting convention (see Battlefield bands)
  and only "Lands" earns a label.
- **A fixed verb bar (PLAY LAND / CAST SPELL / ATTACK / ABILITIES / PASS).**
  Hard-coding a verb vocabulary reintroduces client-side assumptions about what
  actions exist. ADR 0004 stands: entity actions render on entities; the tray is
  O(1), server-labeled, and never enumerates categories.
- **Owner-colored cards.** The boards paint every card in its owner's color,
  erasing the permanent's own color identity — game information. Identity color
  lives on regions and nameplates only (see Identity).
- **Painted texture and engraved ornament.** Mood is carried by tokens and
  geometry (see Design stance).
- **Compass-direction player headers ("OPPONENT NORTH").** Players are names;
  seat position is a layout concern, not a label.
- **Card faces without state.** The boards show cost + P/T only, never tap
  rotation, keywords, or ability markers; the information budget in Card render
  is the corrective.
