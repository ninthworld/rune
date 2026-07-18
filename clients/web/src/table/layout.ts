/**
 * The fixed-shell tabletop layout (ADR 0023; `docs/design/ui-blueprint.md`).
 *
 * ONE pure function — {@link layout} — carves the measured viewport into the
 * blueprint's fixed anatomy: top status bar, opponent panel(s), the receiver's
 * battlefield panel, a right rail (stack + activity), and a bottom shell owning
 * the receiver's identity, piles, hand, and the single action dock. **Nothing
 * floats over anything**: every region has a permanent, disjoint home, so nothing
 * can overlap or clip *by construction* — this replaces the floating-chrome model
 * (dock/hand/tray overlaying the board) that ADR 0023 retires.
 *
 * The function keys ONLY on measured geometry (width, height) and detected input
 * capability (pointer precision) — never a user-agent string or a device list
 * (ui-requirements §Layout and devices). Geometry breakpoints change
 * **composition, not anatomy**:
 *
 * - `full` — the laptop/tablet anatomy (`prototypes/ui-table-4p-laptop-v1.html`,
 *   `…-tablet-v1.html`): opponents across the top, your battlefield full width,
 *   rail on the right, bottom shell of identity panel · prompt strip + hand ·
 *   action dock.
 * - `compact` — the phone-portrait change of kind (`…-duel-phone-v1.html`): the
 *   top bar compresses to a turn pill + phase dots with stack/log as chips opening
 *   sheets, panels stack vertically, and the bottom shell becomes prompt strip →
 *   fixed action bar → hand fan → identity strip, all in thumb reach.
 *
 * Alongside the viewport-space chrome regions, the layout emits the
 * {@link SceneGeometry} the scene builder consumes: the per-player panel frames
 * and the hand area, in canvas-local coordinates (ADR 0003: one layout positions
 * both renderers).
 */
import type { Rect, SceneGeometry, PanelFrame, SurfaceTier } from './scene';

/** Presentation mode (issue #267): overview vs focus differ in emphasis/density
 * only — never in region order or placement, so the geometry here is mode-invariant. */
export type Mode = 'overview' | 'focus';

/** Pointer precision the environment offers (ui-requirements §Input capability
 * model). Capabilities, not devices: a coarse pointer widens min affordances but
 * never changes region order. */
export type Pointer = 'coarse' | 'fine';

/** Which composition the geometry resolved (see file header). Composition changes
 * how regions condense — never the anatomy's ownership rules (ADR 0023). */
export type Composition = 'full' | 'compact';

/** Stable region identities. Downstream work anchors to these names, never to
 * incidental DOM structure. */
export type RegionId =
  'topBar' | 'canvas' | 'rail' | 'mePanel' | 'promptStrip' | 'dock' | 'handPanel';

/** One positioned region (viewport px). Every region is carved — no floating layer
 * exists in the fixed shell (ADR 0023). */
export interface Region {
  id: RegionId;
  rect: Rect;
}

/** Measured viewport geometry plus detected input capability. */
export interface Viewport {
  width: number;
  height: number;
  /** Pointer precision, if detected; defaults to `fine` when absent (SSR/tests). */
  pointer?: Pointer;
}

/** The computed shell: carved chrome regions plus the scene geometry. */
export interface TableLayout {
  /** The (clamped) viewport the layout was computed for. */
  viewport: Required<Viewport>;
  /** The seat count the panels reflowed for. */
  playerCount: number;
  /** width / height. */
  aspect: number;
  /** Coarse orientation derived from the aspect (never from a device list). */
  orientation: 'portrait' | 'landscape';
  /** The resolved composition (full anatomy vs the phone change of kind). */
  composition: Composition;
  /** Every chrome region, keyed by its stable identity. On `compact` the rail is
   * a zero-width rect (its content lives behind top-bar chips as sheets); the
   * region identity itself never disappears (chrome never reorders). */
  regions: Record<RegionId, Region>;
  /** The card-surface geometry the scene builder consumes (canvas-local). */
  scene: SceneGeometry;
}

/**
 * Layout constants (viewport px). The fractional caps guarantee the battlefield
 * panels stay the majority surface at every geometry.
 */
const L = {
  pad: 8,
  gap: 8,
  /** Top status bar height (compact composition condenses it). */
  topBarH: 44,
  topBarCompactH: 40,
  /** Right rail width bounds (stack + activity). */
  railMin: 236,
  railMax: 312,
  railFrac: 0.19,
  /** Below this width the composition changes kind (phone portrait). */
  compactBelow: 720,
  /** Panel header strip (crest · name · meta). */
  panelHeaderH: 32,
  /** Opponent panel piles column width; the local panel has none (its piles live
   * in the bottom shell's identity panel, per the blueprint). */
  pilesColW: 60,
  /** Bottom shell: identity panel and action dock widths (full composition). */
  mePanelW: 240,
  dockW: 224,
  /** Prompt strip height (the pending question in words). */
  promptStripH: 32,
  /** Hand card area height: the hand tier plus breathing room. */
  handAreaH: 162,
  /** Compact bottom shell strips. */
  compactActionBarH: 52,
  compactHandH: 158,
  compactMeStripH: 28,
  /** Opponent row height bounds (full composition). */
  oppRowMin: 176,
  oppRowFrac: 0.44,
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

/** Carve a panel rect into its header / content / piles frames (canvas coords). */
function frame(rect: Rect, pilesW: number): PanelFrame {
  const header: Rect = { x: rect.x, y: rect.y, w: rect.w, h: Math.min(L.panelHeaderH, rect.h) };
  const piles: Rect =
    pilesW > 0
      ? {
          x: rect.x + rect.w - pilesW,
          y: rect.y + header.h,
          w: pilesW,
          h: Math.max(0, rect.h - header.h),
        }
      : { x: rect.x + rect.w, y: rect.y + header.h, w: 0, h: 0 };
  const content: Rect = {
    x: rect.x + L.pad,
    y: rect.y + header.h + 4,
    w: Math.max(0, rect.w - L.pad * 2 - piles.w),
    h: Math.max(0, rect.h - header.h - 4 - L.pad),
  };
  return { rect, header, content, piles };
}

/**
 * The per-surface card tiers for a geometry (the blueprint's tier ladder: the
 * receiver's battlefield is a step larger than the opponents'; a duel at full
 * width earns the largest board tiers). The density ladder may step these DOWN
 * per panel (scene builder) — never up.
 */
function tiersFor(
  composition: Composition,
  opponents: number,
): { you: SurfaceTier; opp: SurfaceTier } {
  if (composition === 'full' && opponents <= 1) return { you: 'field', opp: 'support' };
  return { you: 'support', opp: 'mini' };
}

/**
 * Position every shell region for a measured viewport.
 *
 * Pure and total: the same `(viewport, playerCount)` always yields the same rects,
 * every chrome region stays inside the viewport, and the carved regions are
 * pairwise disjoint (the canvas region underlies the bottom-shell chrome regions,
 * but no chrome region overlaps another and card areas never sit under chrome).
 */
export function layout(viewport: Viewport, playerCount: number): TableLayout {
  const width = Math.max(1, Math.floor(viewport.width));
  const height = Math.max(1, Math.floor(viewport.height));
  const pointer: Pointer = viewport.pointer ?? 'fine';
  const seats = Math.max(1, Math.floor(playerCount));
  const opponents = Math.max(1, seats - 1);

  const aspect = width / height;
  const orientation = width >= height ? 'landscape' : 'portrait';
  const composition: Composition = width < L.compactBelow ? 'compact' : 'full';

  return composition === 'full'
    ? fullLayout(width, height, pointer, seats, opponents, aspect, orientation)
    : compactLayout(width, height, pointer, seats, opponents, aspect, orientation);
}

/** The full (laptop/tablet) anatomy. */
function fullLayout(
  width: number,
  height: number,
  pointer: Pointer,
  seats: number,
  opponents: number,
  aspect: number,
  orientation: 'portrait' | 'landscape',
): TableLayout {
  const pad = L.pad;
  const gap = L.gap;
  const topBar: Rect = { x: pad, y: pad, w: width - pad * 2, h: L.topBarH };

  const railW = clamp(
    Math.round(width * L.railFrac),
    L.railMin,
    Math.min(L.railMax, Math.floor(width * 0.3)),
  );
  const contentTop = topBar.y + topBar.h + gap;
  const rail: Rect = {
    x: width - pad - railW,
    y: contentTop,
    w: railW,
    h: Math.max(0, height - contentTop - pad),
  };

  // Left column: board panels above the bottom shell.
  const leftW = Math.max(1, width - pad * 2 - railW - gap);
  const bottomH = Math.min(L.promptStripH + L.handAreaH, Math.floor((height - contentTop) * 0.4));
  const bottomY = height - pad - bottomH;
  const boardH = Math.max(0, bottomY - gap - contentTop);

  // Bottom shell: identity panel · hand panel (prompt strip on top) · action dock.
  const mePanelW = Math.min(L.mePanelW, Math.floor(leftW * 0.24));
  const dockW = Math.min(L.dockW, Math.floor(leftW * 0.22));
  const mePanel: Rect = { x: pad, y: bottomY, w: mePanelW, h: bottomH };
  const dock: Rect = { x: pad + leftW - dockW, y: bottomY, w: dockW, h: bottomH };
  const handPanel: Rect = {
    x: mePanel.x + mePanel.w + gap,
    y: bottomY,
    w: Math.max(0, leftW - mePanelW - dockW - gap * 2),
    h: bottomH,
  };
  const promptStrip: Rect = { x: handPanel.x, y: handPanel.y, w: handPanel.w, h: L.promptStripH };

  // The canvas underlies the whole left column below the top bar; panel frames and
  // the hand area are carved inside it (canvas-local coordinates).
  const canvas: Rect = {
    x: pad,
    y: contentTop,
    w: leftW,
    h: Math.max(0, height - contentTop - pad),
  };
  const toCanvas = (r: Rect): Rect => ({ x: r.x - canvas.x, y: r.y - canvas.y, w: r.w, h: r.h });

  // Opponent panels: one row up to 3 across; two rows beyond. A duel gets one wide
  // panel. Panels split the row evenly — composition, not reordering.
  const oppRows = opponents <= 3 ? 1 : 2;
  const perRow = Math.ceil(opponents / oppRows);
  const oppH = Math.max(L.oppRowMin, Math.floor(boardH * L.oppRowFrac));
  const oppRowH = oppRows === 1 ? oppH : Math.floor((oppH * 1.6) / 2);
  const oppAreaH = oppRows === 1 ? oppH : oppRowH * 2 + gap;
  const opponentFrames: PanelFrame[] = [];
  for (let i = 0; i < opponents; i += 1) {
    const row = Math.floor(i / perRow);
    const col = i % perRow;
    const inRow = row === oppRows - 1 ? opponents - perRow * (oppRows - 1) : perRow;
    const w = Math.floor((leftW - gap * (inRow - 1)) / inRow);
    const rect: Rect = {
      x: pad + col * (w + gap),
      y: contentTop + row * (oppRowH + gap),
      w,
      h: oppRows === 1 ? oppH : oppRowH,
    };
    opponentFrames.push(frame(toCanvas(rect), L.pilesColW));
  }

  const youRect: Rect = {
    x: pad,
    y: contentTop + oppAreaH + gap,
    w: leftW,
    h: Math.max(0, boardH - oppAreaH - gap),
  };
  // The local panel has no piles column: your piles live in the identity panel
  // (bottom shell), at the largest pile tier, per the blueprint.
  const you = frame(toCanvas(youRect), 0);

  const handRect: Rect = {
    x: handPanel.x + pad,
    y: promptStrip.y + promptStrip.h + 4,
    w: Math.max(0, handPanel.w - pad * 2),
    h: Math.max(0, handPanel.h - promptStrip.h - 4 - pad),
  };

  const scene: SceneGeometry = {
    width: canvas.w,
    height: canvas.h,
    opponents: opponentFrames,
    you,
    hand: toCanvas(handRect),
    tiers: tiersFor('full', opponents),
    handFan: false,
  };

  return {
    viewport: { width, height, pointer },
    playerCount: seats,
    aspect,
    orientation,
    composition: 'full',
    regions: {
      topBar: { id: 'topBar', rect: topBar },
      canvas: { id: 'canvas', rect: canvas },
      rail: { id: 'rail', rect: rail },
      mePanel: { id: 'mePanel', rect: mePanel },
      promptStrip: { id: 'promptStrip', rect: promptStrip },
      dock: { id: 'dock', rect: dock },
      handPanel: { id: 'handPanel', rect: handPanel },
    },
    scene,
  };
}

/** The compact (phone-portrait) change of kind. Same anatomy ownership: the top
 * bar owns status (as a pill + dots + chips), the panels own the boards, the
 * bottom shell owns prompt → action bar → hand fan → identity strip. */
function compactLayout(
  width: number,
  height: number,
  pointer: Pointer,
  seats: number,
  opponents: number,
  aspect: number,
  orientation: 'portrait' | 'landscape',
): TableLayout {
  const pad = 6;
  const gap = 6;
  const topBar: Rect = { x: pad, y: pad, w: width - pad * 2, h: L.topBarCompactH };
  const contentTop = topBar.y + topBar.h + gap;

  // Bottom shell strips, thumb-reach: prompt strip, fixed action bar, hand fan,
  // identity strip. All interaction lives here; the top half is display.
  const meStrip: Rect = {
    x: pad,
    y: height - pad - L.compactMeStripH,
    w: width - pad * 2,
    h: L.compactMeStripH,
  };
  const handPanel: Rect = {
    x: pad,
    y: meStrip.y - gap - L.compactHandH,
    w: width - pad * 2,
    h: L.compactHandH,
  };
  const dock: Rect = {
    x: pad,
    y: handPanel.y - gap - L.compactActionBarH,
    w: width - pad * 2,
    h: L.compactActionBarH,
  };
  const promptStrip: Rect = {
    x: pad,
    y: dock.y - gap - L.promptStripH,
    w: width - pad * 2,
    h: L.promptStripH,
  };

  const boardH = Math.max(0, promptStrip.y - gap - contentTop);
  const canvas: Rect = {
    x: pad,
    y: contentTop,
    w: width - pad * 2,
    h: Math.max(0, height - contentTop - pad),
  };
  const toCanvas = (r: Rect): Rect => ({ x: r.x - canvas.x, y: r.y - canvas.y, w: r.w, h: r.h });

  // Panels stack vertically; the receiver's is slightly larger (mock: 5:6). With
  // several opponents each shares the opponent portion evenly.
  const youShare = 6 / (5 * opponents + 6);
  const youH = Math.floor(boardH * youShare);
  const oppH = Math.floor((boardH - youH - gap * opponents) / opponents);
  const opponentFrames: PanelFrame[] = [];
  for (let i = 0; i < opponents; i += 1) {
    const rect: Rect = {
      x: pad,
      y: contentTop + i * (oppH + gap),
      w: width - pad * 2,
      h: oppH,
    };
    opponentFrames.push(frame(toCanvas(rect), Math.min(L.pilesColW, 44)));
  }
  const youRect: Rect = {
    x: pad,
    y: contentTop + opponents * (oppH + gap),
    w: width - pad * 2,
    h: youH,
  };
  const you = frame(toCanvas(youRect), Math.min(L.pilesColW, 44));

  const handRect: Rect = {
    x: handPanel.x + pad,
    y: handPanel.y + 4,
    w: Math.max(0, handPanel.w - pad * 2),
    h: Math.max(0, handPanel.h - 4 - pad),
  };

  const scene: SceneGeometry = {
    width: canvas.w,
    height: canvas.h,
    opponents: opponentFrames,
    you,
    hand: toCanvas(handRect),
    tiers: tiersFor('compact', opponents),
    handFan: true,
  };

  // The rail region identity persists (chrome never reorders) but claims no space:
  // on compact the stack/log live behind top-bar chips that open sheets.
  const rail: Rect = { x: width - pad, y: contentTop, w: 0, h: 0 };

  return {
    viewport: { width, height, pointer },
    playerCount: seats,
    aspect,
    orientation,
    composition: 'compact',
    regions: {
      topBar: { id: 'topBar', rect: topBar },
      canvas: { id: 'canvas', rect: canvas },
      rail: { id: 'rail', rect: rail },
      mePanel: { id: 'mePanel', rect: meStrip },
      promptStrip: { id: 'promptStrip', rect: promptStrip },
      dock: { id: 'dock', rect: dock },
      handPanel: { id: 'handPanel', rect: handPanel },
    },
    scene,
  };
}
