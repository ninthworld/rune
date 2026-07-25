/**
 * The match shell's **viewport, safe-area, and stacking contract** (issue #528).
 *
 * This module is the single place the fixed shell's geometry and its layer order
 * are decided. It exists because the shipped composition could occlude the very
 * things a decision needs: hand cards were positioned with a negative offset into
 * a clipped band, the compact composition let the controls column paint over the
 * hand, and the decision sheet sat *below* the top bar in the stacking order, so
 * a forced mulligan prompt could be covered by chrome — a soft-lock.
 *
 * The values here are consumed two ways, and both are load-bearing:
 *
 * 1. {@link shellStyleVars} emits them as `--shell-*` custom properties on the
 *    shell root, which `live-match.module.css` reads for every track size and
 *    every hand offset. The stylesheet holds no dimensional literal of its own.
 * 2. {@link LAYER} mirrors the `--rune-z-*` ladder declared in
 *    `src/chrome/tokens.css`. `shellLayout.test.ts` reads that stylesheet and
 *    fails if the two drift, so the ladder cannot be edited in one place only.
 *
 * ## The three invariants
 *
 * **I1 — containment.** Every shell region is inside the safe viewport, and no
 * two regions overlap. {@link shellBands} reports them as rects so this is
 * provable rather than asserted by eye. A card, an identity crest, an action, or
 * the battlefield may never be covered by a fixed region.
 *
 * **I2 — the hand fits its band.** The hand band is tall enough for a full
 * `hand`-tier card *plus its largest lift* ({@link SHELL.handLiftMax}, the
 * forced-decision selected state) and wide enough that the fan's outermost cards
 * are inset by half a card. Nothing in the hand is ever drawn outside the band
 * that clips it. See {@link handFanFraction} / {@link handFanSpacing}.
 *
 * **I3 — the ladder yields to decisions.** A pending server decision outranks
 * *every* fixed shell region, including the top bar. Only layers the player
 * explicitly invoked and can dismiss without answering (menus, inspect, zone
 * browser, settings) may sit above it, plus the terminal game-over panel and the
 * pointer-transparent toast layer.
 *
 * Later work moves the regions around — #531 recomposes the battlefield, #533
 * rebuilds the hand, #534 removes the dashboard panels and relocates the action
 * home. They should keep these three invariants and re-point at these constants;
 * the geometry is deliberately expressed as *named bands*, not as a fixed grid,
 * so a recomposition changes {@link shellBands} rather than scattering numbers.
 */
import type { CSSProperties } from 'react';
import { TIER } from '../../tokens';
import type { Viewport } from '../hooks/useViewport';

/**
 * The stacking ladder — the mirror of the `--rune-z-*` tokens in
 * `src/chrome/tokens.css`. Every `z-index` in the match's stylesheets reads one
 * of those tokens; none may be a bare number.
 *
 * Read bottom-to-top. The rule that decides the order: **a layer may only be
 * covered by a layer the player explicitly invoked and can dismiss without
 * answering it.** That is what puts `decision` above `shellTop`: a mulligan
 * prompt the server is waiting on is not dismissible, so no permanent chrome may
 * ever be painted over it.
 *
 * Two caveats that matter when adding a layer:
 *
 * - A region that carries a `z-index` **creates a stacking context** (grid items
 *   do so even at `position: static`). Everything inside `.top`, `.scene`,
 *   `.rail`, and `.bottom` is therefore trapped at that region's rung — a
 *   `position: fixed` popover rendered inside the top bar cannot climb above
 *   `shellTop`. Anything that must outrank a shell region has to be a sibling of
 *   those regions, as the decision sheet and the overlays are.
 *
 *   Known consequence, accepted here: the game-menu drawer and the phase-strip
 *   expansion are rendered *inside* the top bar, so they are pinned at
 *   `shellTop` and a decision sheet opened at the same time sits above them.
 *   That is the right trade while chrome is where ADR 0023 put it — a decision
 *   the server is waiting on must never be covered, and the menu is not a
 *   required choice. When #534 relocates the chrome, lift those two popovers to
 *   siblings of the regions and give them the `popover` rung they already ask
 *   for, and the collision disappears.
 * - The plane's own layers (`live-plane.module.css`) are scoped by
 *   `isolation: isolate` on the plane host and are intentionally *not* on this
 *   ladder: they order objects within the scene only.
 */
export const LAYER = {
  /** The battlefield plane and everything staged on it. */
  scene: 1,
  /** Fixed shell regions: the rail and the bottom shell. */
  shell: 10,
  /** The presented hand during a forced decision — raised within its own band. */
  shellRaised: 12,
  /** The top status bar: the highest *permanent* region. */
  shellTop: 20,
  /**
   * A pending decision (the decision sheet). Above every fixed region — a
   * forced mulligan/keep/bottoming prompt is what the server is waiting on and
   * cannot be dismissed, so chrome yields to it.
   */
  decision: 30,
  /** The drag ghost: pointer-transparent, follows the pointer over the shell. */
  drag: 40,
  /** Player-invoked, dismissible popovers: menus, the transient inspect preview. */
  popover: 50,
  /** Player-invoked overlays: zone browser, inspect, settings, help, game over. */
  overlay: 60,
  /** Non-blocking notices. Always `pointer-events: none`. */
  toast: 70,
} as const;

export type LayerName = keyof typeof LAYER;

/**
 * Shell dimensions, in CSS px. Every one of these reaches the stylesheet through
 * {@link shellStyleVars}; `live-match.module.css` declares no size of its own.
 */
export const SHELL = {
  /**
   * The composition breakpoint, matching `@media (max-width: 899px)` in
   * `live-match.module.css` and the `compact` prop `LiveMatchTable` passes down.
   * All three must move together.
   */
  compactBreakpoint: 900,
  /** The interactive-target floor (presentation-budgets §Accessibility). */
  minHit: 44,
  /** Top status-bar height, per composition. */
  topH: 56,
  topHCompact: 54,
  /** The right rail's width (full composition only; compact collapses it). */
  railW: 248,
  /** The receiver identity column's width, per composition. */
  identityW: 220,
  identityWCompact: 136,
  /** The decisions column: prompt strip + action dock (full composition). */
  decisionsMinW: 252,
  decisionsMaxW: 330,
  /** The hand band's own minimum width inside the full composition. */
  handMinW: 220,
  /** The hand size the band is sized to keep at the 44 px floor — an opening
   * hand. Larger hands are the layout model's fan-paging case (#533). */
  openingHand: 7,
  /** The compact composition's controls row (identity + decisions, side by side). */
  controlsHCompact: 112,
  /** A `hand`-tier card's footprint — the same token the DOM card renderer uses. */
  handCardW: TIER.hand.w,
  handCardH: TIER.hand.h,
  /** Gap between the hand band's bottom edge and a resting card's bottom. */
  handFloor: 8,
  /** Breathing room above the tallest lifted card, inside the band. */
  handHeadroom: 6,
  /** Lift applied to every card while the view forces the opening-hand decision. */
  handLift: 14,
  /** Lift of a selected card in the ordinary hand. */
  handLiftSelected: 20,
  /**
   * The largest lift any card can take (forced decision + selected). The hand
   * band is sized from this: a transform does not change layout, so the band has
   * to reserve the swept box or `overflow: hidden` clips the lifted card.
   */
  handLiftMax: 34,
  /** Horizontal clearance between the outermost card's edge and the band edge. */
  handGutter: 4,
  /**
   * The plane's own minimum staging height (`LivePlane` clamps to it). The scene
   * band must stay at or above this, or the plane is staged taller than the box
   * that clips it and the receiver's band is cut off.
   */
  sceneMinH: 360,
} as const;

/** A plain rect in shell coordinates (origin at the safe viewport's top-left). */
export interface ShellRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The named, non-overlapping regions of the fixed shell. */
export interface ShellBands {
  compact: boolean;
  /** The safe viewport the shell lays out inside (browser insets removed). */
  viewport: ShellRect;
  top: ShellRect;
  scene: ShellRect;
  /** Absent on the compact composition, where the rail collapses to sheets. */
  rail?: ShellRect;
  bottom: ShellRect;
  identity: ShellRect;
  hand: ShellRect;
  decisions: ShellRect;
}

/** Whether a viewport takes the compact composition. */
export function isCompactShell(viewport: Pick<Viewport, 'width'>): boolean {
  return viewport.width < SHELL.compactBreakpoint;
}

/**
 * The hand band's height: floor + card + the largest lift + headroom. Derived,
 * never a literal — raising {@link SHELL.handLiftMax} or moving to a different
 * card tier grows the band instead of clipping the cards (invariant I2).
 */
export function handBandHeight(): number {
  return SHELL.handFloor + SHELL.handLiftMax + SHELL.handCardH + SHELL.handHeadroom;
}

/** The bottom shell's height for a composition. */
export function bottomShellHeight(compact: boolean): number {
  return compact ? SHELL.controlsHCompact + handBandHeight() : handBandHeight();
}

/**
 * Resolve the full composition's bottom-shell columns exactly as CSS grid does
 * for `identityW / minmax(handMinW, 1fr) / minmax(decisionsMinW, decisionsMaxW)`:
 * the decisions track grows to its limit only with width the hand does not need.
 */
function fullBottomColumns(width: number): { identity: number; hand: number; decisions: number } {
  const free = Math.max(0, width - SHELL.identityW);
  // The decisions column yields to the hand's accessibility floor before it
  // takes its maximum width: an opening hand must keep 44 px of every card
  // exposed (presentation-budgets §Accessibility), and the `hand`-tier card is
  // now the 116 px portrait of card-representation §8.1. Above ~1280 the
  // subtraction is slack and the column still reaches `decisionsMaxW`.
  const decisions = Math.min(
    SHELL.decisionsMaxW,
    Math.max(SHELL.decisionsMinW, free - handBandWidthFor(SHELL.openingHand)),
  );
  return {
    identity: SHELL.identityW,
    hand: Math.max(SHELL.handMinW, free - decisions),
    decisions,
  };
}

/**
 * The shell's regions for a viewport — the TS mirror of the grid
 * `live-match.module.css` builds from the same constants.
 *
 * `inset` is the browser's safe-area inset (notch, home indicator, URL bar); the
 * shell lays out inside it, which is why no region is ever under system chrome.
 */
export function shellBands(
  viewport: Pick<Viewport, 'width' | 'height'>,
  inset: { top?: number; right?: number; bottom?: number; left?: number } = {},
): ShellBands {
  const left = inset.left ?? 0;
  const top = inset.top ?? 0;
  const w = Math.max(0, viewport.width - left - (inset.right ?? 0));
  const h = Math.max(0, viewport.height - top - (inset.bottom ?? 0));
  const compact = isCompactShell(viewport);
  const topH = compact ? SHELL.topHCompact : SHELL.topH;
  const bottomH = bottomShellHeight(compact);
  const railW = compact ? 0 : SHELL.railW;
  const sceneW = w - railW;
  const sceneH = Math.max(0, h - topH - bottomH);
  const bottomY = top + topH + sceneH;
  const handH = handBandHeight();

  const bands: ShellBands = {
    compact,
    viewport: { x: left, y: top, w, h },
    top: { x: left, y: top, w, h: topH },
    scene: { x: left, y: top + topH, w: sceneW, h: sceneH },
    bottom: { x: left, y: bottomY, w: sceneW, h: bottomH },
    // Filled in per composition below.
    identity: { x: left, y: bottomY, w: 0, h: 0 },
    hand: { x: left, y: bottomY, w: 0, h: 0 },
    decisions: { x: left, y: bottomY, w: 0, h: 0 },
  };

  if (compact) {
    // Two rows: a controls row (identity beside the decisions), then the hand
    // band across the full width. The hand never shares a cell with the
    // decisions column — the shipped composition overlapped them, and the
    // opaque decisions panel covered most of the fan on phone portrait.
    const controlsH = SHELL.controlsHCompact;
    const identityW = Math.min(SHELL.identityWCompact, sceneW);
    bands.identity = { x: left, y: bottomY, w: identityW, h: controlsH };
    bands.decisions = {
      x: left + identityW,
      y: bottomY,
      w: Math.max(0, sceneW - identityW),
      h: controlsH,
    };
    bands.hand = { x: left, y: bottomY + controlsH, w: sceneW, h: handH };
    return bands;
  }

  const cols = fullBottomColumns(sceneW);
  bands.rail = { x: left + sceneW, y: top + topH, w: railW, h: h - topH };
  bands.identity = { x: left, y: bottomY, w: cols.identity, h: bottomH };
  bands.hand = { x: left + cols.identity, y: bottomY, w: cols.hand, h: handH };
  bands.decisions = {
    x: left + cols.identity + cols.hand,
    y: bottomY,
    w: cols.decisions,
    h: bottomH,
  };
  return bands;
}

/** Whether two rects share any area (touching edges do not count). */
export function rectsOverlap(a: ShellRect, b: ShellRect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

/** Whether `inner` lies entirely inside `outer`. */
export function rectContains(outer: ShellRect, inner: ShellRect): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.w <= outer.x + outer.w &&
    inner.y + inner.h <= outer.y + outer.h
  );
}

/**
 * A hand card's position along the fan, as a fraction in `[0, 1]` of the band's
 * *usable* span. The stylesheet turns it into a `left` with
 * `calc(inset + (100% - 2 * inset) * t)`, where the inset is half a card plus the
 * gutter — so the outermost cards land exactly inside the band's edges and
 * nothing is clipped, at any band width (invariant I2).
 *
 * The shipped rule was a raw `10%…90%` of the band, which put half of the first
 * and last card outside the clipped band on every viewport whose hand band was
 * narrower than ~660 px — that is, on the tablet floor and phone portrait.
 */
export function handFanFraction(index: number, count: number): number {
  if (count <= 1) return 0.5;
  return index / (count - 1);
}

/** The usable fan span for a band: the width minus one card and both gutters. */
export function handFanSpan(bandWidth: number): number {
  return Math.max(0, bandWidth - SHELL.handCardW - 2 * SHELL.handGutter);
}

/**
 * The band width a `count`-card fan needs to keep {@link SHELL.minHit} of every
 * card exposed — the inverse of {@link handFanSpacing}. The bottom row's column
 * split reads this so a wider `hand`-tier card widens the band it is fanned in,
 * rather than silently eating into each card's exposed sliver.
 */
export function handBandWidthFor(count: number): number {
  return SHELL.handCardW + 2 * SHELL.handGutter + Math.max(0, count - 1) * SHELL.minHit;
}

/**
 * The exposed width of each card in a fan of `count` cards in a band of
 * `bandWidth`, i.e. the distance between neighboring card origins.
 *
 * The 44 px floor holds for an ordinary opening hand at every supported viewport.
 * It does **not** hold for the large hands the layout model routes to fan paging
 * (`docs/design/layout-model.md` §Stress dispositions) — paging belongs to the
 * hand rebuild (#533), and this function is what that work should gate on.
 */
export function handFanSpacing(count: number, bandWidth: number): number {
  if (count <= 1) return SHELL.handCardW;
  return handFanSpan(bandWidth) / (count - 1);
}

/** The card's horizontal extent within the band, for a fan fraction `t`. */
export function handCardBounds(t: number, bandWidth: number): { left: number; right: number } {
  const inset = SHELL.handCardW / 2 + SHELL.handGutter;
  const center = inset + (bandWidth - 2 * inset) * t;
  return { left: center - SHELL.handCardW / 2, right: center + SHELL.handCardW / 2 };
}

/**
 * The `--shell-*` custom properties the stylesheet lays out from. Applied to the
 * shell root, so every region — and every overlay rendered inside it, including
 * the decision sheet — inherits the same geometry contract.
 *
 * `--shell-safe-*` carry the browser's own insets through `env()`, with a 0
 * fallback: the shell is inset by them, so no region can ever be hidden under a
 * notch, a home indicator, or a collapsing mobile URL bar.
 */
export function shellStyleVars(viewport: Pick<Viewport, 'width'>): CSSProperties {
  const compact = isCompactShell(viewport);
  return {
    '--shell-top-h': `${compact ? SHELL.topHCompact : SHELL.topH}px`,
    '--shell-rail-w': `${SHELL.railW}px`,
    '--shell-bottom-h': `${bottomShellHeight(compact)}px`,
    '--shell-controls-h': `${SHELL.controlsHCompact}px`,
    '--shell-identity-w': `${compact ? SHELL.identityWCompact : SHELL.identityW}px`,
    '--shell-decisions-min-w': `${SHELL.decisionsMinW}px`,
    '--shell-decisions-max-w': `${SHELL.decisionsMaxW}px`,
    '--shell-hand-min-w': `${SHELL.handMinW}px`,
    '--shell-hand-h': `${handBandHeight()}px`,
    '--shell-hand-card-w': `${SHELL.handCardW}px`,
    '--shell-hand-card-h': `${SHELL.handCardH}px`,
    '--shell-hand-floor': `${SHELL.handFloor}px`,
    '--shell-hand-gutter': `${SHELL.handGutter}px`,
    '--shell-hand-lift': `${SHELL.handLift}px`,
    '--shell-hand-lift-selected': `${SHELL.handLiftSelected}px`,
    '--shell-hand-lift-max': `${SHELL.handLiftMax}px`,
  } as CSSProperties;
}
