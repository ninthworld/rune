# 2.5D presentation architecture spike — findings

Evidence for the presentation-architecture decision of the 2.5D pivot
(issue #467, under [ADR 0029](../decisions/0029-2-5d-presentation-direction.md) /
issue #464). The selected architecture is recorded in
[ADR 0030](../decisions/0030-2-5d-presentation-architecture.md); this document is
the spike's record: what was built, what was measured, and what each of the
issue's questions resolved to.

## What was built

[`prototypes/ui-2-5d-spike-v1.html`](../../prototypes/ui-2-5d-spike-v1.html) — a
dependency-free, reference-only prototype (never imported into production)
reproducing the baseline composition's load-bearing elements from one plain
state object:

- a perspective battlefield plane (CSS 3D) over an illustrated gradient
  environment — focused opponent expanded across the top, two peripheral
  opponents staged at the sides, the receiver anchored at the bottom
  (4-player Commander composition);
- cards as DOM elements lying *on* the tilted plane (foreshortening for free)
  with lift (`translateZ`), tap rotation, layered shadows, and hover tactility;
- a curved, tactile hand fan in screen space;
- travel animations (library→hand, hand→battlefield) as FLIP ghosts between
  two pure renders of the state object;
- a passive 2D-canvas effects overlay (targeting/attack bezier arrows with
  animated dashes, particle bursts) — `pointer-events: none`, never a hit
  target;
- a reconnect/fast-forward demo (serialize state → rebuild scene with
  animations suppressed), a reduced-motion toggle, a ~120/240-permanent stress
  mode, an FPS meter, and a `window.__spike` harness API;
- a `?flat=1` control mode that renders the identical scene without
  perspective, isolating what the 3D plane itself costs.

Measurements ran in the pre-installed headless Chromium via Playwright at
1440×900. **Headless Chromium rasterizes with SwiftShader (software rendering,
no GPU)**, so every number below is a conservative floor: real hardware
composites CSS transforms and canvas layers on the GPU. Per-device validation
on real hardware is budget work owned by issue #468.

## Measurements (software-rendering floor)

Perspective mode (the target composition):

| Scenario | Cards | fps | p95 frame |
| --- | ---: | ---: | ---: |
| Baseline board, idle | 35 | 59.7 | 16.8 ms |
| Stress board, idle | 125 | 56.2 | 16.7 ms |
| Double stress board, idle | 245 | 54.8 | 16.8 ms |
| Mass untap tween (every tapped card at once) | 125 | 55.5 | 16.7 ms |
| Continuous targeting arrows + particle bursts | 125 | 9.1 | 200 ms |
| 12 back-to-back draw travels (full re-render each) | 137 | 6.7 | 200 ms |

`?flat=1` control (same scene, no perspective): idle and tween rows are
within noise of the perspective rows (55–59 fps), and the two effect-heavy
rows roughly double (20.5 / 15.5 fps) but stay far below budget. Reconnect
rebuild from serialized state at stress scale: **1.4–6.5 ms**. JS heap after
all scenarios: **~1.4 MB**.

Three conclusions, in order of importance:

1. **The 2.5D scene itself is cheap.** 240 perspective-transformed DOM cards
   idle at ~55 fps and mass-tween at ~55 fps *in software rendering*. The
   perspective plane adds no measurable cost over the flat control in the
   compositor-friendly paths (transform/opacity-only animation). The
   composition is not the risk.
2. **Full-viewport 2D-canvas repaints are the bottleneck, not the scene.**
   Continuous arrow/particle repaints collapse the frame rate in both modes.
   Two consequences: the effects overlay must **idle at zero cost**
   (render-on-demand — adding this to the prototype took idle from 17 fps to
   60 fps by itself), and sustained high-density effects belong on a **WebGL
   layer** (the retained Pixi surface), not a full-viewport 2D canvas.
3. **Incremental reconciliation is mandatory.** The travel-storm row rebuilds
   the entire DOM scene on every state change (the prototype's naive
   `render()`); that, not the FLIP ghosts, is its cost. The shipped client's
   reconcile-by-entity-id pattern (`sceneReconciler.ts`) must carry over to
   whatever renders cards. Full rebuilds stay reserved for reconnect, where
   1.4–6.5 ms is negligible.

## The issue's questions, answered

**Which portions of the existing client remain, which are replaced?**

Survives unchanged (presentation-agnostic):

- the pure scene layer — `buildTableScene` and the `scene/` package
  (GameView → logical rects + `CardDisplayData`); it *grows* staging data
  (plane placement, focus, camera) and stays pure, headless, and testable;
- DOM-rect hit-testing and the interaction layer (`EntityOverlay`, the
  interaction/keyboard/focus hooks) — interactivity is already DOM, keyed on
  scene rects, and never touches the renderer;
- the token split (card tokens in `src/tokens.ts`, chrome tokens per
  ADR 0019), the glyph language, the store, protocol handling, and the whole
  pure-scene test harness;
- the shell's interaction commitments (one action home, prompts strip,
  overlays clamped — ADR 0023 §1–2 as *interaction* rules).

Replaced (the 2D card-pixel pipeline, concentrated in three files):

- `card/cardFactory.ts` — Pixi vector card faces → one DOM card component
  (which also dissolves ADR 0003's accepted two-renderer consequence);
- `table/sceneReconciler.ts` — retargeted from the Pixi tree to the DOM scene,
  keeping its invariants (reconcile-by-id, fresh-mount equivalence,
  reduced-motion snap, input never gated);
- `table/BattlefieldCanvas.tsx` — from "the card renderer" to the mount of the
  refocused effects layer.

**Can the DOM/canvas split support the target?** Yes — *inverted*. Today Pixi
draws card pixels and DOM draws chrome and hit targets. The audit found no 3D,
perspective, or filter usage anywhere (the visual side is greenfield), and the
flat DOM hotspot rects would desync from any perspective-transformed Pixi
card. Moving card pixels *into* the DOM elements that already own interaction
makes the hotspot and the pixels the same transformed element — no projection
math, native accessibility — and the measurements show the DOM compositor
carries the scene. The canvas side is retained and refocused as a WebGL
effects layer (Pixi is already a dependency). ADR 0003's principle survives
restated: what you read or click is DOM — now including cards; what glows,
bursts, or streams is the GPU canvas.

**How do cards, overlays, effects, targeting paths, camera/focus transitions,
and environmental layers compose?** As four layers, back to front:
environment (illustrated backdrop, outside the plane) → the scene plane
(regions, piles, portraits, cards; one perspective transform on the plane
container is the "camera") → the passive effects canvas → screen-space chrome
(hand fan, prompts, dock, rails). Focus changes re-stage regions on the plane
(a scene-geometry change, animatable by the same reconciler); targeting paths
anchor to scene rects exactly as combat links do today.

**How does deterministic engine state translate into interruptible
animation?** Unchanged in principle from the shipped view-diff layer, now
proven with travel: every animation is an interpolation between two
authoritative renders (FLIP ghosts for zone travel, transform tweens for
tap/reflow), the destination element exists and is addressable immediately,
and a newer view retargets or discards in-flight interpolations. Nothing
waits on an animation to accept input.

**Reconnect and rapid state updates?** The scene is a pure function of state:
the prototype serializes its whole state and rebuilds in single-digit
milliseconds with animations suppressed — the fast-forward path is "render the
latest view with `animate: false`", exactly as today.

**Quality levels, reduced motion, low power, touch?** Reduced motion
collapses one duration token, with zero layout or state difference (same
contract as the shipped client). The quality ladder degrades in effect
density → shadows → environmental animation order; the DOM scene is the floor
and is always full-fidelity, because it *is* the game state. Hit targets are
real DOM elements at scene-rect sizes, so the ≥44 px rule carries over
unchanged.

**Performance and memory budgets?** This spike supplies the software floor
above and the two hard rules (zero-cost idle, incremental reconcile);
concrete per-device budgets (desktop/mobile/Android envelope, memory, load)
are issue #468's deliverable and should re-run this prototype's harness on
real hardware.

## Out of scope for this spike

Visual quality (the prototype's placeholder look is not the visual system —
issue #469), final layout geometry for 2–6 players and mobile (#470),
production asset formats (#471), and any production code change. The
prototype is evidence, frozen as a reference.
