/**
 * The decision plaque's **anchoring algorithm** (`docs/design/control-language.md`
 * §10.1 / D17 and §10.2, issue #534, under
 * [ADR 0032](../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * ADR 0032 removed the permanent bottom dock, so a decision's controls no longer
 * have a fixed home to hide in — they are placed *next to the thing being asked
 * about*. That is the whole reason this module exists, and it is also the whole
 * risk: a surface that follows its subject can just as easily cover it. §10 opens
 * with the constraint this file implements, and it is absolute — the plaque
 * **must not occlude the subject of the decision or any candidate**. Issue #528
 * is the shipped proof of what happens otherwise: chrome painted over a forced
 * mulligan and left it unanswerable.
 *
 * ## Why it is a pure function
 *
 * `ui-requirements.md` §Performance and determinism requires placement to be
 * deterministic for a given view + viewport, and the client's hard rule requires
 * the whole UI to rebuild from one `GameView` + pending prompt. Both are met by
 * making this a total function of rects: no DOM measurement, no memory of where
 * the plaque was last frame, no animation state. Feed it the same input twice and
 * it returns the same rect — {@link placeDecisionPlaque} is tested for exactly
 * that, because "it drifted by a pixel each frame" is the failure mode a
 * measurement-driven placement would have.
 *
 * The rects come from the caller because the caller is the only thing that can
 * know them: `LiveMatchTable` already collects live geometry in viewport
 * coordinates (`notePlaneGeometry` scales the plane's own rects by the host's
 * `getBoundingClientRect`). This module deliberately does not reach for the DOM,
 * so it stays testable in jsdom and honest about being pure layout arithmetic.
 *
 * ## The five steps, verbatim from §10.1
 *
 * 1. Prefer **below** the subject when the subject sits in the board's top half;
 *    **above** when it sits in the bottom half.
 * 2. Reject any position whose rect intersects the subject rect or any candidate
 *    rect.
 * 3. If rejected, slide along the perpendicular axis in 16 px steps until clear.
 * 4. If no clear position exists, **dock at the cluster slot** (lower right),
 *    which `layout-model.md` guarantees is outside the board and outside the
 *    centre corridor.
 * 5. Always clamp to the viewport with a ≥ 16 px gutter.
 *
 * Step 5 is applied to **every trial position before step 2 tests it**, not once
 * at the end. Clamping after testing would be a lie: a position that was clear
 * before it was pushed off the viewport edge is not clear after.
 *
 * ## Two readings this module had to settle
 *
 * - **Wing slots.** §10.2 says a wing-board subject "never places inside a wing
 *   slot — dock at the cluster". Read narrowly that is a rule about the
 *   *subject*; read plainly it is also a rule about the *plaque*. Both are
 *   implemented: a subject inside a wing docks, and a wing slot is a rejection
 *   rect like a candidate. The plain reading is kept because a wing carries an
 *   opponent's crest cluster, which `layout-model.md` says can never degrade away
 *   — covering it during a combat declaration hides who is being attacked.
 * - **"Dismissible" vs. the forced decision.** §10.2 calls the plaque dismissible
 *   at every geometry; §8/D19 says a view-forced decision offers no cancel. §8
 *   wins — it is the normative taxonomy, and there is no neutral state for a
 *   mulligan to be dismissed *to*. Dismissal is the CANCEL control's presence,
 *   which {@link DecisionPlaque} makes optional for exactly that reason.
 *
 * Both are reported to the maintainer rather than silently chosen; see the PR
 * notes for #534.
 */
import { CONTROL } from '../controls';
import { rectsOverlap, type ShellRect } from '../live/shellLayout';

/**
 * The plaque's own geometry, in CSS px. The spec's scale anchor (D1) is
 * 1 baseline px = 1 CSS px, so these are fixed lengths and never viewport
 * fractions — {@link PLAQUE.sheetMaxFraction} is the single deliberate exception,
 * because §10.2 states the bottom sheet's cap as a fraction of the viewport.
 */
export const PLAQUE = {
  /**
   * The drawn plate width of control-ui panel 7's "CHOOSE ATTACKERS" plaque
   * (§3.1: "≥ 251 × content"). Mirrors `--rune-plaque-min-w` in
   * `chrome/tokens.css`; {@link plaqueAnchor.test.ts} does not police that pair,
   * but the stylesheet reads the token and this module reads this constant, so a
   * change has to be made in both places on purpose.
   */
  minW: 251,
  /** The plate's internal padding (`--rune-space-6`). */
  pad: 12,
  /** The gap between the title line and the control row, and between controls. */
  rowGap: 12,
  /** One line of `--rune-type-heading` (16 px) at the plate's line height. */
  titleH: 22,
  /** §10.1 step 5: the minimum clearance between the plaque and the viewport edge. */
  gutter: 16,
  /** §10.1 step 3: the perpendicular slide increment. */
  slideStep: 16,
  /** Clearance between the subject's edge and the plaque's, so they read apart. */
  subjectGap: 12,
  /** §10.2: the bottom sheet's height cap, as a fraction of viewport height. */
  sheetMaxFraction: 0.4,
  /**
   * The tablet floor (`layout-model.md` §Hand-offs): at or above this width in
   * landscape the desktop staging holds, and §10.2 places the plaque as desktop.
   * Below it — or in portrait at any width — the compact change-of-kind engages
   * and the plaque takes the bottom-sheet form.
   */
  tabletFloorW: 1180,
  /**
   * §10.2: at five or six seats the plaque **always** docks. The corridor is
   * dense with combat paths at that count and the wings are digested, so there is
   * no position near the subject that is not in something's way.
   */
  dockFromSeats: 5,
} as const;

/**
 * How the plaque is rendered — the form the stylesheet keys off, not merely
 * where it landed.
 *
 * `anchored` and `docked` are both free-floating plates; they differ in whether
 * the placement stayed near the subject. `sheet` is a different surface: full
 * width, capped height, resting above the receiver's band.
 */
export type PlaqueForm = 'anchored' | 'docked' | 'sheet';

/** Which side of the subject the plaque took, for tests and for assistive text. */
export type PlaqueSide = 'below' | 'above' | 'docked' | 'sheet';

/** Why the plaque docked, when it did. Every dock has exactly one cause. */
export type PlaqueDockReason =
  /** §10.2: five or six seats always dock. */
  | 'seat-count'
  /** §10.2: the subject sits on a wing board. */
  | 'wing-subject'
  /** The decision has no on-board subject to anchor to (an option-only prompt). */
  | 'no-subject'
  /** §10.1 step 4: every slid position still intersected something. */
  | 'no-clear-position';

/** Where the plaque goes, and how it got there. */
export interface PlaquePlacement {
  /** The plaque's rect in viewport coordinates. Applied as inline `left/top/width`. */
  rect: ShellRect;
  form: PlaqueForm;
  side: PlaqueSide;
  /**
   * The perpendicular offset step 3 applied, in px — 0 when the preferred
   * position was already clear. Signed: positive slides right (below/above a
   * subject the perpendicular axis is horizontal).
   */
  slide: number;
  /** Present only when {@link PlaquePlacement.form} is `docked`. */
  dockReason?: PlaqueDockReason;
}

/**
 * Everything the placement needs, all of it already known to the layout.
 *
 * Every rect is in the **same viewport coordinate space** — the space
 * `getBoundingClientRect` reports and `LiveMatchTable.notePlaneGeometry` already
 * converts the plane's own rects into. Mixing plane-local and viewport
 * coordinates here would place the plaque somewhere plausible and wrong, which is
 * the one failure this module cannot detect for itself.
 */
export interface PlaqueAnchorInput {
  /** The safe viewport (browser insets removed) — `shellBands(viewport).viewport`. */
  viewport: ShellRect;
  /**
   * The board the top-half/bottom-half test of step 1 is taken against —
   * `shellBands(viewport).scene`. Not the viewport: the shell's top bar and hand
   * band are not board, and measuring the halves against them would flip the
   * preferred side for a subject near the boundary.
   */
  board: ShellRect;
  /** The plaque's own size — {@link estimatePlaqueSize}. */
  size: { w: number; h: number };
  /**
   * The lower-right control cluster's slot, the step-4 dock. `layout-model.md`
   * guarantees it is outside the board and outside the centre corridor, which is
   * what makes it a safe terminal fallback.
   */
  cluster: ShellRect;
  /** Seats at the table — `view.seat_order.length`. Drives the §10.2 5–6 rule. */
  seatCount: number;
  /**
   * The decision's subject: the card or permanent the action belongs to. Absent
   * for a decision with no on-board subject (a bare `option` prompt), which docks.
   */
  subject?: ShellRect;
  /**
   * The **active** slot's candidate rects. Only the active slot: §6.1 makes
   * non-active candidates inert, and treating them as blockers would push the
   * plaque away from positions that are in fact clear.
   */
  candidates?: ShellRect[];
  /** The wing slots' rects. See the module note on the two readings of §10.2. */
  wings?: ShellRect[];
  /**
   * The receiver's band. The bottom sheet rests its bottom edge on this band's
   * top edge, so §10.2's "the receiver's band is never covered" holds by
   * construction rather than by hope.
   */
  receiverBand?: ShellRect;
  /**
   * Override the form {@link plaqueForm} would derive. Present so a test can
   * exercise one geometry without synthesising a whole viewport, and so the shell
   * can force the sheet if a future composition wants it; production passes
   * nothing and takes the derived form.
   */
  form?: PlaqueForm;
}

/**
 * The plaque's size for a control count, derived rather than measured.
 *
 * Measuring would make placement depend on layout, and layout on placement — the
 * loop that produces a plaque that jitters for a frame on every view. The
 * estimate is an upper bound in practice: the controls are fixed-width
 * (`--rune-control-w-pair`) and the title is one line, so the only way the drawn
 * plate exceeds this is a title long enough to wrap, which widens nothing (the
 * plate's width is already fixed) and grows the height by one line the plaque
 * absorbs inside its own padding.
 */
export function estimatePlaqueSize(controlCount: number): { w: number; h: number } {
  const controls = Math.max(0, controlCount);
  const row = controls * CONTROL.wPair + Math.max(0, controls - 1) * PLAQUE.rowGap;
  return {
    w: Math.max(PLAQUE.minW, 2 * PLAQUE.pad + row),
    // The control row is measured at the 44 px HIT box, not the 36 px drawn
    // plate (D2) — the transparent correction padding is real box, and a plaque
    // sized to the plate would clip the targets it was sized for.
    h: 2 * PLAQUE.pad + PLAQUE.titleH + PLAQUE.rowGap + CONTROL.hit,
  };
}

/**
 * The form a viewport takes (§10.2's last two rows).
 *
 * The boundary is `layout-model.md`'s own tablet floor rather than a number
 * invented here: full desktop staging holds at 1180 px landscape and wider, and
 * below it — or in portrait at any width — the compact change-of-kind engages and
 * the plaque becomes a bottom sheet. Deriving it from the same floor is what
 * keeps the plaque's form and the board's staging from disagreeing about which
 * geometry the player is on.
 *
 * Note this floor is **not** `SHELL.compactBreakpoint` (900): that switch chooses
 * the shell's bottom-row composition, this one chooses the board's staging kind.
 * A 1000 px landscape window takes the full shell composition and the sheet form.
 */
export function plaqueForm(viewport: Pick<ShellRect, 'w' | 'h'>): PlaqueForm {
  const portrait = viewport.h > viewport.w;
  if (portrait || viewport.w < PLAQUE.tabletFloorW) return 'sheet';
  return 'anchored';
}

/**
 * §10.1 step 5. Shrinks the rect to fit before moving it, so a plaque larger than
 * the gutter-inset viewport is trimmed rather than pushed half off-screen.
 */
export function clampToViewport(
  rect: ShellRect,
  viewport: ShellRect,
  gutter: number = PLAQUE.gutter,
): ShellRect {
  const w = Math.min(rect.w, Math.max(0, viewport.w - 2 * gutter));
  const h = Math.min(rect.h, Math.max(0, viewport.h - 2 * gutter));
  const minX = viewport.x + gutter;
  const minY = viewport.y + gutter;
  const maxX = viewport.x + viewport.w - gutter - w;
  const maxY = viewport.y + viewport.h - gutter - h;
  return {
    x: Math.min(Math.max(rect.x, minX), maxX),
    y: Math.min(Math.max(rect.y, minY), maxY),
    w,
    h,
  };
}

/** §10.1 step 4: the terminal fallback, right-aligned in the cluster column. */
function dock(input: PlaqueAnchorInput, dockReason: PlaqueDockReason): PlaquePlacement {
  // Right-aligned rather than left-aligned so the plaque keeps the cluster's own
  // 28 px margin from the viewport edge whatever its width — §3.3 fixes the
  // margin, not the column's left edge.
  const rect = clampToViewport(
    {
      x: input.cluster.x + input.cluster.w - input.size.w,
      y: input.cluster.y,
      w: input.size.w,
      h: input.size.h,
    },
    input.viewport,
  );
  return { rect, form: 'docked', side: 'docked', slide: 0, dockReason };
}

/**
 * §10.2's phone / tablet-portrait row: full width, capped at 40 % of the
 * viewport's height, resting on the receiver's band so the band is never covered.
 *
 * The board re-stages so the subject stays visible above the sheet; that
 * re-staging is the plane's job, not this module's. What this module guarantees
 * is the space: the sheet never grows past the cap and never crosses the band's
 * top edge, so there is always room above it for the plane to stage into.
 */
function sheet(input: PlaqueAnchorInput): PlaquePlacement {
  const { viewport, receiverBand } = input;
  const gutter = PLAQUE.gutter;
  const bottom = receiverBand ? receiverBand.y : viewport.y + viewport.h - gutter;
  const available = Math.max(0, bottom - (viewport.y + gutter));
  const h = Math.min(input.size.h, Math.floor(viewport.h * PLAQUE.sheetMaxFraction), available);
  const rect = clampToViewport(
    { x: viewport.x + gutter, y: bottom - h, w: viewport.w - 2 * gutter, h },
    viewport,
    gutter,
  );
  return { rect, form: 'sheet', side: 'sheet', slide: 0 };
}

/**
 * Place the decision plaque. The one entry point; every geometry of §10.2 routes
 * through it, and the §10.1 algorithm runs only for the geometries that anchor.
 *
 * Consumed in production by `LiveMatchTable`, which passes the result straight to
 * {@link DecisionPlaque}'s `placement` prop.
 */
export function placeDecisionPlaque(input: PlaqueAnchorInput): PlaquePlacement {
  const form = input.form ?? plaqueForm(input.viewport);
  if (form === 'sheet') return sheet(input);

  // §10.2, rows 5 and 4: two geometries decide before the subject is consulted at
  // all, because at those geometries there is no near-the-subject position worth
  // trying — not because the algorithm failed to find one.
  if (input.seatCount >= PLAQUE.dockFromSeats) return dock(input, 'seat-count');
  const { subject } = input;
  if (!subject) return dock(input, 'no-subject');
  const wings = input.wings ?? [];
  if (wings.some((wing) => rectsOverlap(wing, subject))) return dock(input, 'wing-subject');

  // Step 2's rejection set. The subject leads it: the decision may not cover the
  // very thing it is asking about, which is the sentence §10 opens with.
  const blockers = [subject, ...(input.candidates ?? []), ...wings];

  // Step 1. The board's own midline, not the viewport's — see `board` above.
  const below = subject.y + subject.h / 2 < input.board.y + input.board.h / 2;
  const side: PlaqueSide = below ? 'below' : 'above';
  const y = below
    ? subject.y + subject.h + PLAQUE.subjectGap
    : subject.y - PLAQUE.subjectGap - input.size.h;
  const baseX = subject.x + subject.w / 2 - input.size.w / 2;

  // Step 3. The perpendicular axis of a below/above placement is horizontal, so
  // the slide walks x. Right before left at each step, and the whole walk is
  // bounded by the span a plaque could occupy — the order and the bound are what
  // make the answer deterministic rather than merely correct.
  const span = input.viewport.w + input.size.w;
  const tried = new Set<string>();
  for (let step = 0; step * PLAQUE.slideStep <= span; step += 1) {
    for (const direction of step === 0 ? [0] : [1, -1]) {
      const slide = direction * step * PLAQUE.slideStep;
      // Step 5 before step 2: the clamped rect is the rect that would be drawn,
      // so it is the rect that must be tested for intersection.
      const rect = clampToViewport(
        { x: baseX + slide, y, w: input.size.w, h: input.size.h },
        input.viewport,
      );
      const key = `${rect.x},${rect.y},${rect.w},${rect.h}`;
      // Once clamped against an edge, further slide in that direction returns the
      // same rect; re-testing it would burn the whole walk on one position.
      if (tried.has(key)) continue;
      tried.add(key);
      if (blockers.some((blocker) => rectsOverlap(rect, blocker))) continue;
      return { rect, form: 'anchored', side, slide };
    }
  }

  // Step 4.
  return dock(input, 'no-clear-position');
}
