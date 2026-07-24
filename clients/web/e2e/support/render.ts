/**
 * Render-path probes — the canary half of the smoke suite (issue #279).
 *
 * The bug this suite exists for shipped because nothing rendered the client in a
 * real browser: jsdom has no WebGL, so the canvas-owning component no-ops there
 * and a detached canvas looks identical to a healthy one. These probes read the
 * two things a green unit suite cannot see:
 *
 * - {@link readCanvasHealth} — the WebGL surface is *attached to the live
 *   document with a live context*. React StrictMode double-invokes effects in a
 *   development build; a mount shape that does not survive that leaves the
 *   container empty and the table without its effects surface. That is precisely
 *   the shape of the regression, and `present === false` is precisely what it
 *   looks like from here.
 * - {@link readPlaneHealth} — the scene plane is not blank: it holds laid-out,
 *   hit-testable card elements with real boxes. A plane that mounts but paints
 *   nothing is a blank table, and structurally indistinguishable from a working
 *   one until you look at the boxes.
 *
 * Both are read-only DOM observations. Neither needs a pixel diff, which would
 * be brittle across renderers and GPUs (ADR 0011 keeps baselines secondary).
 */
import type { Page } from '@playwright/test';

/** What the effects surface's WebGL canvas looks like from the document's side. */
export interface CanvasHealth {
  /** Whether a `<canvas>` exists inside the effects-surface container at all. */
  present: boolean;
  /** Whether that canvas is attached to the live document. */
  connected: boolean;
  /** Backing-store width in device pixels. */
  backingWidth: number;
  /** Backing-store height in device pixels. */
  backingHeight: number;
  /** Laid-out CSS width. */
  cssWidth: number;
  /** Laid-out CSS height. */
  cssHeight: number;
  /** Which WebGL context the canvas actually holds, or `null` for none. */
  context: 'webgl2' | 'webgl' | null;
  /** The context's drawing-buffer width; `0` when there is no context. */
  drawingBufferWidth: number;
}

/** What the DOM scene plane looks like: mounted, laid out, and populated. */
export interface PlaneHealth {
  /** Whether the plane host element exists. */
  present: boolean;
  /** The plane host's laid-out width. */
  hostWidth: number;
  /** The plane host's laid-out height. */
  hostHeight: number;
  /** Entity ids of card elements with a real (non-degenerate) box. */
  visibleEntityIds: string[];
  /** Total card elements in the plane, painted or not. */
  cardCount: number;
  /**
   * Whether hit-testing the plane's centre lands inside the match table — a
   * blank, detached, or zero-sized scene fails this even when elements exist in
   * the tree.
   */
  centreHitsTable: boolean;
}

/** Probe the effects surface's canvas. See {@link CanvasHealth}. */
export async function readCanvasHealth(page: Page): Promise<CanvasHealth> {
  return page.evaluate(() => {
    const host = document.querySelector('[data-testid="effects-surface"]');
    const canvas = host?.querySelector('canvas') ?? null;
    if (canvas === null) {
      return {
        present: false,
        connected: false,
        backingWidth: 0,
        backingHeight: 0,
        cssWidth: 0,
        cssHeight: 0,
        context: null,
        drawingBufferWidth: 0,
      };
    }
    const rect = canvas.getBoundingClientRect();
    // `getContext` returns the context the canvas already holds; asking for the
    // wrong flavour yields null, so try both and report which one is live.
    const gl2 = canvas.getContext('webgl2');
    const gl = gl2 ?? canvas.getContext('webgl');
    return {
      present: true,
      connected: canvas.isConnected,
      backingWidth: canvas.width,
      backingHeight: canvas.height,
      cssWidth: rect.width,
      cssHeight: rect.height,
      context: gl === null ? null : gl2 === null ? ('webgl' as const) : ('webgl2' as const),
      drawingBufferWidth: gl?.drawingBufferWidth ?? 0,
    };
  });
}

/** Probe the DOM scene plane. See {@link PlaneHealth}. */
export async function readPlaneHealth(page: Page): Promise<PlaneHealth> {
  return page.evaluate(() => {
    const host = document.querySelector('[data-testid="live-2-5d-plane"]');
    const plane = document.querySelector('[data-testid="live-plane-dom"]');
    if (host === null || plane === null) {
      return {
        present: false,
        hostWidth: 0,
        hostHeight: 0,
        visibleEntityIds: [],
        cardCount: 0,
        centreHitsTable: false,
      };
    }
    const hostRect = host.getBoundingClientRect();
    const cards = Array.from(plane.querySelectorAll<HTMLElement>('[data-entity-id]'));
    const visibleEntityIds = cards
      .filter((card) => {
        const rect = card.getBoundingClientRect();
        return rect.width > 8 && rect.height > 8;
      })
      .map((card) => card.dataset.entityId ?? '');
    const centre = document.elementFromPoint(
      hostRect.x + hostRect.width / 2,
      hostRect.y + hostRect.height / 2,
    );
    return {
      present: true,
      hostWidth: hostRect.width,
      hostHeight: hostRect.height,
      visibleEntityIds,
      cardCount: cards.length,
      centreHitsTable:
        centre !== null && centre.closest('[data-testid="live-match-table"]') !== null,
    };
  });
}
