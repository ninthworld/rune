import type { Rect } from '../scene/types';
import type { PlaneViewport } from './types';

/**
 * Plane staging metrics (logical px, or fractions of the plane dimension noted).
 * The proportions carry the staging prototype's evidence
 * (`prototypes/ui-2-5d-layouts-v1.html`, reference-only) into the model of
 * `docs/design/layout-model.md`; the 44 px floor is normative
 * (`docs/design/presentation-budgets.md` §Accessibility).
 */
export const PLANE = {
  /** Inset from a slot rect to its card content area. */
  pad: 10,
  /** Vertical gap between type-grouped rows. */
  rowGap: 8,
  /** Horizontal gap between cards in a row. */
  cardGap: 6,
  /** The interactive-target floor: every hotspot is at least this square. */
  minHit: 44,
  /*
   * There is no `crest` constant here any more (issue #582 §1).
   *
   * It was `{ w: 52, h: 52 }`, documented as "the headroom constant a slot must
   * clear above its board for the seat's identity to stage on-plane". It was
   * neither: the drawn medallion is `D` from {@link PLANE.cluster}'s rung ladder
   * — 112 px local, 96 px focused — so the constant was always smaller than the
   * thing it claimed to reserve for, and no staging code ever read it, so no
   * slot ever cleared anything. The seat cluster was simply drawn over its own
   * board. The board's reservation is derived from the staged cluster's own
   * bounds now (`regions.ts`, `clusterReserve`), which cannot go stale.
   */
  /*
   * …and no `pile` constant either. It was `{ w: 44, h: 62 }`, "the digest rack
   * button's floor", and it was the reason a digested rack could only hold a
   * column of numbers: 44 × 62 is too small for four identified zones. The
   * button is derived from the chip grid it has to hold now
   * ({@link PLANE.rack.digest}, `rack.ts`), floored at {@link PLANE.minHit} —
   * so growing a sub-indicator grows the button instead of overflowing it.
   */
  /**
   * The receiver's band: full-width bottom third (±), fractions of W/H.
   *
   * In a **duel** it is not flush with the box's bottom edge — `carveSlots`
   * drops it by exactly the far side's own top margin, so the two rows sit
   * symmetrically inside the arena with the corridor between them (issue #582
   * §2). Flush against a far side dropped `0.09` clear of the top is what the
   * maintainer's capture shows: the player's row near the CENTRE of the arena,
   * the opponent's jammed against the top edge, and no shared space for the one
   * relationship the screen most needs to express during Declare Attackers.
   */
  receiver: { x: 0.12, w: 0.76, h: 0.33 },
  /**
   * The far side at 2 players: the opponent, full width. Dropped clear of the
   * plane's top edge (issue #531) so the seat's crest cluster stages **onstage**
   * above it — at `y = 0.02` the crest resolved to a negative `y` at every
   * reference viewport and the focused seat's identity was off-canvas. The
   * band's bottom edge is unchanged at `0.36`, so the corridor is untouched.
   */
  duelFar: { x: 0.12, y: 0.09, w: 0.76, h: 0.27 },
  /** The far side at 3+ players: the focused opponent, top center — same drop,
   * same unchanged `0.36` bottom edge. */
  far: { x: 0.2, y: 0.1, w: 0.6, h: 0.26 },
  /**
   * The clear band between the two rows, as a fraction of the staging box's
   * height. Not a slot — it is what is left once the two rows take their share
   * — but it is the space combat is drawn in, so it is stated as a floor rather
   * than left to arithmetic. `plane-composition.test.ts` checks it at every
   * supported viewport and player count.
   */
  corridorMinH: 0.2,
  /**
   * Ultrawide surplus-width policy (layout-model §Hand-offs and open items):
   * beyond this aspect the focused far side and the center corridor stop
   * widening — the central column is capped at `H × corridorMaxAspect`, centered,
   * and the surplus horizontal width falls into the side gutters, where the wings
   * (still full-width fractions of W) spend it. 16:9 is the widest standard
   * desktop aspect that stages at full width; 16:10, 3:2, and narrower all fall
   * below it, so only true 21:9 ultrawide redistributes width. A duel has no
   * wings and keeps its full-width far side. */
  corridorMaxAspect: 16 / 9,
  /**
   * The tablet-landscape geometry floor (layout-model §Hand-offs, ui-blueprint
   * §Tablet landscape): the smallest landscape width that still shows full
   * desktop multiplayer staging (three opponent battlefields in full). At or
   * above it desktop staging holds; below it multiplayer changes kind — the
   * compact branch engages. The tablet-landscape reference (1180×820) sits at
   * this floor; the 1280×800 desktop reference sits above it. */
  compactFloorWidth: 1180,
  /** Wing staging: outward from the top, up to two per side. */
  wing: {
    /**
     * First wing rank's top, as a fraction of H, for a **two-per-side** stack
     * (5–6 players): the pair of ranks then spans the seat flank exactly as
     * `environment-system.md` §3.1 panels 4–5 describe ("Zone B fully covered").
     */
    top: 0.12,
    /**
     * First wing rank's top for **one-per-side** staging (3–4 players): the
     * baseline arena hangs a single peripheral seat at mid height on each flank
     * (`rune-2.5d-interface-baseline.jpg`, wing medallions at `y ≈ 0.28`), not
     * pinned to the top edge. Dropping the lone rank also lifts its crest out of
     * the top-left / top-right prop pockets, which the `0.12` staging put it in.
     */
    singleTop: 0.24,
    /** Fraction of a wing's width staged past the plane edge (the felt bleed)
     * — what keeps wing inner edges clear of the center corridor. */
    bleed: 0.28,
    /** One wing per side (3–4 players): the larger wing. */
    single: { w: 0.24, h: 0.4 },
    /** Two wings per side (5–6 players): the digest-rung wing. */
    double: { w: 0.21, h: 0.25 },
    /**
     * The digest threshold (layout-model §The degradation ladder, rung 4): a
     * wing whose slot is narrower than this fraction of W stops drawing its
     * board and digests from the start. The double wing (0.21·W, 269 px at the
     * 1280 reference) falls below it; the single wing (0.24·W, 307 px) stays
     * above — so two-per-side staging (5–6 players) is exactly the digest
     * baseline, aspect-independently (both widths and the threshold scale with
     * W). The far side and the receiver never digest. */
    digestBelowWidthFrac: 0.225,
    /** Vertical gap between wing ranks, as a fraction of H. */
    rankGap: 0.03,
  },
  /**
   * The **seat envelope** the plane may stage inside — Zone A ∪ Zone B of
   * `docs/design/environment-system.md` §2.2, as fractions of the canvas.
   * Outside the focal core (`x < coreX` or `x > 1 − coreX`) a staged rect must
   * lie inside the flank band, because everything else out there is Zone C: the
   * theme's prop pockets, the `AMBIENT SPACE` reservation, and the in-match
   * wordmark (§6.3).
   *
   * Transcribed rather than imported so `plane/` keeps **no** dependency on
   * `environment/` — the dependency runs the other way (`planeOccupancy.ts`
   * calls `carveSlots`). `plane-composition.test.ts` asserts the two agree.
   */
  envelope: { coreX: 0.1, flankTop: 0.1, flankBottom: 0.67 },
  /**
   * The per-seat zone rack (`docs/design/zone-geography.md` §2). Offsets are in
   * `u` — the rack's pile-card width at its tier — measured from the seat's
   * identity anchor, along the rack's reading axis and perpendicular to it.
   */
  rack: {
    /** Pile card aspect: a pile card is `u × pileAspect·u` (§2.1). */
    pileAspect: 1.4,
    /** Identity medallion diameter, in `u` (§2.2). */
    medallion: 1.45,
    /** Perpendicular offset from the anchor to the pile strip, in `u` (§2.2). */
    strip: 1.85,
    /** Along-axis pitch between pile slots, in `u` (§2.3, uniform). */
    pitch: 1.6,
    /** The command slot's along offset, in `u` (§2.2, ±1.0u tolerance). */
    commandAlong: 1,
    /** The command slot's perpendicular offset when the strip is inboard. */
    commandInboard: 3.35,
    /** The command slot's perpendicular offset when the strip is outboard —
     * it crosses to the anchor's far side so it is still the innermost element. */
    commandOutboard: -2,
    /** The command slot's width, in `u` (§2.2 — the largest rack element). */
    commandScale: 1.35,
    /** Breathing room, in `u`, between the command slot and the pile strip where
     * §2.2's fixed `+3.35u` is not enough to separate them (a horizontal rack —
     * see `rack.ts`). Never applies on a vertical rack, where `3.35u` wins. */
    commandClear: 0.2,
    /**
     * The digest trigger (§2.3 [D4]): below this `u` the along-axis pitch
     * resolves under 48 px and three 44 px hit rects can no longer be separated,
     * so the rack falls to the digest variant. The single numeric trigger.
     */
    minU: 30,
    /** The clearance halo the rack's hit-rect union keeps off the corridor (§2.4 [D5]). */
    halo: 12,
    /** Gap between the rack strip and the region's card content area. */
    gap: 10,
    /**
     * How much of a region the rack may claim perpendicular to its reading axis
     * — a seat fixture, never the board. **[D]** `zone-geography.md` fixes the
     * rack's proportions but not its share of a slot; below these the rack would
     * out-compete the cards it exists to serve. A rack that cannot reach
     * {@link minU} inside its share digests instead of shrinking the board.
     */
    share: {
      /** Fraction of the receiver band's width (a vertical rack). */
      receiver: 0.2,
      /** Fraction of the far side's width the horizontal rack spends along it —
       * enough that the focused seat still draws a real rack at the 1180 px
       * tablet-landscape floor rather than digesting there. */
      far: 0.26,
      /** Fraction of a wing's **visible** width (a vertical rack). */
      wing: 0.35,
    },
    /** The pile-card width each rack variant asks for before fitting (§6). */
    nominal: { receiver: 96, far: 78, wing: 62 },
    /**
     * The digest button's **shaped sub-indicators** (§6.1, issue #582 §5).
     *
     * §6.1's digest form is "one rack button with four shaped sub-indicators".
     * The shipped button drew `attr(data-library) '\\A' attr(data-graveyard)
     * '\\A' attr(data-exile)` — a column of bare numbers in a featureless dark
     * rectangle, with nothing saying which number was which zone, and the
     * command count dropped entirely. A rack that has lost every zone's
     * identity has stopped being a rack; the numbers are the one part of a pile
     * that is NOT its identity.
     *
     * So the button holds a grid of small chips, one per zone anchor, each
     * carrying that zone's own material and silhouette from the full rack (the
     * library's card back, the graveyard's ash, exile's oversized glass pane,
     * command's gold rule) with its count on it. Two columns, because a
     * two-by-two block reads as one object at this size and a four-tall column
     * reads as a list — and because it is the same block whichever axis the
     * seat's rack runs on, which is what makes both seats' digests one
     * component (§2.5 keeps the *drawn* rack's axis).
     */
    digest: {
      /** One sub-indicator's drawn box. */
      chip: { w: 24, h: 28 },
      /** Gap between chips, both axes. */
      gap: 3,
      /** Inset from the button's edge to the grid. */
      pad: 4,
      /** Chips per row. */
      columns: 2,
    },
  },
  /**
   * The seat **identity cluster** (`docs/design/seat-identity.md` §1, §2, §6).
   * Every offset is expressed in `D` — the portrait medallion's outer rim
   * diameter — because §1.1 fixes one scale unit per cluster and nothing inside
   * it scales independently. The origin of every offset is the portrait centre.
   */
  cluster: {
    /** The §1.1 rung ladder: `D` in px per variant. Every value clears 44 px. */
    d: { local: 112, focused: 96, wing: 76, compact: 60, minimal: 48 },
    /** `D` may never exceed this fraction of the plane's width… */
    maxWidthFrac: 0.14,
    /** …nor of its height, so a phone never stages a cluster it cannot hold. */
    maxHeightFrac: 0.2,
    /** Total rim band, in `D` (§1.2 el. 1: hairline + brushed band + hairline). */
    rim: 0.065,
    /** Outer radius of the priority double ring, in `D` (§6.1). */
    priorityOuter: 0.625,
    /** Life medallion: diameter and centre offset, in `D` (§1.2 el. 2). */
    life: { d: 0.55, cy: 0.5 },
    /** The minimal rung's life medallion, slung at 7–8 o'clock (§2). */
    minimalLife: { d: 0.42, cx: -0.38, cy: 0.38 },
    /** Nameplate geometry and its text fitting (§1.2 el. 4/5, §7 long name). */
    plate: {
      /** Plate height, in `D`. */
      h: 0.35,
      /** Shortest plate the variant table still draws, in `D`. */
      minLen: 1.05,
      /** Longest plate before the name truncates, in `D` (§7). */
      maxLen: 2.4,
      /** Text inset from the chevron tip, in `D` (§1.2 el. 5). */
      inset: 0.19,
      /**
       * Nominal advance of one letterspaced small-caps grapheme, in `D`. jsdom
       * measures no text, and the reconciler is pure, so the plate's length is
       * a deterministic function of the grapheme count rather than of a layout
       * pass. Chosen so {@link maxGraphemes} exactly fills {@link maxLen}.
       */
      advance: 0.155,
      /** Clear gap between the portrait rim and the plate's inboard end, in `D`. */
      gap: 0.72,
      /** Centre offset of a **below**-mode plate, in `D` (see `cluster.ts`). */
      belowCy: 1.05,
      /** Graphemes a full-length plate holds — §7's 8 + ellipsis + 4. */
      maxGraphemes: 13,
      /** Graphemes kept before the middle ellipsis (§7). */
      headGraphemes: 8,
      /** Graphemes kept after the middle ellipsis (§7). */
      tailGraphemes: 4,
    },
    /** Identity gem: diagonal and centre offset, in `D` (§1.2 el. 6). */
    gem: { s: 0.19, cx: 0.62 },
    /** Hand pip hexagon, in `D` (§1.2 el. 7). */
    pip: { w: 0.38, h: 0.42, cx: 0.65 },
    /** The compact rung's under-slung shield tab, in `D` (§1.2 el. 9). */
    tab: { w: 0.48, h: 0.52, cy: 0.75 },
    /** The status rail arc: medallion diameter, radius, and step (§1.2 el. 10). */
    rail: { d: 0.28, radius: 1.15, stepDeg: 26, startDeg: 60 },
    /** How many rail medallions a rung draws before the `+N` overflow (§7). */
    railCap: { local: 2, focused: 2, wing: 1, compact: 1, minimal: 0 },
  },
  /** Compact change-of-kind staging (rung 5, phone portrait, 3+ players). */
  compact: {
    /** The receiver's band height, as a fraction of H. */
    receiverH: 0.4,
    /** The focused opponent's drawn board. */
    far: { x: 0.06, y: 8, w: 0.88, h: 0.3 },
    /** Summary tiles: ≥ minHit tall, stacked below the focused board. */
    tile: { x: 0.06, w: 0.5, h: 48, gap: 8, topGap: 12, stripGap: 8 },
  },
} as const;

/**
 * Whether the viewport takes the compact change-of-kind staging branch (the
 * compact change-of-kind at 3+ players, layout-model rung 5): portrait
 * orientation (phones), or a landscape viewport below the tablet-landscape
 * geometry floor (`compactFloorWidth`) — the width below which full multiplayer
 * anatomy no longer fits and multiplayer changes kind. Tablet landscape at the
 * floor (1180×820) and every desktop geometry stay on full desktop staging.
 */
export function isCompactGeometry(viewport: PlaneViewport): boolean {
  return viewport.height > viewport.width || viewport.width < PLANE.compactFloorWidth;
}

/** A rect inset by `by` on every side (clamped to non-negative dimensions). */
export function insetRect(rect: Rect, by: number): Rect {
  return {
    x: rect.x + by,
    y: rect.y + by,
    w: Math.max(0, rect.w - 2 * by),
    h: Math.max(0, rect.h - 2 * by),
  };
}

/**
 * Whether a rect lies inside the seat envelope ({@link PLANE.envelope}): the
 * part of it outside the focal core must sit inside the flank band. Mirrors
 * `environment/zones.ts`'s `withinSeatEnvelope`, in plane pixels.
 */
export function withinEnvelope(rect: Rect, viewport: PlaneViewport): boolean {
  const { coreX, flankTop, flankBottom } = PLANE.envelope;
  const left = viewport.width * coreX;
  const right = viewport.width * (1 - coreX);
  const escapes = rect.x < left - 1e-9 || rect.x + rect.w > right + 1e-9;
  if (!escapes) return true;
  return (
    rect.y >= viewport.height * flankTop - 1e-9 &&
    rect.y + rect.h <= viewport.height * flankBottom + 1e-9
  );
}

/**
 * Clamp a staged chrome rect into the seat envelope and onto the plane — the
 * guard that keeps a crest cluster out of the theme's prop pockets (Zone C) and
 * off the canvas edge. A rect whose horizontal span leaves the focal core is
 * additionally pinned inside the flank band; one inside the core only has to
 * stay on the plane.
 */
export function clampToEnvelope(rect: Rect, viewport: PlaneViewport): Rect {
  const { coreX, flankTop, flankBottom } = PLANE.envelope;
  const clamp = (value: number, lo: number, hi: number): number =>
    Math.max(lo, Math.min(hi, value));
  const x = clamp(rect.x, 4, Math.max(4, viewport.width - rect.w - 4));
  const escapes =
    x < viewport.width * coreX - 1e-9 || x + rect.w > viewport.width * (1 - coreX) + 1e-9;
  const lo = escapes ? viewport.height * flankTop : 4;
  const hi = escapes
    ? viewport.height * flankBottom - rect.h
    : Math.max(4, viewport.height - rect.h - 4);
  return { ...rect, x, y: clamp(rect.y, lo, Math.max(lo, hi)) };
}

/**
 * The interactive hotspot for a drawn rect: the rect grown (centered) to the
 * 44 px floor in any dimension that falls short. The drawn footprint never
 * shrinks — only the hit target grows.
 */
export function hitRectFor(rect: Rect, min: number = PLANE.minHit): Rect {
  const w = Math.max(rect.w, min);
  const h = Math.max(rect.h, min);
  return {
    x: rect.x - Math.floor((w - rect.w) / 2),
    y: rect.y - Math.floor((h - rect.h) / 2),
    w,
    h,
  };
}
