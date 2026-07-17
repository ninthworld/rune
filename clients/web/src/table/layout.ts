/**
 * The full-bleed tabletop shell layout (issue #295).
 *
 * ONE pure function — {@link layout} — maps measured viewport geometry to the set
 * of region rects that position BOTH the DOM chrome and the Pixi battlefield scene
 * (ADR 0003: one `layout()` positions both renderers). It keys ONLY on measured
 * geometry (width, height, aspect) and detected input capability (pointer
 * precision) — never on a user-agent string or a desktop/mobile breakpoint list
 * (ui-requirements §Layout and devices). Portrait, landscape, 16:9, and ultrawide
 * all resolve from this one function.
 *
 * The shell is the procedural tabletop `docs/design/ui-design-notes.md` (§Tabletop
 * shell) specifies: the battlefield owns the center and most of the viewport; the
 * chrome docks around its edges and never displaces the board into a scrolled
 * document flow. Regions NEVER reorder between states — they only scale, condense,
 * or collapse — so play never requires visually re-locating a control.
 *
 * Region model
 * ------------
 * Two layers (see {@link Region.layer}):
 *
 * - **docked** — carves the battlefield: `indicator` and `opponentHud` stack across
 *   the top, `rail` docks on the right edge, and `battlefield` is the large central
 *   remainder the Pixi scene sizes to. These four are pairwise disjoint.
 * - **floating** — overlays the battlefield's edges without shrinking it (the board
 *   still owns that space): `localDock` (bottom-left), `hand` (bottom-center anchor
 *   band), and `tray` (the action tray, floating just above the hand). On narrow
 *   geometry the `rail` collapses to a floating badge instead of docking.
 *
 * The battlefield rect is the one the scene consumes: its width is the wrap budget
 * fed to `buildTableScene`, which returns a scene no wider than that — so the board
 * never scrolls horizontally at any supported geometry (it grows downward instead,
 * scrolling vertically inside the region if a huge board overflows).
 *
 * Downstream issues (#296 HUDs, #297 indicator, #298 tray/prompts, #299 rail, #301
 * focus) consume these stable region identities plus the scene's reported card/lane
 * rects (`scene.bands[].rect`, `scene.handRegion.rect`) and the `mode` signal.
 */
import type { Rect } from './scene';

/** Presentation mode (issue #267): overview vs focus differ in emphasis/density
 * only — never in region order or placement, so the geometry here is mode-invariant. */
export type Mode = 'overview' | 'focus';

/** Pointer precision the environment offers (ui-requirements §Input capability
 * model). Capabilities, not devices: a coarse pointer widens min affordances but
 * never changes region order. */
export type Pointer = 'coarse' | 'fine';

/** Stable region identities. Downstream work anchors to these names, never to
 * incidental DOM structure. */
export type RegionId =
  'indicator' | 'opponentHud' | 'battlefield' | 'rail' | 'localDock' | 'hand' | 'tray';

/** Whether a region carves the battlefield (`docked`) or overlays it (`floating`). */
export type RegionLayer = 'docked' | 'floating';

/** One positioned region: its rect (viewport px) and which layer it sits on. */
export interface Region {
  id: RegionId;
  rect: Rect;
  layer: RegionLayer;
}

/** Measured viewport geometry plus detected input capability. */
export interface Viewport {
  width: number;
  height: number;
  /** Pointer precision, if detected; defaults to `fine` when absent (SSR/tests). */
  pointer?: Pointer;
}

/** The computed shell: the region rects plus the derived geometry signals the DOM
 * and scene read. */
export interface TableLayout {
  /** The (clamped) viewport the layout was computed for. */
  viewport: Required<Viewport>;
  /** The presentation mode echoed through (geometry is mode-invariant). */
  mode: Mode;
  /** The seat count the HUD strip reflowed for. */
  playerCount: number;
  /** width / height. */
  aspect: number;
  /** Coarse orientation derived from the aspect (never from a device list). */
  orientation: 'portrait' | 'landscape';
  /** Whether the stack/activity rail collapsed to a floating badge (narrow width). */
  railCollapsed: boolean;
  /** Every region, keyed by its stable identity. */
  regions: Record<RegionId, Region>;
}

/**
 * Layout constants (viewport px / fractions). Fractional caps are what guarantee
 * the battlefield stays the majority surface at every geometry: the top chrome
 * never exceeds {@link topChromeMaxFrac} of the height and the rail never exceeds
 * {@link railMaxFrac} of the width, so the central battlefield always keeps well
 * over half the viewport.
 */
const L = {
  pad: 8,
  /** Compact turn/phase indicator bar height (top). */
  indicatorH: 48,
  /** The indicator never eats more than this fraction of a short viewport. */
  indicatorMaxFrac: 0.12,
  /** One row of opponent HUD tiles. */
  hudRowH: 76,
  /** Approx min tile pitch used to reflow the HUD strip by player count. */
  hudTilePitch: 160,
  /** Top chrome (indicator + HUD) never exceeds this fraction of the height. */
  topChromeMaxFrac: 0.3,
  /** Stack/activity rail: minimum docked width and its fractional cap. */
  railMin: 240,
  railMaxFrac: 0.26,
  railPreferredFrac: 0.2,
  /** Below this width the rail collapses to a floating badge (never docks). */
  railCollapseBelow: 700,
  /** The collapsed rail badge is a single touch target. */
  railBadge: 44,
  /** Floating action tray height (min one touch target + padding). */
  trayH: 60,
  /** Local player dock (bottom-left) nominal size and width cap. */
  dockW: 260,
  dockH: 96,
  dockMaxFrac: 0.32,
  /** Nominal hand anchor band height (the hand is drawn by the scene inside the
   * battlefield; this rect is the stable bottom-center anchor downstream #298 uses). */
  handBandH: 200,
  /** A coarse pointer widens the shortest floating strips to keep 44px targets. */
  coarseTrayH: 68,
} as const;

/** The viewport the layout falls back to where there is no `window` (SSR/tests). */
export const DEFAULT_VIEWPORT: Required<Viewport> = {
  width: 1280,
  height: 800,
  pointer: 'fine',
};

/** Clamp helper (min ≤ value ≤ max). */
function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

/** Whether two rects overlap on a positive area (touching edges do not count). */
export function rectsOverlap(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

/** A rect's area (0 for a degenerate rect). */
export function rectArea(rect: Rect): number {
  return Math.max(0, rect.w) * Math.max(0, rect.h);
}

/**
 * Position every shell region for a measured viewport.
 *
 * Pure and total: the same `(viewport, mode, playerCount)` always yields the same
 * rects, and every rect stays inside the viewport. `mode` is echoed through but
 * does NOT move any region (overview/focus differ in density/emphasis only), so a
 * caller can re-lay-out on a mode flip and see identical placement.
 */
export function layout(viewport: Viewport, mode: Mode, playerCount: number): TableLayout {
  // Guard against zero/negative geometry (a hidden or not-yet-measured container)
  // so every derived rect stays well-formed.
  const width = Math.max(1, Math.floor(viewport.width));
  const height = Math.max(1, Math.floor(viewport.height));
  const pointer: Pointer = viewport.pointer ?? 'fine';
  const seats = Math.max(1, Math.floor(playerCount));

  const aspect = width / height;
  const orientation = width >= height ? 'landscape' : 'portrait';

  // ── Top chrome: a compact indicator bar over the opponent HUD strip. The HUD
  // reflows by seat count (wide tile at 2p → grid at 8p), but the whole top band is
  // hard-capped at `topChromeMaxFrac` of the height so it never crowds out the board
  // (and, at a degenerate tiny viewport, never overruns it into a negative board).
  const indicatorH = Math.min(L.indicatorH, Math.floor(height * L.indicatorMaxFrac));
  const topCap = Math.floor(height * L.topChromeMaxFrac);
  const tilesPerRow = Math.max(1, Math.floor(width / L.hudTilePitch));
  const hudRows = Math.max(1, Math.ceil(seats / tilesPerRow));
  const hudH = Math.min(hudRows * L.hudRowH, Math.max(0, topCap - indicatorH));
  const topH = indicatorH + hudH;

  // ── Rail: docks on the right edge when wide; collapses to a floating badge when
  // narrow so the board keeps the width.
  const railCollapsed = width < L.railCollapseBelow;
  const railW = railCollapsed
    ? 0
    : clamp(Math.floor(width * L.railPreferredFrac), L.railMin, Math.floor(width * L.railMaxFrac));

  // ── Battlefield: the central remainder — everything below the top chrome and
  // left of the docked rail. This is the rect the Pixi scene sizes to.
  const battlefield: Rect = { x: 0, y: topH, w: width - railW, h: height - topH };

  const indicator: Rect = { x: 0, y: 0, w: width, h: indicatorH };
  const opponentHud: Rect = { x: 0, y: indicatorH, w: width, h: hudH };

  const rail: Region = railCollapsed
    ? {
        id: 'rail',
        layer: 'floating',
        rect: {
          x: width - L.railBadge - L.pad,
          y: topH + L.pad,
          w: L.railBadge,
          h: L.railBadge,
        },
      }
    : {
        id: 'rail',
        layer: 'docked',
        rect: { x: width - railW, y: topH, w: railW, h: height - topH },
      };

  // ── Floating bottom chrome, overlaying the battlefield's lower edge (the board
  // still owns that space; these sit above it).
  const dockW = Math.min(L.dockW, Math.floor(width * L.dockMaxFrac));
  const dockH = Math.min(L.dockH, Math.floor(battlefield.h * 0.5));
  const localDock: Rect = {
    x: L.pad,
    y: height - dockH - L.pad,
    w: dockW,
    h: dockH,
  };

  // The hand's stable bottom-center anchor band (the scene draws the actual hand
  // cards inside the battlefield; this rect is what downstream #298 anchors to).
  const handBandH = Math.min(L.handBandH, Math.floor(battlefield.h * 0.5));
  const hand: Rect = {
    x: 0,
    y: height - handBandH,
    w: battlefield.w,
    h: handBandH,
  };

  // The action tray floats just above the hand band, clearing the local dock.
  const trayH = pointer === 'coarse' ? L.coarseTrayH : L.trayH;
  const trayX = dockW + L.pad * 2;
  const tray: Rect = {
    x: trayX,
    y: Math.max(topH, hand.y - trayH - L.pad),
    w: Math.max(0, battlefield.w - trayX - L.pad),
    h: trayH,
  };

  return {
    viewport: { width, height, pointer },
    mode,
    playerCount: seats,
    aspect,
    orientation,
    railCollapsed,
    regions: {
      indicator: { id: 'indicator', layer: 'docked', rect: indicator },
      opponentHud: { id: 'opponentHud', layer: 'docked', rect: opponentHud },
      battlefield: { id: 'battlefield', layer: 'docked', rect: battlefield },
      rail,
      localDock: { id: 'localDock', layer: 'floating', rect: localDock },
      hand: { id: 'hand', layer: 'floating', rect: hand },
      tray: { id: 'tray', layer: 'floating', rect: tray },
    },
  };
}

/** The battlefield width the scene should wrap within, for a given layout. A thin
 * convenience so callers don't reach into the region map for the one value the
 * Pixi scene consumes. */
export function battlefieldWidth(computed: TableLayout): number {
  return computed.regions.battlefield.rect.w;
}
