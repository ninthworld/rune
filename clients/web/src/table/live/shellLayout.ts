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
 * **I1 — containment.** Every chrome region is inside the safe viewport, no two
 * chrome regions overlap each other, and none of them overlaps the **staging
 * box** the plane carves its slots inside. {@link shellBands} reports them all
 * as rects so this is provable rather than asserted by eye.
 *
 * Under ADR 0032 the scene spans the whole viewport and chrome floats over it,
 * so "no region overlaps another" can no longer mean "no rects intersect" — the
 * hand and the cluster necessarily sit over the scene. What it means now is
 * that no chrome region may cover a *staged object*: a card, a crest, an
 * action, or a path endpoint. That is exactly what the staging box enforces,
 * which is why it is a band here and an input to `stagePlane` there.
 *
 * **I2 — the hand fits its band.** The hand band is tall enough for a full
 * `hand`-tier card *plus its largest lift* ({@link SHELL.handLiftMax}, the
 * forced-decision selected state) and wide enough that the fan's outermost cards
 * are inset by half a card. Nothing in the hand is ever drawn outside the band
 * that clips it. See {@link handFanFraction} / {@link handFanSpacing}.
 *
 * **I3 — the ladder yields to decisions.** A pending server decision outranks
 * *every* chrome region. Only layers the player
 * explicitly invoked and can dismiss without answering (menus, inspect, zone
 * browser, settings) may sit above it, plus the terminal game-over panel and the
 * pointer-transparent toast layer.
 *
 * The geometry is deliberately expressed as *named bands* rather than a fixed
 * grid, which is what let #531, #533, and #534 recompose the shell by changing
 * {@link shellBands} instead of scattering numbers. #534 was the largest of
 * those: it deleted the permanent `top`, `rail`, `identity`, and `decisions`
 * bands outright (ADR 0032) and replaced them with the scene, two floating
 * chrome regions, and the staging box that keeps them off the board.
 *
 * ## What #533 left for #534
 *
 * The hand rebuild consumes this module and publishes three things the
 * contextual-controls work needs, all of them already exported:
 *
 * - **Band geometry.** `shellBands(viewport).hand` is the fan's span; the fan
 *   itself is planned by `table/handFan.ts`'s {@link localFanPlan}, which is
 *   pure and viewport-free. Moving the hand band is a change to
 *   {@link shellBands} alone — the fan re-plans against whatever width it gets,
 *   down to the 44 px floor, and pages below it.
 * - **Lift extents.** {@link SHELL.handLift} / {@link SHELL.handLiftSelected} /
 *   {@link SHELL.handLiftMax} bound every transform a hand card takes, and
 *   {@link handBandHeight} is derived from the last of them. A lifted card can
 *   never exceed `handLiftMax` above its rest position, so #534 can compute the
 *   band the fan actually sweeps without measuring the DOM.
 * - **The decision-clearance rule.** Invariant I3, unchanged: a pending decision
 *   outranks every fixed region. The fan raises to {@link LAYER.shellRaised}
 *   during a forced decision and never above it, so a contextual plaque on the
 *   `decision` rung is always free to cover chrome and never the cards it is
 *   asking about.
 */
import type { CSSProperties } from 'react';
import { TIER } from '../../tokens';
import { CONTROL } from '../controls/controlTokens';
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
 *   do so even at `position: static`). Everything inside `.scene`, `.hand`, and
 *   `.cluster` is therefore trapped at that region's rung. Anything that must
 *   outrank a chrome region has to be a **sibling** of those regions, as the
 *   decision sheet, the decision plaque, the stack stage, and the overlays all
 *   are — `LiveMatchTable` mounts them at the shell root for exactly this
 *   reason, and `LiveMatchTable.occlusion.test.tsx` asserts it.
 * - The plane's own layers (`live-plane.module.css`) are scoped by
 *   `isolation: isolate` on the plane host and are intentionally *not* on this
 *   ladder: they order objects within the scene only.
 */
export const LAYER = {
  /** The battlefield plane and everything staged on it. */
  scene: 1,
  /** Contextual chrome that floats over the scene: the hand and the cluster. */
  shell: 10,
  /** The presented hand during a forced decision — raised within its own band. */
  shellRaised: 12,
  /**
   * The highest chrome rung. ADR 0023's permanent top bar is gone (#534); what
   * sits here now is chrome that must outrank the hand and the cluster — the
   * stack stage's history surface handle and the transient activity ticker —
   * while still yielding to a decision.
   */
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
  /** The hand size the band is sized to keep at the 44 px floor — an opening
   * hand. Larger hands are the layout model's fan-paging case (#533). */
  openingHand: 7,
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

/**
 * The shell's regions under [ADR 0032](../../../../docs/decisions/0032-contextual-shell-anatomy.md).
 *
 * The permanent `top`, `rail`, `identity`, and `decisions` reservations are
 * gone: the top status bar, the right stack/activity rail, the receiver's
 * identity column, and the bottom decision dock were carved tracks that
 * existed whether or not they had anything to say. What remains is the scene,
 * two chrome regions that overlay it, and the box the scene may actually stage
 * objects in.
 */
export interface ShellBands {
  compact: boolean;
  /** The safe viewport the shell lays out inside (browser insets removed). */
  viewport: ShellRect;
  /**
   * The battlefield. It **is** the viewport now — the arena is visible behind
   * the controls rather than ending where they begin.
   */
  scene: ShellRect;
  /** The receiver's hand fan: a bottom band, left of the control column. */
  hand: ShellRect;
  /** The lower-right control cluster: primary, utilities, phase plaque. */
  cluster: ShellRect;
  /**
   * The chrome-free rect the plane carves its slots inside
   * (`plane/types.ts`'s `PlaneViewport.safe`). The scene spans the whole
   * viewport; only *staging* is inset, so no seat, card, crest, or path
   * endpoint is ever laid under a control.
   */
  staging: ShellRect;
}

/** What contextual chrome is currently standing on the scene. */
export interface ShellOccupancy {
  /**
   * Whether the stack stage is drawn — i.e. the stack is non-empty. It claims
   * the right-hand column above the cluster (`control-language.md` §4.4/D7:
   * the stack rail is that column and the cluster sits at its foot).
   *
   * This is an *input to the geometry* rather than a constant, because #534
   * requires that an "empty stack/log consumes no permanent battlefield
   * width". Reserving the column unconditionally would be simpler and would
   * fail that criterion; the cost is that the plane re-stages when the first
   * object hits the stack, which the layout model already treats as ordinary
   * re-staging.
   */
  stackPresent?: boolean;
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

/**
 * The control cluster's height: the stacked primary, the icon/utility row, and
 * the phase plaque, with a gap between each (`control-language.md` §3.3).
 *
 * Derived rather than written down, so growing the plaque or the primary moves
 * the staging box that clears it instead of quietly overlapping the board.
 */
export function clusterHeight(): number {
  return CONTROL.hPrimary + CONTROL.clusterGap + CONTROL.hit + CONTROL.clusterGap + CONTROL.plaqueH;
}

/**
 * How much of the viewport's bottom edge chrome stands on: whichever of the
 * hand fan and the inset control cluster reaches higher. The staging box stops
 * here, so the receiver's band is never laid under either.
 */
export function bottomChromeHeight(): number {
  return Math.max(handBandHeight(), clusterHeight() + 2 * CONTROL.clusterMargin);
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
  occupancy: ShellOccupancy = {},
): ShellBands {
  const left = inset.left ?? 0;
  const top = inset.top ?? 0;
  const w = Math.max(0, viewport.width - left - (inset.right ?? 0));
  const h = Math.max(0, viewport.height - top - (inset.bottom ?? 0));
  const compact = isCompactShell(viewport);
  const margin = CONTROL.clusterMargin;

  // The scene is the whole safe viewport: ADR 0032's first consequence is that
  // cards get the viewport back, and #534 asks for the arena to remain visible
  // *behind* the controls rather than stopping where they start.
  const scene: ShellRect = { x: left, y: top, w, h };

  // The control cluster: a fixed-size column inset from the bottom-right
  // corner. §3.3 pins it in CSS px (D1's scale anchor), never as a fraction, so
  // it is the same physical size on every viewport.
  const clusterW = Math.min(CONTROL.wCluster, Math.max(0, w - 2 * margin));
  const clusterH = clusterHeight();
  const cluster: ShellRect = {
    x: left + w - margin - clusterW,
    y: top + h - margin - clusterH,
    w: clusterW,
    h: clusterH,
  };

  // The hand fan keeps the bottom edge but yields the cluster's column, so the
  // two never overlap — on the compact composition it yields the whole width
  // and sits above the cluster instead, because a phone has no room for both
  // side by side.
  const handH = handBandHeight();
  const hand: ShellRect = compact
    ? { x: left, y: cluster.y - margin - handH, w, h: handH }
    : {
        x: left,
        y: top + h - handH,
        w: Math.max(0, w - clusterW - 2 * margin),
        h: handH,
      };

  // The staging box. Its bottom clears whichever chrome reaches higher; its
  // right edge yields the stack stage's column ONLY while the stack is drawn,
  // so an empty stack costs the battlefield nothing (#534's own criterion).
  const bottomInset = compact
    ? Math.max(top + h - hand.y, handH)
    : Math.max(bottomChromeHeight(), handH);
  const rightInset = occupancy.stackPresent ? clusterW + 2 * margin : 0;
  const staging: ShellRect = {
    x: left,
    y: top,
    w: Math.max(0, w - rightInset),
    h: Math.max(0, h - bottomInset),
  };

  return { compact, viewport: { x: left, y: top, w, h }, scene, hand, cluster, staging };
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
 * (`docs/design/layout-model.md` §Stress dispositions).
 *
 * **Paging shipped in #533** and gates on exactly this arithmetic:
 * `table/handFan.ts`'s `fanCapacity` is this function inverted, and the page
 * size is derived from it, so `handFanSpacing(pageSize, bandWidth) ≥
 * SHELL.minHit` holds for every page of every plan. This function stays as the
 * band-sizing input (`fullBottomColumns` reads its inverse) and as the
 * independent check `handFan.test.ts` agrees with at the paging boundary.
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
export function shellStyleVars(viewport: Pick<Viewport, 'width' | 'height'>): CSSProperties {
  const bands = shellBands(viewport);
  return {
    // The two chrome regions, published as rects rather than as grid tracks:
    // under ADR 0032 they overlay the scene instead of carving it, so the
    // stylesheet positions them absolutely from these.
    '--shell-hand-w': `${bands.hand.w}px`,
    '--shell-cluster-w': `${bands.cluster.w}px`,
    '--shell-cluster-h': `${bands.cluster.h}px`,
    '--shell-bottom-chrome-h': `${bottomChromeHeight()}px`,
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
