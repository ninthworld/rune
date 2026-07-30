# ADR 0003: Hybrid DOM/canvas rendering in the web client

- Status: superseded by ADR 0030 (Phase 4 complete, issue #504)
- Date: 2026-07-10

## Context

The battlefield needs GPU-accelerated rendering of 100+ objects; prompts, logs,
and action buttons need accessibility, text selection, and native input semantics.
Retrofitting accessibility onto an all-canvas UI is a rewrite.

## Decision

One full-bleed Pixi canvas renders battlefield, hand, and stack cards plus
targeting arrows and animations. Everything a user reads or clicks that is not a
card — prompt banners, action bar, player tiles/zone rail, log, zone browsers,
inspect — is React DOM floating above the canvas. DOM anchors to canvas objects
only via reported rects; the DOM never reaches into the Pixi scene. Both layers
are positioned by one layout() function and re-render from the same GameView.

## Consequences

Two card renderers exist (Pixi factory + HTML component); they must share one
token module (clients/web/src/tokens.ts). In exchange: screen-reader and keyboard
support come from the platform, and the canvas stays a pure performance surface.

## Superseded by ADR 0030

The 2.5D pivot (ADR 0030) inverted this split: cards render in **React DOM**, not
the Pixi canvas, and Pixi is reserved for the passive effects overlay. The
all-canvas card layer this ADR introduced — the `cardFactory` Pixi draw path, the
`sceneReconciler`/`BattlefieldCanvas`/`EntityOverlay` stack, the `layout()`
shell-carving function, and the legacy scene builder — was retired in stages
(#494/#498, and finally #504 once the read-only spectate mode moved onto the DOM
scene plane). The surviving DOM/canvas boundary is unchanged in spirit — DOM owns
everything readable or clickable, and it still anchors to scene geometry only via
reported rects — but there is now a **single** card renderer (the DOM `CardFace`),
so the two-renderer/one-token-module consequence above no longer applies. See ADR
0030 for the current architecture.
