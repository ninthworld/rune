# UI redesign plan — closing the gap to the concept boards

> **Status: phases A–E delivered** by the UI catch-up batch (July 2026, single PR),
> with the decisions folded into [`ui-design-notes.md`](ui-design-notes.md).
> **Superseded in direction:** the design investigation that followed produced
> [`ui-blueprint.md`](ui-blueprint.md) and
> [ADR 0023](../decisions/0023-fixed-shell-anatomy.md) — a fixed-shell anatomy
> that replaces the floating-chrome model this plan polished. The next
> implementation effort builds against the blueprint, not against this plan.
> Still open from here: **F2** — screenshot checks in the real-browser canary
> (#279). The audit and phase text below are kept as the record of what was
> found and why each change was made.
>
> **Note (July 2026):** the "graphics too heavy/textured" rejection recorded in
> the critique table below is itself superseded as direction by the 2.5D
> presentation pivot ([ADR 0029](../decisions/0029-2-5d-presentation-direction.md),
> issue #464). It stands only as the record of the decision made at the time.

A delivery plan, not a new design. The design source of truth stays
[`ui-design-notes.md`](ui-design-notes.md); its §Concept-board decisions section already
records what the 2026 concept boards got right and wrong, and this plan does not relitigate
those calls. What this plan adds is an honest audit of where the *shipped* table falls short
of the *agreed* design, plus an ordered batch of issue-sized work items to close the gap.
When a phase lands, fold its decisions into `ui-design-notes.md` and its status into
[`../roadmap.md`](../roadmap.md), then retire the corresponding section here.

## Concept critiques, disposition

Raised against the concept boards in the July 2026 review; almost all were already
recorded decisions:

| Critique | Disposition |
| --- | --- |
| "Frontline/support/lands" rows have no rules basis | Already rejected — rows stay a sorting convention; only "Lands" earns a label (§Battlefield bands) |
| Concepts never show tapping | Already solved — 90° rotation with reserved footprint at field/support tier, dim + corner glyph at chip tier (#318) |
| Concepts don't foresee card images | Already solved — the frame reserves an art window; the monogram is a placeholder, not the identity (§Card render) |
| Card faces show no keywords / activated abilities | Already solved — the per-tier information budget: keyword glyph strip + latent-ability marker (#320) |
| "Opponent North" headers are nonsense | Already rejected — players are names; seat position is layout, not a label (§Identity) |
| Graphics too heavy/textured, would constrain future design | Already rejected — mood via tokens and procedural geometry, not painted ornament (§Design stance) |

Disposition of the critiques is **keep the existing decisions**; no design-doc change is
needed. The gap is delivery: the shipped table under-delivers the agreed design badly
enough that it reads as a gray dashboard anchored to one corner.

## Gap audit (grounded in code)

1. **The expanded phase indicator clips.** `layout()` hands `indicator` a fixed 48 px
   strip (`table/layout.ts` `L.indicatorH`); the expanded twelve-step `<ol>` renders
   *inside* that strip (`Table.tsx` wraps `PhaseIndicator` in `regionBox`, and
   `.regionIndicator` is `overflow: auto`). The expansion can never display fully — a
   direct violation of ui-requirements §Stack, priority, and timers ("expansions of the
   phase display must render entirely within the viewport, never clipped by an edge").

2. **The board is small and left-anchored; big screens are wasted.** Card tiers are fixed
   pixel sizes (`tokens.ts` `TIER`), `flowRow` lays cards from the left margin, the scene's
   reported width is its content extent, and `sceneBox` sits at the battlefield region's
   origin. Nothing centers and nothing scales up, so a 27-inch monitor shows a phone-sized
   table in the top-left corner. `ui-design-notes.md` §Tabletop shell already requires the
   opposite ("large screens are *spent*, not left over"); it is specified but unimplemented.

3. **Identical lands do not stack, because they are actionable.** `groupStacks`
   (`table/scene.ts`) refuses to fold any card with `actions.length > 0` into an ×N stack.
   An untapped basic land always carries its tap-for-mana action, so every untapped Plains
   renders individually — the ×N collapse only ever engages on tapped or action-less
   permanents, which is why boards read as a row of duplicates. The "individually
   addressable" guard is correct for targeting/multi-select (a prompt must reach each
   physical object) but too strict for ordinary actionability when the members' full state
   — including their offered action labels — is identical.

4. **Zone piles read as header icons, not piles.** `ZonePile` renders a 20 px glyph plus a
   count inside the 48 px band-header strip (`TableGeography`). The design calls for
   "card-shaped spatial objects … parked in a consistent corner of every player's region —
   findable at a glance" (§Zone piles). The component contract is right (glyph identity,
   count's single home, `faceUp` slot); the *presentation* is a fraction of the intended
   size and sits in chrome, not on the table.

5. **The action tray floats detached and mixes stakes.** The tray anchors at
   `x = dockW + pad·2`, left-aligned, so "Pass priority" and "Concede" sit together in a
   lone box between regions. Concede — the highest-stakes action in the game — is one
   slip away from the most-pressed button and visually part of routine play.

6. **Bands are labeled by seat id.** `bandLabel` renders `p1` / `p0 (you)` even though
   display names ride the protocol (#294) and the HUD already uses them.

7. **Structure without identity.** The shipped surfaces deliver the §Visual hierarchy
   tiers only faintly: one border color for every band (the per-player identity accents of
   §Identity are undelivered), a flat table background (no vignette or procedural motif),
   life totals as small caption text, and floating chrome that barely separates from the
   board. This is the "gray dashboard" failure §Design stance names as the shipped danger.

## Plan

Ordered by impact over effort. Each item is one issue-sized change; A and B are
independent of C–E and can proceed in parallel. No engine or protocol change is required
anywhere in this plan (item C2 uses graveyard contents already present in `GameView`).

### Phase A — bugs and broken promises

- **A1. Un-clip the phase indicator.** The expanded step list becomes a floating tier-4
  overlay dropping below the collapsed bar (same pattern as the rail's floating expansion),
  sized and clamped to the viewport. Add a test asserting the expanded list's rect is fully
  inside the viewport at representative geometries, per ui-requirements.
- **A2. Display names on bands.** `bandLabel` uses `playerName(view, id)` with the
  existing seat-derived fallback; "(you)" marker unchanged.
- **A3. Let identical actionable permanents stack.** Extend the ×N grouping key with the
  card's offered-action *labels* (not entity-bound ids): members whose full visual state
  and action set are identical fold into one stack; activating the stack submits the
  representative's action id. Targeting/multi-select candidacy, selection, chosen state,
  combat participation, and attachments still force individual renders (a prompt must
  address each physical object — ui-requirements §Table and zones). Four untapped Plains
  finally read as one chip ×4; tapping the stack floats one mana.

### Phase B — spend the screen

- **B1. Scene scale.** `layout()` derives a `sceneScale` from the battlefield rect
  (clamped, e.g. 1.0–1.6, stepped so text stays crisp); `buildTableScene` multiplies its
  tier geometry by it. Both renderers already consume the same reported rects, so the DOM
  overlay, hotspots, and combat links inherit the scale for free. Pure and deterministic:
  same viewport ⇒ same scale ⇒ same rects.
- **B2. Center and balance the board.** The scene centers horizontally in the battlefield
  region; vertically, opponent bands justify from the top, the local band anchors adjacent
  to the hand, and slack space distributes *between* bands instead of pooling at the
  bottom. The wrap budget and vertical-scroll overflow behavior are unchanged.
- **B3. Hand presentation at scale.** The hand row centers bottom-center and adopts the
  scaled hand tier, keeping the tray/prompt anchors above it.

### Phase C — zone piles as table furniture

- **C1. A pile column per band.** Piles move out of the header strip onto the table: a
  vertical column parked at a consistent edge of every player's region (mirrored for
  opponents), rendered at chip footprint minimum and scaling with B1. The library draws as
  a card *back* (stacked-edge offset so it reads as a physical pile) with its count; empty
  piles render as a faint outline slot, so the geography never disappears.
- **C2. Graveyard shows its top card.** Graveyard contents are already public in
  `GameView.graveyards`; the pile's existing `faceUp` slot renders the top card's face at
  chip tier, so "what died last" is board-visible without opening the browser. The library
  `faceUp` slot stays reserved for a future server reveal (protocol change, unchanged).

### Phase D — the priority surface and the game menu

- **D1. Tray redesign.** The tray centers above the hand, visually tied to it; the primary
  offered action (pass/resolve) gets the prominent treatment with its keybind hint; the
  contextual selection echo keeps its ADR 0004 shape (O(1), server-labeled, never a verb
  vocabulary).
- **D2. A game menu.** Concede leaves the tray for a small always-present menu affordance
  (top corner of the shell), opening a restrained drawer: concede (with confirm step),
  shortcut help (relocating `ShortcutHelp`'s trigger), and the future settings home
  (reduced-motion, text scale). This adopts the menus concept board in shape, minus its
  ornament. Menu state is ephemeral presentation, never load-bearing.

### Phase E — personality, carried by tokens

- **E1. Table surface.** A subtle radial vignette plus a very-low-alpha procedural rune
  motif (the `RuneMark`/glyph geometry, not a raster) centered under the battlefield, so
  the board reads as a table rather than an app background (§Visual hierarchy tier 1).
  Token-driven; respects the no-texture decision.
- **E2. Per-player identity accents.** A deterministic seat-indexed accent palette
  (disjoint from the WUBRG frame hues, selection blue, targeting orange, and gold) tints
  each player's band border, nameplate, and HUD tile — never their cards (§Identity).
- **E3. Life crests.** Life totals become the identity moment they are in the concepts: a
  procedural rune-framed badge with display-face numerals on HUD tiles and the local dock,
  replacing caption-sized text. Damage/gain flashes honor reduced motion.
- **E4. Elevation pass.** Floating chrome (dock, tray, rail, HUD) gains the shadow and
  contrast step of §Visual hierarchy tier 3; band interiors quiet down (thinner header,
  fainter empty-band hint) so cards, not boxes, carry the board.

### Phase F — verification and docs (continuous)

- **F1.** Each phase updates `ui-design-notes.md` (A3's stacking decision, B1's scale
  model, C1's pile column, D2's menu) and the roadmap on landing.
- **F2.** The real-browser canary (#279, unblocked) gains screenshot checks at a small and
  a large viewport, so "spends the screen" and "never clipped" stay verified instead of
  re-regressing silently.
