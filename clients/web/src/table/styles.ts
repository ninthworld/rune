/**
 * Canvas-anchored geometry helpers for the React DOM layers (ADR 0003).
 *
 * Everything here positions a DOM element from a runtime rect the layout or the
 * scene reports — coordinates that only exist at render time, so they cannot be
 * static CSS classes. The chrome *look* (surfaces, borders, typography, and all
 * interactive states) lives in the CSS-module styling layer (`chrome.module.css`,
 * ADR 0019); these helpers carry only geometry plus the few colors that must agree
 * with a shared token.
 */
import type { CSSProperties } from 'react';
import { SURFACES } from '../tokens';
import type { Rect } from './scene';

/** Minimum touch target per AGENTS.md (44px), applied to every affordance. */
const TOUCH = 44;

/**
 * The fixed-shell root (ADR 0023): a positioned box the size of the measured
 * viewport, clipping its own overflow so no region can push the page into a
 * scroll. Every region is absolutely positioned within it from a layout rect
 * (`table/layout.ts`); fixed-position overlays (inspect, sheets, game over)
 * escape it.
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
 * Absolutely position a shell region from its {@link Rect}. The region's *look*
 * (surface tier, tint, elevation) is a `chrome.module.css` class spread on top;
 * this helper carries only the runtime geometry, which — being viewport-derived —
 * cannot be a static class (ADR 0019 keeps geometry inline).
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
 * The expanded phase strip's step panel: a floating overlay dropped BELOW the top
 * bar — one of the only layers permitted to cover the shell (blueprint
 * §Interaction model), always viewport-clamped (ui-requirements §Stack, priority,
 * and timers: the expansion "renders entirely within the viewport, never clipped
 * by an edge"), scrolling internally if it must.
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
 * The Pixi scene box inside the canvas region: sized to exactly what the scene
 * reports (the carved canvas area — panels never outgrow it, so nothing ever
 * scrolls).
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

/* ── Panel chrome geometry (ADR 0023) ─────────────────────────────────────────
 * The per-player panel boxes, headers, piles columns, and hints are positioned
 * from the scene's band rects. The visible chrome is styled by
 * `chrome.module.css`; these helpers own only the rect placement.
 */

/** The panel chrome layer: same coordinate space as the canvas, non-interactive
 * except where children opt back in. */
export function panelLayer(width: number, height: number): CSSProperties {
  return {
    position: 'absolute',
    inset: 0,
    width,
    height,
    pointerEvents: 'none',
  };
}

/** A player's bounded panel box (the accent styling is a chrome class). */
export function panelBox(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: rect.w,
    height: rect.h,
    boxSizing: 'border-box',
  };
}

/** The header strip pinned to the top of a panel. */
export function panelHeaderBox(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: rect.w,
    height: rect.h,
    boxSizing: 'border-box',
    display: 'flex',
    alignItems: 'center',
    gap: 8,
    // The chrome layer is pointer-events: none; targeting/focus opt back in.
    pointerEvents: 'auto',
  };
}

/**
 * The zone-pile column (§Zone piles): the panel-edge strip where the library /
 * graveyard / exile stack parks as table furniture. Geometry only; the pile look
 * is `chrome.module.css`.
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
    justifyContent: 'center',
    gap: 10,
    boxSizing: 'border-box',
    // The chrome layer is pointer-events: none; the piles opt back in.
    pointerEvents: 'auto',
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
    left: rect.x + 4,
    top: rect.y - 2,
    color: 'var(--rune-text-muted)',
    opacity: 0.6,
    fontSize: 10,
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
    pointerEvents: 'none',
  };
}

/** Faint centered prompt filling an empty panel so the lane invites play. */
export function emptyPanelHint(rect: Rect): CSSProperties {
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
    pointerEvents: 'none',
  };
}
