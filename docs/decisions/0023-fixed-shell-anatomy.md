# ADR 0023: Fixed-shell anatomy with one action home

- Status: accepted
- Date: 2026-07-18

## Context

The shipped table (issues #295–#301 and the July 2026 catch-up batch) used a
floating-chrome model: the battlefield owned the viewport and the local dock,
hand, and action tray *overlaid* it, with the hand drawn inside the scrollable
battlefield scene and per-card actions rendered as popups on the entity. Every
recurring presentation defect traced to that model — chrome overlapping the hand,
action popups clipping at the screen edge, the tray drifting relative to its
neighbors, prompt overlays colliding with controls. Polishing regions could not
fix a failure class the architecture guaranteed.

A design investigation (three high-fidelity mocks at laptop 1440×900, tablet
1180×820, and phone-portrait 390×844, each against a hostile 4-player or duel
state; see `docs/design/ui-blueprint.md`) demonstrated that a fixed anatomy
carries the full requirement matrix — multiplayer, degenerate boards, touch,
small screens — with one shared presentation vocabulary.

## Decision

The client shell is a **carved, fixed layout**: top status bar, opponent
panel(s), the receiver's battlefield panel, a right rail (stack + activity), and
a bottom shell owning the receiver's identity, piles, hand, and a single
**action dock**. Regions never float over one another and never reorder;
geometry breakpoints change composition (panels reflow, rail collapses to
chips/sheets, hand becomes a fan), never the anatomy's ownership rules.

Every server-offered action renders in the action dock; selecting an entity
routes its actions there. Per-card action popups are removed. Overlays/sheets
(zone browsers, option pickers, expanded phase strip) are the only layer that
may cover the shell and must be viewport-clamped.

ADR 0003 (DOM/canvas split) and ADR 0004 (subject-owned actions, O(1) action
surface) stand; ADR 0004's contextual echo *becomes* the single action home
rather than duplicating per-entity popups. The hand leaves the battlefield
scene and becomes a shell region.

## Consequences

- The overlap/clipping defect class is eliminated by construction, and every
  UI state (empty stack, no actions, mid-drag) has a designed home.
- Zone homes are stable, which makes travel animations (draw, play, die, tap)
  legible and makes drag targets deterministic.
- The board no longer owns the entire viewport; panels are bounded. Degenerate
  boards are absorbed by the density ladder (tier step-down, ×N folding) rather
  than unbounded growth.
- The shipped floating-shell layout (`layout()` floating regions, scene-drawn
  hand, `EntityOverlay` action popups, anchored prompt overlay positioning) must
  be reworked against the blueprint — a deliberate, sizeable implementation
  effort tracked in `docs/design/ui-redesign-plan.md`'s successor work.
