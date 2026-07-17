/**
 * Canvas-anchored geometry helpers for the React DOM overlay (ADR 0003).
 *
 * Everything here positions a DOM element OVER the Pixi canvas from a runtime rect
 * the scene reports — coordinates that only exist at render time, so they cannot be
 * static CSS classes. The chrome *look* (surfaces, borders, typography, and all
 * interactive states) lives in the CSS-module styling layer (`chrome.module.css`,
 * ADR 0019); these helpers carry only geometry plus the few colors that must agree
 * with a shared token.
 *
 * Colors here read tokens, never hex literals: chrome tints come from the chrome
 * custom properties (`chrome/tokens.css`) via `var()`, while the selection/target
 * rings read the shared card token (`SURFACES`) so the DOM ring stays in lockstep
 * with the Pixi renderer that draws the same accent.
 */
import type { CSSProperties } from 'react';
import { SURFACES } from '../tokens';
import type { Rect } from './scene';

/** Minimum touch target per AGENTS.md (44px), applied to every affordance. */
const TOUCH = 44;

/**
 * The full-bleed tabletop shell root (issue #295): a positioned box the size of
 * the measured viewport, clipping its own overflow so no region can push the page
 * into a scroll. Every region below is absolutely positioned within it from a
 * layout rect (`table/layout.ts`), and fixed-position overlays (inspect, zone
 * browser, game over) escape it as before.
 */
export function shellBox(width: number, height: number): CSSProperties {
  return {
    position: 'relative',
    width,
    height,
    overflow: 'hidden',
  };
}

/**
 * Absolutely position a shell region from its {@link Rect} (issue #295). The
 * region's *look* (surface tier, tint, elevation) is a `chrome.module.css` class
 * spread on top; this helper carries only the runtime geometry, which — being
 * viewport-derived — cannot be a static class (ADR 0019 keeps geometry inline).
 */
export function regionBox(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: rect.w,
    height: rect.h,
    boxSizing: 'border-box',
  };
}

/**
 * The collapsed stack/activity rail on narrow geometry (issue #295): instead of
 * docking (which would eat board width), it floats a content-sized panel pinned to
 * the right edge at its badge anchor, overlaying the board. #299 redesigns this
 * into an on-demand expand from a badge; for now the panel floats so the stack
 * stays reachable without carving the battlefield.
 */
export function railFloat(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    top: rect.y,
    right: 8,
    maxWidth: 320,
    maxHeight: '60vh',
    overflow: 'auto',
    boxSizing: 'border-box',
  };
}

/**
 * The Pixi scene box inside the battlefield region: sized to exactly what the
 * scene reports. Its width never exceeds the battlefield region's width (the scene
 * wraps within that budget), so it never scrolls horizontally; a board taller than
 * the region scrolls vertically within the region instead (§Battlefield bands).
 */
export function sceneBox(width: number, height: number): CSSProperties {
  return {
    position: 'relative',
    width,
    height,
  };
}

export function overlay(width: number, height: number): CSSProperties {
  return {
    position: 'absolute',
    inset: 0,
    width,
    height,
    // The overlay itself passes pointer events through; only its buttons catch
    // them, so empty board space never intercepts clicks.
    pointerEvents: 'none',
  };
}

export function hotspot(rect: Rect, selected: boolean): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: Math.max(rect.w, TOUCH),
    height: Math.max(rect.h, TOUCH),
    minWidth: TOUCH,
    minHeight: TOUCH,
    padding: 0,
    background: 'transparent',
    border: selected ? `2px solid ${SURFACES.selection}` : '2px solid transparent',
    borderRadius: 10,
    cursor: 'pointer',
    pointerEvents: 'auto',
  };
}

/**
 * A target-pick hotspot for targeting mode: the same touch-sized hitbox as a
 * selection {@link hotspot}, ringed and faintly filled in the shared targeting
 * color so a legal target reads as pickable. Ineligible cards get no hotspot at
 * all (they are dimmed in the canvas), so only candidates catch a click.
 *
 * In a multi-select an already-chosen candidate is `chosen`: it fills more solidly
 * (in the shared selection accent) so a toggled pick reads as committed, while
 * unchosen candidates keep the lighter targeting fill.
 */
export function targetHotspot(rect: Rect, chosen = false): CSSProperties {
  return {
    ...hotspot(rect, false),
    border: `2px solid ${chosen ? SURFACES.selection : SURFACES.targeting}`,
    background: chosen ? 'var(--rune-selection-fill)' : 'var(--rune-targeting-fill)',
  };
}

export function entityActions(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y + rect.h + 4,
    display: 'flex',
    flexWrap: 'wrap',
    gap: 4,
    pointerEvents: 'auto',
    zIndex: 2,
  };
}

/**
 * The inspect handle drawn ON a card (issue #261): a small, touch-sized affordance
 * anchored at the card's top-right corner that opens the inspect popover. It is a
 * DISTINCT control from the select/target hotspot beneath it (its own testid and a
 * higher stacking context), so a card can be inspected without disturbing its
 * select/target interaction — the two coexist (a card stays both inspectable and
 * toggleable in targeting/multi-select).
 */
export function inspectHandle(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x + rect.w - TOUCH + 6,
    top: rect.y - 6,
    width: TOUCH,
    height: TOUCH,
    padding: 0,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 999,
    border: `1px solid ${SURFACES.selection}`,
    background: 'var(--rune-table-strong)',
    color: 'var(--rune-text)',
    fontSize: 15,
    fontWeight: 700,
    lineHeight: 1,
    cursor: 'pointer',
    pointerEvents: 'auto',
    zIndex: 3,
  };
}

/* ── Table geography geometry (issue #278) ────────────────────────────────────
 * The labeled, bounded per-player lanes + hand row are positioned from the scene's
 * band/hand rects. Boundaries are transparent-filled boxes (they never occlude a
 * card); the header strip lives in the reserved space above the cards. The visible
 * chrome (labels, pile buttons) is styled by `chrome.module.css`; these helpers own
 * only the rect placement. All colors read tokens.
 */

/** The geography overlay: same coordinate space as the canvas, non-interactive. */
export function geographyLayer(width: number, height: number): CSSProperties {
  return {
    position: 'absolute',
    inset: 0,
    width,
    height,
    // Chrome only; the zone-pile buttons opt back into pointer events via CSS.
    pointerEvents: 'none',
  };
}

/** A player's bounded battlefield lane. The local lane is ringed like its tile. */
export function bandRegion(rect: Rect, isLocal: boolean): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: rect.w,
    height: rect.h,
    boxSizing: 'border-box',
    border: `1px solid ${isLocal ? SURFACES.selection : 'var(--rune-border)'}`,
    borderRadius: 8,
    background: isLocal ? 'var(--rune-region-bg-local)' : 'var(--rune-region-bg)',
  };
}

/** The label + zone-pile strip pinned to the top of a band (or the hand row). */
export function regionHeader(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x + 12,
    top: rect.y + 6,
    width: rect.w - 24,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: 10,
  };
}

/** Faint centered prompt filling an empty band so the lane invites play. */
export function emptyBandHint(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: rect.w,
    height: rect.h,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: 'var(--rune-text-muted)',
    opacity: 0.45,
    fontSize: 13,
    fontStyle: 'italic',
  };
}
