# ADR 0029: Pivot to a polished 2.5D presentation direction

- Status: accepted
- Date: 2026-07-24

## Context

The client's presentation direction has, until now, been deliberately
graphics-light: the brief excluded "a 3D or effects-heavy presentation", the
design stance carried personality "by tokens and procedural geometry, never by
surface weight" and rejected painted texture and ornament, and the blueprint's
third commitment was a "flat-but-deliberate" structural surface with texture
deferred as an optional skin. Those constraints produced a functional but
utilitarian interface that reads as a rules-engine dashboard rather than a
game (issue #450's playtest findings; issue #464).

The project owner has reversed that direction (issue #464, with the approved
visual baseline committed as
[`docs/ui-concepts/rune-2.5d-interface-baseline.jpg`](../ui-concepts/rune-2.5d-interface-baseline.jpg)).
The target is a **polished 2.5D presentation**: cleaner and flatter than a
heavily textured fantasy tabletop, slightly cartoon-like, professionally
illustrated, tactile, animated, and visually exciting — with depth carried by
composition, perspective, motion, shadows, focus, and layering. Games such as
Magic: The Gathering Arena set the quality bar for tactility, staging,
effects, and clarity; RUNE develops its own identity and stays multiplayer-
and Commander-first.

## Decision

The graphics-light visual direction is **superseded**. Specifically:

- The brief's product exclusion of an effects-capable presentation is lifted;
  what remains excluded is a *requirement* for fully modeled 3D environments
  or characters. 2.5D techniques — perspective, lift, tilt, layering, shadows,
  illustrated environments, GPU-assisted rendering, and a designed effects
  vocabulary — are now in scope and in fact the target.
- The design-notes stance that mood is carried "by tokens and procedural
  geometry, never by surface weight", the "no 3D, no particle noise" effects
  restraint, and the concept-board rejection of painted/illustrated surfaces
  are superseded as *direction*. They remain accurate as the record of the
  shipped client.
- The blueprint's "flat-but-deliberate" surface commitment and its deferred
  texture-skin question are superseded: the 2.5D direction answers the
  question, and the new visual system (issue #469) replaces the flat surface
  as the target.
- The baseline image is the anchor for tone, depth, composition, and quality —
  **not** a pixel-perfect specification, and not a substitute for the
  interaction and scaling design work it deliberately leaves open.

### What is explicitly preserved

The pivot changes presentation, not architecture or product boundaries:

- **Server authority and zero client game logic.** `valid_actions[]` /
  `valid_commands` remain the only sources of interactivity; the client still
  computes no legality, cost, effect, or outcome.
- **The one-view reconstruction invariant.** The whole client UI — including
  any in-flight presentation sequence — must remain reconstructable from one
  `GameView` (or `LobbyView`/`SpectatorView`) plus the pending prompt.
  Animation is interpolation between authoritative states, never load-bearing
  client state.
- **Engine speed is presentation-independent.** Client animation and pacing
  must never slow the engine, headless games, or AI-only games.
- **Accessibility.** Reduced-motion support, non-color state channels,
  keyboard/touch/pointer equivalence, minimum hit targets, and readable text
  at supported sizes are unchanged requirements
  ([`../design/ui-requirements.md`](../design/ui-requirements.md) stays
  binding; its capabilities are presentation-independent).
- **Legal constraints.** No official card images, frames, symbols, branding,
  or Oracle text in the project's distribution; ADR 0024's player-side art
  pipeline is the only exception. Original (including AI-generated) art for
  RUNE's own presentation may ship. Nothing in this pivot copies another
  game's assets or interface.
- **Multiplayer and Commander as primary use cases**, not extensions of a
  duel layout.

### What this ADR does not decide

- **No rendering library or architecture is mandated.** Whether the DOM/canvas
  split (ADR 0003), the fixed-shell anatomy (ADR 0023), and the chrome styling
  layer (ADR 0019) survive, adapt, or are replaced is owned by the
  presentation-architecture spike (issue #467), which will record its outcome
  in its own ADR. Until then, ADRs 0003/0019/0023 remain in force for work on
  the existing client; their *flat-surface rationale* no longer constrains new
  design work.
- The visual system, motion grammar, budgets, and multiplayer layouts are
  designed under their own issues (#468–#470), not here.

## Consequences

- New design and implementation work is no longer required to satisfy the
  abandoned graphics-light direction, and reviewers should not cite the
  superseded statements against it.
- `docs/brief.md`, `docs/design/ui-design-notes.md`,
  `docs/design/ui-blueprint.md`, and `docs/roadmap.md` are updated in the same
  change to point at this ADR; the superseded text is kept, marked, as the
  record of the shipped client.
- The redesign is tracked as a roadmap milestone (issue #464 and its child
  issues), phased so that a working spike precedes any large rewrite.
- Asset-bearing presentation (illustrated environments, portraits, effects)
  introduces licensing, size, and pipeline obligations that did not exist
  under the procedural-only direction; issue #471 owns that groundwork before
  production assets land.
