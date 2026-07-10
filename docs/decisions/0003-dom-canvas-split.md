# ADR 0003: Hybrid DOM/canvas rendering in the web client

- Status: accepted
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
