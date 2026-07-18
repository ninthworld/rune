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

/** Clamp helper (min ≤ value ≤ max). */
function clampTo(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

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
 * The floating action tray (issue #298): pinned by its BOTTOM edge to the tray
 * region's lower edge so it grows upward as its content (decision controls) wraps,
 * clearing the hand below. Only geometry lives here; the tray's elevated look is a
 * `chrome.module.css` class. The outer box passes pointer events through (so empty
 * space above the hand never blocks a card); only the tray content catches them.
 */
export function trayBox(rect: Rect, viewportHeight: number): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    bottom: Math.max(0, viewportHeight - (rect.y + rect.h)),
    width: rect.w,
    boxSizing: 'border-box',
    display: 'flex',
    // Centered above the hand, visually tied to it — not a lone box drifting at
    // the dock's edge (§Tabletop shell: the tray floats above the hand).
    justifyContent: 'center',
    // Pass-through container; the tray content (its child) opts back in via CSS.
    pointerEvents: 'none',
  };
}

/**
 * The anchored prompt overlay (issue #298): a focused decision surface positioned
 * relative to the subjects a decision concerns, using the scene's REPORTED RECTS
 * (ADR 0003 — never by reaching into Pixi). The caller supplies the anchor point in
 * shell (viewport) coordinates — the horizontal center of the subject span and the
 * y just above (or below) it — plus the battlefield region the overlay must stay
 * within. `place` decides whether the surface grows upward from the anchor (subject
 * lower on the board) or downward (subject near the top), so it never runs off an
 * edge; a `translate` keeps it centered/edge-anchored without measuring its height.
 */
export function promptOverlayBox(
  anchor: { centerX: number; y: number; place: 'above' | 'below' },
  region: Rect,
): CSSProperties {
  const margin = 8;
  const centerX = clampTo(anchor.centerX, region.x + margin, region.x + region.w - margin);
  return {
    position: 'absolute',
    left: centerX,
    top: anchor.y,
    transform: anchor.place === 'above' ? 'translate(-50%, -100%)' : 'translate(-50%, 0)',
    width: 'max-content',
    maxWidth: Math.max(TOUCH, Math.min(460, region.w - margin * 2)),
    maxHeight: Math.max(TOUCH, region.h - margin * 2),
    overflowY: 'auto',
    boxSizing: 'border-box',
    zIndex: 6,
  };
}

/**
 * The expanded phase indicator's step panel: a floating overlay dropped BELOW the
 * compact bar, never rendered inside the fixed-height indicator strip (which
 * clipped it — ui-requirements §Stack, priority, and timers demands the expansion
 * "render entirely within the viewport, never clipped by an edge"). Fixed
 * positioning escapes the strip's overflow; the max sizes keep the panel inside the
 * viewport at every geometry, scrolling internally if it must. Geometry only — the
 * elevated look is a `chrome.module.css` class.
 */
export function indicatorStepsBox(indicatorHeight: number): CSSProperties {
  const top = indicatorHeight + 8;
  return {
    position: 'fixed',
    top,
    left: '50%',
    transform: 'translateX(-50%)',
    maxWidth: 'min(92vw, 760px)',
    maxHeight: `calc(100vh - ${top + 16}px)`,
    overflowY: 'auto',
    boxSizing: 'border-box',
    zIndex: 7,
  };
}

/**
 * The expanded stack/activity rail when the geometry is narrow (issue #299): rather
 * than docking (which would eat board width), the on-demand expanded panel floats a
 * content-sized surface pinned to the right edge at the rail's badge anchor,
 * overlaying the board so the stack stays reachable without carving the
 * battlefield. On wide geometry the rail docks instead (see {@link regionBox}).
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
 * The collapsed rail badge (issue #299): a single {@link TOUCH}-sized target pinned
 * to the top-right corner of the rail's region rect. On narrow geometry that rect
 * IS the 44px badge anchor (`layout.ts`), so the badge lands exactly on it; on wide
 * geometry (a user who manually collapsed the docked column) it tucks into the
 * column's top-right corner. Geometry only — the look is a `chrome.module.css` class.
 */
export function railBadgeBox(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x + rect.w - TOUCH,
    top: rect.y,
    width: TOUCH,
    height: TOUCH,
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
/**
 * A per-card **inspect surface** (issue #321): a transparent, chrome-less layer over
 * an otherwise non-interactive card (an opponent's permanent, an inert hand card),
 * carrying the inspect gestures — hover-dwell, long-press, right-click, and keyboard
 * focus + activate — without a permanently visible handle. It covers the card's rect
 * and is focusable for the keyboard/AT path, but paints nothing, so the board stays
 * quiet. Cards that already carry a select/target hotspot host the same gestures on
 * that hotspot instead, so no card ever stacks two interactive layers.
 */
export function inspectSurface(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: rect.w,
    height: rect.h,
    padding: 0,
    margin: 0,
    border: 'none',
    background: 'transparent',
    cursor: 'default',
    pointerEvents: 'auto',
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

/**
 * The zone-pile column (§Zone piles): the reserved right-edge strip of a band the
 * scene computed (`Band.pileRect`), where the library / graveyard / exile stack
 * parks as table furniture. Geometry only; the pile look is `chrome.module.css`.
 */
export function pileColumnBox(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: rect.w,
    height: rect.h,
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    gap: 10,
    boxSizing: 'border-box',
    // The geography layer is pointer-events: none; the piles opt back in.
    pointerEvents: 'auto',
  };
}

/**
 * A player's bounded battlefield lane, bordered and tinted in the controller's
 * identity accent (§Identity: the region answers "whose stuff"; cards never wear
 * the accent). The local lane reads slightly stronger.
 */
export function bandRegion(rect: Rect, isLocal: boolean, accent: string): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: rect.w,
    height: rect.h,
    boxSizing: 'border-box',
    border: `1px solid ${accent}${isLocal ? '8C' : '4D'}`,
    borderRadius: 8,
    background: `${accent}${isLocal ? '14' : '0A'}`,
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

/**
 * The single honest row label — only the lands row earns one (issue #318). Placed
 * at the row's left edge, faint and small, so it names the sorting convention
 * without reading as a rule-implying zone header.
 */
export function rowLabel(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x + 12,
    top: rect.y - 2,
    color: 'var(--rune-text-muted)',
    opacity: 0.6,
    fontSize: 10,
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
    pointerEvents: 'none',
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
