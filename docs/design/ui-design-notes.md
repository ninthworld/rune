# UI design decisions

The current design source of truth for the web client's presentation. The full
capability list is in [`ui-requirements.md`](ui-requirements.md); delivery status
belongs in [`../roadmap.md`](../roadmap.md). The historical reference
implementation is [`prototypes/ui-battlefield-v3.html`](../../prototypes/ui-battlefield-v3.html).
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

## Tabletop shell

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
  counts they merely summarize; a count appears in exactly one home.

One pure function `layout(viewport, mode, playerCount)` positions every region for
both renderers. It keys on measured geometry (width, height, aspect) and input
capabilities (pointer precision, hover availability) — not user-agent sniffing or
desktop/mobile breakpoints. Portrait, landscape, and ultrawide all resolve from
the same function. Two modes — **overview** and **focus** — differ in emphasis and
density only; regions never move between modes.

## Visual hierarchy

The uniform panel look (one background, one border color, one radius everywhere)
is replaced by explicit surface tiers:

1. **Table** — the deepest layer: the board surface, with subtle procedural
   texture/vignette so it reads as a table, not an app background.
2. **Board regions** — player lanes, hand row, zone piles: bounded areas *on* the
   table, delineated by geometry and tint, not chrome boxes.
3. **Floating chrome** — HUDs, tray, rail, indicator: elevated above the table
   (shadow/contrast), visually lighter than the board they frame.
4. **Overlays** — prompts, inspect, zone browsers, game over: topmost, with the
   layers beneath dimmed.

Typography gets a scale (display / heading / body / caption) and a distinctive
display face for identity moments — the wordmark, victory/defeat, phase names.
Body text stays a legible system stack. Effects remain restrained per the brief
(no 3D, no particle noise): elevation, tint, and motion that always honors
`prefers-reduced-motion`.

## Identity

RUNE's look is procedural geometry: the rune/monogram motif from the card
renderer extends to the pre-game screens and sparse table accents. Connection and
lobby are the product's front door and share the table's visual system — not
generic dark forms. Players read as people, not seat ids: display names ride the
protocol (a contract change, issue #294; seat-derived fallbacks like "Player 2"
until then).
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

No images. Frame color + oversized initial monogram stand in for art. Battlefield
size shows name, cost pips, computed P/T (never printed values; pill only).
Size tiers: chip 44×60 (lands/digest), support 66×92, field 84×118, hand 104×146,
plus a "full" inspect tier with type line and rules text.

## Battlefield bands

Per player, ordered toward the center line: creatures/planeswalkers/battles
(field tier) nearest the opponent, artifacts/enchantments (support tier) behind,
lands as chips at the back. Basics render as glyph chips; nonbasics as small named
cards. Identical-state permanents collapse to one render with an ×N badge
(grouping key = full state identity). Attachments cluster with their host.

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
