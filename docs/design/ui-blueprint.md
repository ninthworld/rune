# UI blueprint — the fixed-shell tabletop

**This document is the design authority for the client's screen anatomy,
interaction model, and presentation vocabulary.** It was produced by the July 2026
design investigation: three high-fidelity mocks at the three load-bearing
geometries, each against a deliberately hostile game state, iterated with the
project owner. Where this blueprint conflicts with older presentation text in
[`ui-design-notes.md`](ui-design-notes.md), **this document wins**; the notes
remain authoritative for card tokens, palette, the card-face information budget,
combat indicators, and legal constraints. The architectural decision to move from
the floating-chrome shell to this fixed anatomy is recorded in
[ADR 0023](../decisions/0023-fixed-shell-anatomy.md).

The evidence (reference-only prototypes, never imported):

| Geometry | Case proven | Prototype |
| --- | --- | --- |
| Laptop 1440×900 | 4 players, ~60 permanents, mid-combat, 4-deep stack | [`prototypes/ui-table-4p-laptop-v1.html`](../../prototypes/ui-table-4p-laptop-v1.html) |
| Tablet 1180×820 (the floor) | same board, density ladder engaged, drag-to-play state | [`prototypes/ui-table-4p-tablet-v1.html`](../../prototypes/ui-table-4p-tablet-v1.html) |
| Phone portrait 390×844 | duel, hand fan, fixed action bar, thumb-reach shell | [`prototypes/ui-table-duel-phone-v1.html`](../../prototypes/ui-table-duel-phone-v1.html) |

The conclusion of the investigation: **one design system spans the whole
requirement matrix; what varies per geometry is composition, not language.**

## The three commitments

1. **Fixed shell.** Every element has a permanent home in a carved layout —
   nothing floats over anything, so nothing can overlap or clip *by
   construction*. Regions never reorder; they condense, collapse to chips/sheets,
   or change composition at a geometry breakpoint. This replaces the
   floating-chrome model (dock/hand/tray overlaying the board), which was the
   root cause of the shipped client's overlap and clipping bug class.
2. **One action home.** Every server-offered action renders in the **action
   dock** — a fixed location near the hand. Selecting a card (hand or
   battlefield) lifts it and routes its offered actions there; the primary action
   (pass/resolve) is always the same button in the same place. Per-card popup
   menus are abolished (a popup under a bottom-edge card is guaranteed to clip;
   in this anatomy it cannot exist). ADR 0004 is preserved, reinterpreted: the
   *entity* is still the selection surface and `subject` still routes actions;
   what changed is that the offered actions render in the dock rather than on a
   per-card popup, and the dock stays O(1).
3. **Flat-but-deliberate, animation-first.** The surface is structural, not
   textured: line-work panel borders with corner notches, per-player identity
   accents, layered dark gradients, one type system, gold = actionable, orange =
   targeting. Painted texture stays a *deferrable skin* (token-driven, decided
   later, possibly per-platform). Motion is a first-class design material: zone
   changes travel (library→hand, hand→battlefield, battlefield→graveyard pile),
   tap is a rotation tween, rows close gaps with eased reflow. Fixed zone homes
   are what make travel animations legible. `prefers-reduced-motion` snaps
   everything with no layout or state difference.

## Screen anatomy

### Desktop / laptop (multiplayer or duel)

```
┌────────────────────────────────────────────────────────────┬──────────────┐
│ top bar: brand · turn/phase strip · combat status · log · menu            │
├──────────────────┬──────────────────┬──────────────────────┼──────────────┤
│ opponent panel   │ opponent panel   │ opponent panel       │ STACK        │
│ (crest·name·hand)│  (active marker) │  (attacked marker)   │  (items or   │
│  creature row    │   …              │   …                  │   quiet      │
│  support row     │                  │                      │   empty)     │
│  land chips      │            piles │                piles ├──────────────┤
├──────────────────┴──────────────────┴──────────────────────┤ ACTIVITY     │
│ YOUR BATTLEFIELD (full width panel)                        │  (log, turn- │
│   creature row (larger tier)                     [piles]   │   grouped)   │
│   support row · land chips                                 │              │
├──────────────────┬─────────────────────────────┬───────────┤              │
│ crest · name     │ prompt strip                │ ACTIONS   │              │
│ mana · statuses  │ hand row (largest tier)     │  dock     │              │
│ your piles       │                             │           │              │
└──────────────────┴─────────────────────────────┴───────────┴──────────────┘
```

- In a duel the opponent row is one wide panel; the composition is otherwise
  identical. Opponent panels reflow by count (1 wide → 3 across; beyond 3, two
  rows or the density ladder).
- The right rail owns the stack (top, adjacent to the boards it resolves into)
  and the activity log. The stack shows a designed quiet state when empty —
  chrome never disappears and reflows.
- The bottom shell owns the receiver: identity crest, mana, **your piles**
  (largest pile tier), the hand, and the action dock. Nothing may render over it.

### Tablet landscape (~1180×820) — the floor for full multiplayer anatomy

Same anatomy one notch tighter: opponents at the stepped-down card tier by
default, rail and shell condensed. This is the smallest geometry that shows
three opponent battlefields in full; the density ladder is load-bearing here,
not cosmetic. Below this width, multiplayer must change kind (see support
matrix).

### Phone portrait (~390×844) — the change of kind

A duel still shows **both battlefields in full**. What changes:

- the top bar compresses to a turn pill + phase-progress dots; the full step
  strip, stack, and log collapse to **chips that open sheets**;
- the hand becomes an **overlapping fan**; tapping a card lifts it to full size;
- the **action bar** sits fixed between board and hand, thumb-sized (≥44 px);
- all interaction lives in the bottom half (thumb reach); the top half is
  display;
- board panels may scroll internally in degenerate board states (acceptable on
  mobile only).

## Card vocabulary (identical at every geometry)

- **Tiers**: hand/fan (largest) → your battlefield → opponent battlefield →
  stepped-down dense tier → land chips. Exact pixel values live in tokens; the
  *set of faces* never changes, only which tier a surface uses.
- **Tap is one treatment everywhere**: ~25° rotation + slight dim, at every tier
  including land chips. Partial rotation is what keeps small cards legible; the
  row gap absorbs the swept corners. Rendered as a tween.
- **×N stacks**: identical-full-state permanents (including identical offered
  action shapes) fold into one render with an ×N badge. Pick-specific
  affordances (target candidacy, selection, combat participation, attachments)
  always force individual renders.
- **Attachment clusters**: equipment/auras splay *behind* their host, name band
  peeking above, one slot wide at every tier. The peeking band is the
  attachment's own selection surface.
- **Art window**: every card face reserves art. Placeholder procedural art
  (seeded painterly fill) stands in until real art exists; a dark scrim at the
  art's bottom edge keeps P/T and keyword glyphs legible. **Art policy**:
  original (including AI-generated) art for RUNE's own cards may ship;
  user-supplied image packs are a strictly local, opt-in feature; official
  imagery is never shipped or fetched (see the brief).
- Badges keep their existing shape-coded meanings (counters, marked damage, ×N,
  gold actionable bar, selection/targeting rings) per the information budget in
  `ui-design-notes.md`.

## Density ladder

Per player panel, engaged automatically in order:

1. full tier for the surface;
2. one card-tier step down;
3. aggressive ×N folding;
4. (mobile only) internal panel scroll.

Each panel picks its own rung — one hoarding opponent never shrinks the others.

## Interaction model

- **Select → confirm** is the universal path (pointer, touch, keyboard,
  controller): selecting an entity lifts/rings it and routes its offered actions
  to the action dock; the prompt strip states the pending question in words.
- **Direct activation shortcuts the round trip where intent is unambiguous**
  (ADR 0025), on the same single gesture in every input mode: a sole
  server-flagged mana ability fires on the first activation (tap the land, get
  the mana); a combat-declaration candidate enters the declaration pre-toggled;
  and activating the already-selected entity again fires its sole action. The
  dock remains the labeled home for every action and the only home for
  ambiguous ones.
- **Drag is an enhancement, never required**: dragging a playable hand card
  ghosts it under the pointer/touch point, holds its origin slot open in the
  hand, and lights the **legal drop area** (gold inset on your battlefield for a
  permanent; orange rings on legal targets for a targeted spell). Release snaps
  into the sorted row — there is no free placement, so drops are deterministic.
  Esc/release-outside cancels back to the origin slot (animated).
- **Prompts**: the prompt strip + action dock carry every decision's question,
  progress, and controls. Zone browsers, option pickers, and the expanded phase
  strip open as overlays/sheets *above* the shell — the only layer permitted to
  cover it, always viewport-clamped, always dismissible.
- Server-authoritative interaction is unchanged: `valid_actions[]` is the only
  source of interactivity; the client computes no legality anywhere in this
  blueprint.

## Support matrix

| Surface | Duel | 3–4 player |
| --- | --- | --- |
| Desktop / laptop landscape | full anatomy | full anatomy |
| Tablet landscape | full anatomy | full anatomy (floor; ladder engaged) |
| Phone / portrait | full anatomy, fan + sheets | **summary tiles + focus mode** (opponents collapse to crest/name/counts tiles; tapping one expands that battlefield) — designed, not yet mocked |

## Superseded and preserved

- **Superseded**: the floating-chrome shell (floating dock/hand/tray regions,
  hand drawn inside the battlefield scene, per-card action popups, anchored
  prompt overlays positioned over the board, zone piles inside band margins).
- **Preserved**: ADR 0003 (canvas draws cards/board; DOM draws chrome — the
  fixed shell is DOM, panels' card areas are canvas-backed); ADR 0004 (subject
  routing, O(1) action surface — reinterpreted per commitment 2); the card
  tokens, glyph language, palette, information budget, combat indicator shapes,
  identity accents, and every engine/protocol invariant.

## Open design items

1. Front-door screens (main menu, play/room flow) in this language — replaces
   the IP-entry connection screen; server address becomes a default +
   advanced/settings affordance.
2. The 4-player phone/portrait summary-tile + focus-mode composition (mock).
3. Targeting-drag and stack-response states on phone (mock).
4. The texture-skin question — explicitly deferred; revisit only after the
   flat-but-deliberate surface ships and reads.
