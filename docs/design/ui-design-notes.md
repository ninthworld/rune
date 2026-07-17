# UI design notes — locked decisions

Condensed from the design phase. The working reference implementation is
`prototypes/ui-battlefield-v3.html`. Full capability list and stress analysis:
`design/ui-requirements.md`. Change tokens in `clients/web/src/tokens.ts` only
in lockstep with this document.

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

Every action has a `subject`. Entity-subject actions render as the entity's
interactivity (the card is the button); the bar holds only global actions plus a
contextual echo of the selected entity's actions. The bar is O(1) regardless of
hand size. Interaction is select-then-confirm everywhere (tap/click twice, or
Enter), which unifies mouse, touch, and controller focus.

## Layout model

Me fixed at bottom (hand, battlefield, actions); opponents are uniform tiles that
reflow by count (1 tile at 2p → grid at 8p), with one tile auto-expanded by
attention rules (turn change, being targeted). Two modes — overview and focus —
differ only in density; regions never reorder. One `layout(viewport, mode,
playerCount)` function positions both layers.

## DOM/canvas split (ADR 0003)

Pixi canvas: battlefield, hand, stack cards, targeting arrows, animations.
React DOM: prompt banners, action bar, player tiles + zone rail (library /
graveyard / exile chips), log, zone browsers, inspect. Rule of thumb: text you
read or click is DOM; things that move are canvas. DOM anchors to canvas objects
only via reported rects, never by reaching into the scene.

## Prompts, zones, inspect

All choices (targets, modes, X, ordering, searches) are one uniform prompt queue
with banner + spotlight + progress, answered client-side and submitted atomically.
One zone-browser component serves graveyard/exile/search/reveal/select-from-zone.
Inspect has two intensities: transient peek (hover dwell) and pinned panel
(right-click / long-press / select+I) showing current-vs-printed state and
related cards. The library is never present client-side beyond counts and
server-revealed subsets.
