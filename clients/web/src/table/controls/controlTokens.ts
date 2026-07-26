/**
 * The TS mirror of the match-control geometry declared in `chrome/tokens.css`
 * (`docs/design/control-language.md` §2.2, issue #534).
 *
 * Layout code that has to *reason* about a control — the cluster's band height,
 * the decision plaque's anchoring, the stack stage's clearance — needs these as
 * numbers, not as `var()` strings it cannot add up. Rather than let those call
 * sites re-declare `36` and `268` and drift from the stylesheet, they read this
 * module, and {@link controlTokens.test.ts} parses `chrome/tokens.css` and fails
 * if any value here disagrees with the declaration it mirrors.
 *
 * This is the same discipline `shellLayout.ts`'s {@link LAYER} applies to the
 * `--rune-z-*` ladder, for the same reason: a token that exists in two places is
 * a token that will eventually be edited in one.
 *
 * **Colour is deliberately absent.** Every control hue stays in CSS, where the
 * stylesheet is the only consumer; mirroring it here would invite a component to
 * paint from JS and put a literal hex back into the tree that ADR 0019 removed
 * it from.
 */

/**
 * Control geometry in CSS px. The spec's scale anchor (D1) is 1 baseline px =
 * 1 CSS px — pinned by the 44 px icon button, which is exactly the touch floor —
 * so these are fixed lengths and never viewport fractions. Board geometry scales
 * with the viewport (`layout-model.md`); controls do not.
 */
export const CONTROL = {
  /** The large stadium primary's plate height. */
  hPrimary: 56,
  /** Every other control's drawn plate height (§3.1). */
  h: 36,
  /**
   * The minimum hit box (D2). Four drawn controls fall below the 44 px floor —
   * UNDO at 35, CONFIRM/CANCEL at 31, RESOLVE/RESPOND at 33. The **plate** keeps
   * the drawn height as the visual form and the **hit box** is padded out to
   * this with transparent inset padding, so the baseline's proportions survive
   * without shipping a sub-floor target.
   */
  hit: 44,
  /** The control cluster's column width (§3.3). */
  wCluster: 268,
  /** A paired button: UNDO, CONFIRM, CANCEL, RESOLVE, RESPOND. */
  wPair: 118,
  /** The 45° corner cut on the chamfered family. */
  chamfer: 8,
  /** The phase plaque's plate height. */
  plaqueH: 68,
  /** The depth of the plaque's hexagonal points. */
  plaquePoint: 22,
  /** The gold frame stroke on every framed control. */
  frameW: 2,
  /** The primary's bevel frame stroke — one px heavier than the gold family. */
  primaryFrameW: 3,
  /** Step-pip diameter. */
  pipSize: 11,
  /** Step-pip centre-to-centre pitch. */
  pipPitch: 21,
  /** The cluster's inset from the viewport edge (§3.3). */
  clusterMargin: 28,
  /** The gap between cluster rows, and between an icon and its neighbouring pill. */
  clusterGap: 12,
} as const;

/** The token name in `chrome/tokens.css` each {@link CONTROL} value mirrors. */
export const CONTROL_TOKEN_NAMES: Record<keyof typeof CONTROL, string> = {
  hPrimary: '--rune-control-h-primary',
  h: '--rune-control-h',
  hit: '--rune-control-hit',
  wCluster: '--rune-control-w-cluster',
  wPair: '--rune-control-w-pair',
  chamfer: '--rune-control-chamfer',
  plaqueH: '--rune-plaque-h',
  plaquePoint: '--rune-plaque-point',
  frameW: '--rune-control-frame-w',
  primaryFrameW: '--rune-primary-frame-w',
  pipSize: '--rune-pip-size',
  pipPitch: '--rune-pip-pitch',
  clusterMargin: '--rune-cluster-margin',
  clusterGap: '--rune-cluster-gap',
};

/**
 * The **menu rungs** (§3.4, issue #566) — the one viewport term the control
 * language has, and the mirror of `--rune-menu-scale` / `--rune-menu-scale-dense`
 * in `chrome/tokens.css`.
 *
 * A rung multiplies §3.1's own plate values; it never replaces them. `min` is
 * the floor a rung clamps to, and it is `1` on both: a small or zoomed viewport
 * draws the menus at exactly the match's control sizes and never smaller. `basis`
 * is the `100vmin` divisor and `max` the ceiling, so the rung a viewport takes is
 * `clamp(min, vmin / basis, max)` — {@link menuRung} is that formula, and
 * `controlTokens.test.ts` parses the stylesheet and fails if the two disagree.
 *
 * `vmin` rather than `vw`: an ultrawide's surplus width is gutter (the plane's
 * own policy in `plane/metrics.ts` spends it the same way), and growing controls
 * into it would push the arena's content off the top and bottom of a short
 * window.
 */
export const MENU_RUNG = {
  /** One decision on an empty plaza: the front door, the lobby, create-table. */
  open: { min: 1, basis: 620, max: 1.6 },
  /**
   * A seat per player plus the table's plaque in one box — the ready room's
   * ring. Seats scale with the rung and the arena does not, so a generous rung
   * grows the seats past the room; this one grows more slowly and stops sooner.
   */
  dense: { min: 1, basis: 900, max: 1.25 },
} as const;

/** A menu rung's name. */
export type MenuRungName = keyof typeof MENU_RUNG;

/** The token in `chrome/tokens.css` each rung mirrors. */
export const MENU_RUNG_TOKEN_NAMES: Record<MenuRungName, string> = {
  open: '--rune-menu-scale',
  dense: '--rune-menu-scale-dense',
};

/**
 * The multiplier a rung resolves to on a viewport — the TS form of the
 * stylesheet's `clamp(min, 100vmin / basis, max)`. Layout code that has to
 * reason about a menu's drawn size (and the tests that pin the rung's behaviour
 * at the reference geometries) reads this rather than re-deriving the clamp.
 */
export function menuRung(rung: MenuRungName, viewport: { width: number; height: number }): number {
  const { min, basis, max } = MENU_RUNG[rung];
  return Math.max(min, Math.min(max, Math.min(viewport.width, viewport.height) / basis));
}

/**
 * The five phase groups the step pips render (D3), in turn order.
 *
 * The baselines draw four pips in panel 6 and three in situ; those counts are
 * illustrative of the *form*, not a model of the turn. The pips render the phase
 * groups the client already classifies, so the row means something a player can
 * check against the step list behind the chevron.
 */
export const PIP_COUNT = 5;

/**
 * The width the pip row occupies: five pips at a 21 px pitch. Derived rather
 * than written down, so changing {@link PIP_COUNT} or the pitch moves the row
 * instead of silently overflowing the plaque.
 */
export function pipRowWidth(count: number = PIP_COUNT): number {
  if (count <= 0) return 0;
  return CONTROL.pipSize + (count - 1) * CONTROL.pipPitch;
}
