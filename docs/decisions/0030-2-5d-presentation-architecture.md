# ADR 0030: 2.5D presentation architecture — DOM scene, WebGL effects

- Status: accepted
- Date: 2026-07-24

## Context

ADR 0029 pivoted the client to a polished 2.5D presentation and deferred the
architecture question to a spike (issue #467). The spike ran: an architecture
audit of the shipped client, plus a dependency-free prototype
([`prototypes/ui-2-5d-spike-v1.html`](../../prototypes/ui-2-5d-spike-v1.html))
reproducing the baseline composition's load-bearing elements, measured in
headless Chromium. The evidence and per-question answers are recorded in
[`../design/spike-2-5d-findings.md`](../design/spike-2-5d-findings.md).

The load-bearing facts: the pure scene layer (`buildTableScene`:
GameView → logical rects) and DOM-rect hit-testing are presentation-agnostic;
the 2D card-pixel pipeline is concentrated in three files (card factory,
scene reconciler, canvas mount); the client uses no 3D or filter capability
anywhere today; 240 perspective-transformed DOM cards idle and mass-tween at
~55 fps *under software rendering*; full-viewport 2D-canvas effect repaints —
not the 2.5D scene — are the measured bottleneck; and scene rebuilds from
serialized state cost single-digit milliseconds.

## Decision

The match presentation is a four-layer architecture, back to front:

1. **Environment** — illustrated backdrop layers behind the battlefield
   (static art per the asset pipeline, issue #471; restrained parallax/motion
   under the quality ladder). Outside the interactive scene.
2. **Scene plane (DOM, CSS 3D)** — one perspective-transformed plane carries
   every spatial object: player regions, zone piles, portraits, and **cards as
   DOM elements** rendered by **one card component** for battlefield, hand,
   browsers, and inspect. Lift, tilt, tap, shadows, and travel are
   transform/opacity-only animation. The plane container's single transform is
   the camera: focus and staging changes are scene-geometry changes, animated
   by the reconciler like any other diff.
3. **Effects overlay (WebGL, Pixi — retained and refocused)** — targeting and
   attack paths, bursts, glows, and streams. Passive (`pointer-events: none`,
   never a hit target), **render-on-demand with zero idle cost**, pooled
   sprites, first to degrade under the quality ladder.
4. **Screen-space chrome (React DOM)** — hand fan, prompt strip, action dock,
   rails, sheets, and overlays, keeping ADR 0023's interaction commitments:
   one action home, viewport-clamped overlays, no region ever covered by
   another except designated overlay layers.

Carried invariants, unchanged: the scene derives purely from the latest view
(one-view reconstruction; reconnect = render with animation suppressed);
reconciliation is by entity id with fresh-mount equivalence; every animation
interpolates between two authoritative scenes, is interruptible, and never
gates input (destination rects are addressable immediately);
`prefers-reduced-motion` snaps with no layout or state difference; the client
computes no legality.

Two performance rules are binding on the implementation, both measured in the
spike: **the effects layer idles at zero cost**, and **scene updates are
incremental** (full rebuilds are reserved for reconnect/fast-forward).

### Disposition of prior presentation ADRs

- **ADR 0003 (DOM/canvas split): superseded — the split inverts, the
  principle survives.** DOM now renders what you read *or click including
  cards*; the GPU canvas renders what glows and moves in volume. The
  two-card-renderer consequence ADR 0003 accepted (Pixi factory + HTML
  component) dissolves into one DOM card component. The rule that DOM anchors
  to scene geometry via reported rects survives — strengthened, since the
  hotspot and the pixels are now the same element, which also removes the
  projection-desync a perspective Pixi card would have forced on the flat
  hit-test rects.
- **ADR 0019 (chrome styling layer): stands.** CSS custom properties + CSS
  modules extend naturally to the scene layer; the card/chrome token split
  survives until the visual system (issue #469) revisits tokens.
- **ADR 0023 (fixed shell, one action home): interaction commitments stand;
  flat composition superseded.** One action home, the prompt strip, clamped
  overlays, and regions-never-overlap-by-construction carry into the staged
  battlefield; the carved flat-panel look and the "flat-but-deliberate"
  surface were already superseded by ADR 0029.
- **ADR 0011 (browser test strategy): unaffected.** Pure scene tests keep
  covering geometry and staging; DOM cards are *more* jsdom-testable than
  Pixi trees; the effects layer keeps the structural-snapshot approach.

## Consequences

- The replacement surface in `clients/web` is bounded and known:
  `card/cardFactory.ts` (→ DOM card component), `table/sceneReconciler.ts`
  (retargeted at the DOM scene, same invariants), and
  `table/BattlefieldCanvas.tsx` (→ effects-layer mount). The scene package,
  interaction hooks, store, tokens split, glyphs, and test harness carry over.
- Phase 1 of the redesign (issue #464) can be split into implementation
  issues against this architecture; the visual system (#469) and layout
  designs (#470) style and stage it without re-deciding it.
- Budgets (#468) inherit a measured software floor and two hard rules; the
  spike's harness (`window.__spike`, the measurement scenarios in the
  findings doc) should be re-run on representative real hardware to set the
  per-device numbers.
- Risks accepted: DOM node count is the scene's scaling ceiling (measured
  fine at 245 cards under software rendering, but real low-end mobile
  validation is owed by #468), and Pixi remains a dependency whose role
  shrinks to effects — if the effects vocabulary stays small, a leaner WebGL
  layer may replace it later without touching the scene.
