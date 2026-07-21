import { describe, expect, it } from 'vitest';
import { buildTableScene } from './scene';
import { normalizeGameView } from '../wire';
import type { GameView } from '../protocol';
import {
  DEFAULT_VIEWPORT,
  layout,
  rectArea,
  rectsOverlap,
  type RegionId,
  type TableLayout,
  type Viewport,
} from './layout';

/**
 * The supported-geometry matrix the shell must resolve identically from: a
 * portrait phone, a small landscape window, a laptop, a 16:9 desktop, an
 * ultrawide, and a portrait tablet. Every one comes out of the one pure function.
 */
const GEOMETRIES: { name: string; viewport: Viewport }[] = [
  { name: 'portrait phone', viewport: { width: 390, height: 844, pointer: 'coarse' } },
  { name: 'small landscape', viewport: { width: 668, height: 375, pointer: 'coarse' } },
  { name: 'laptop', viewport: { width: 1280, height: 800, pointer: 'fine' } },
  { name: '16:9 desktop', viewport: { width: 1920, height: 1080, pointer: 'fine' } },
  { name: 'ultrawide', viewport: { width: 3440, height: 1440, pointer: 'fine' } },
  { name: 'tall portrait tablet', viewport: { width: 768, height: 1024, pointer: 'coarse' } },
];

const PLAYER_COUNTS = [2, 4, 8];

const ALL_REGIONS: RegionId[] = [
  'topBar',
  'canvas',
  'rail',
  'mePanel',
  'promptStrip',
  'dock',
  'handPanel',
];

/** The chrome regions that must never overlap one another. The canvas underlies
 * the bottom shell by design, and the prompt strip rides the hand panel's top
 * edge (full composition), so those two are excluded from the pairwise check and
 * asserted separately. */
function chromeRegions(computed: TableLayout) {
  return ALL_REGIONS.filter((id) => id !== 'canvas' && id !== 'promptStrip').map(
    (id) => computed.regions[id],
  );
}

/** A board with `perController` permanents each, for the no-sideways-scroll check. */
function boardView(controllers: string[], perController: number): GameView {
  const battlefield = controllers.flatMap((controller) =>
    Array.from({ length: perController }, (_, i) => ({
      id: `${controller}_perm_${i}`,
      controller,
      owner: controller,
      card: { id: `${controller}_perm_${i}`, name: `Servo ${i}`, type_line: 'Artifact Creature' },
    })),
  );
  return normalizeGameView({
    you: controllers[0],
    my_hand: [],
    opponents: controllers.slice(1).map((player_id) => ({
      player_id,
      hand_size: 0,
      life: 20,
      library_size: 40,
    })),
    battlefield,
    phase: 'precombat_main',
    valid_actions: [],
  });
}

describe('layout region geometry (ADR 0023 fixed shell)', () => {
  for (const { name, viewport } of GEOMETRIES) {
    for (const playerCount of PLAYER_COUNTS) {
      describe(`${name} @ ${playerCount}p`, () => {
        const computed = layout(viewport, playerCount);

        it('carves every region of the anatomy, keyed by its stable identity', () => {
          for (const id of ALL_REGIONS) {
            expect(computed.regions[id]).toBeDefined();
            expect(computed.regions[id].id).toBe(id);
          }
        });

        it('keeps every region inside the viewport', () => {
          for (const region of Object.values(computed.regions)) {
            const { x, y, w, h } = region.rect;
            expect(x).toBeGreaterThanOrEqual(0);
            expect(y).toBeGreaterThanOrEqual(0);
            expect(w).toBeGreaterThanOrEqual(0);
            expect(h).toBeGreaterThanOrEqual(0);
            expect(x + w).toBeLessThanOrEqual(viewport.width);
            expect(y + h).toBeLessThanOrEqual(viewport.height);
          }
        });

        it('never overlaps two chrome regions — nothing floats over anything', () => {
          const chrome = chromeRegions(computed);
          for (let i = 0; i < chrome.length; i += 1) {
            for (let j = i + 1; j < chrome.length; j += 1) {
              expect(rectsOverlap(chrome[i]!.rect, chrome[j]!.rect)).toBe(false);
            }
          }
        });

        it('parks the prompt strip on the hand panel or its own strip, never elsewhere', () => {
          const prompt = computed.regions.promptStrip.rect;
          const hand = computed.regions.handPanel.rect;
          if (computed.composition === 'full') {
            // The strip rides the hand panel's top edge — contained in it.
            expect(prompt.x).toBeGreaterThanOrEqual(hand.x);
            expect(prompt.y).toBe(hand.y);
            expect(prompt.x + prompt.w).toBeLessThanOrEqual(hand.x + hand.w);
            expect(prompt.y + prompt.h).toBeLessThanOrEqual(hand.y + hand.h);
          } else {
            // Compact: its own strip above the action bar, overlapping no chrome.
            for (const other of chromeRegions(computed)) {
              expect(rectsOverlap(prompt, other.rect)).toBe(false);
            }
          }
        });

        it('keeps the bottom-shell chrome inside the canvas it overlays', () => {
          const canvas = computed.regions.canvas.rect;
          for (const id of ['mePanel', 'promptStrip', 'dock', 'handPanel'] as RegionId[]) {
            const r = computed.regions[id].rect;
            expect(r.x).toBeGreaterThanOrEqual(canvas.x);
            expect(r.y).toBeGreaterThanOrEqual(canvas.y);
            expect(r.x + r.w).toBeLessThanOrEqual(canvas.x + canvas.w);
            expect(r.y + r.h).toBeLessThanOrEqual(canvas.y + canvas.h);
          }
        });

        it('gives the board canvas the majority of the viewport', () => {
          const total = viewport.width * viewport.height;
          expect(rectArea(computed.regions.canvas.rect)).toBeGreaterThan(total * 0.5);
        });

        it('makes the canvas the single largest region', () => {
          const canvas = rectArea(computed.regions.canvas.rect);
          for (const region of Object.values(computed.regions)) {
            if (region.id === 'canvas') continue;
            expect(canvas).toBeGreaterThanOrEqual(rectArea(region.rect));
          }
        });

        it('emits one opponent panel frame per opponent seat', () => {
          expect(computed.scene.opponents).toHaveLength(Math.max(1, playerCount - 1));
          expect(computed.scene.width).toBe(computed.regions.canvas.rect.w);
          expect(computed.scene.height).toBe(computed.regions.canvas.rect.h);
        });
      });
    }
  }

  it('derives orientation from the aspect, never a device list', () => {
    expect(layout({ width: 390, height: 844 }, 2).orientation).toBe('portrait');
    expect(layout({ width: 1920, height: 1080 }, 2).orientation).toBe('landscape');
    expect(layout({ width: 800, height: 800 }, 2).orientation).toBe('landscape');
  });

  it('keeps the receiver bottom-anchored regardless of opponent count (issue #348)', () => {
    // The bottom shell stays pinned to the viewport bottom whether the table
    // seats one opponent or seven — opponents are added toward the top, never by
    // displacing the receiver's interaction area.
    for (const { viewport } of GEOMETRIES) {
      for (const playerCount of [2, 3, 4, 8]) {
        const computed = layout(viewport, playerCount);
        const { mePanel, handPanel, dock, topBar } = computed.regions;
        const bottoms = [mePanel, handPanel, dock].map((r) => r.rect.y + r.rect.h);
        // The shell's lowest region sits within the bottom pad of the viewport.
        expect(Math.max(...bottoms)).toBeGreaterThan(viewport.height - 12);
        expect(Math.max(...bottoms)).toBeLessThanOrEqual(viewport.height);
        // The top bar stays at the top, above every bottom-shell region.
        for (const r of [mePanel, handPanel, dock]) {
          expect(topBar.rect.y + topBar.rect.h).toBeLessThanOrEqual(r.rect.y);
        }
      }
    }
  });
});

describe('layout composition changes kind on geometry, not anatomy', () => {
  it('resolves the compact composition below the width threshold', () => {
    expect(layout({ width: 390, height: 844 }, 2).composition).toBe('compact');
    expect(layout({ width: 719, height: 900 }, 2).composition).toBe('compact');
    expect(layout({ width: 720, height: 900 }, 2).composition).toBe('full');
    expect(layout({ width: 1920, height: 1080 }, 2).composition).toBe('full');
  });

  it('docks a bounded right rail on the full composition', () => {
    const computed = layout({ width: 1280, height: 800 }, 2);
    const rail = computed.regions.rail.rect;
    expect(rail.w).toBeGreaterThanOrEqual(236);
    expect(rail.w).toBeLessThanOrEqual(312);
    // The rail parks at the right edge, beside (never over) the canvas.
    expect(rail.x + rail.w).toBe(1280 - 8);
    expect(rectsOverlap(rail, computed.regions.canvas.rect)).toBe(false);
  });

  it('collapses the rail to a zero-area region on compact — the identity persists', () => {
    const compact = layout({ width: 390, height: 844 }, 2);
    // The region is still carved (chrome never reorders) but claims no space:
    // stack/log live behind top-bar chips that open sheets.
    expect(compact.regions.rail.id).toBe('rail');
    expect(rectArea(compact.regions.rail.rect)).toBe(0);
    // The canvas spans the full padded width instead.
    expect(compact.regions.canvas.rect.w).toBe(390 - 12);
  });

  it('orders the compact bottom shell prompt → dock → hand → identity, top to bottom', () => {
    const { regions } = layout({ width: 390, height: 844 }, 2);
    expect(regions.promptStrip.rect.y).toBeLessThan(regions.dock.rect.y);
    expect(regions.dock.rect.y).toBeLessThan(regions.handPanel.rect.y);
    expect(regions.handPanel.rect.y).toBeLessThan(regions.mePanel.rect.y);
  });

  it('rides the prompt strip on the hand panel top edge on the full composition', () => {
    const { regions } = layout({ width: 1280, height: 800 }, 2);
    expect(regions.promptStrip.rect.y).toBe(regions.handPanel.rect.y);
    expect(regions.promptStrip.rect.x).toBe(regions.handPanel.rect.x);
    expect(regions.promptStrip.rect.w).toBe(regions.handPanel.rect.w);
    // Identity panel · hand panel · dock sit side by side on one bottom row.
    expect(regions.mePanel.rect.y).toBe(regions.handPanel.rect.y);
    expect(regions.dock.rect.y).toBe(regions.handPanel.rect.y);
    expect(regions.mePanel.rect.x + regions.mePanel.rect.w).toBeLessThanOrEqual(
      regions.handPanel.rect.x,
    );
    expect(regions.handPanel.rect.x + regions.handPanel.rect.w).toBeLessThanOrEqual(
      regions.dock.rect.x,
    );
  });
});

describe('opponent panel reflow (composition, never reordering)', () => {
  it('splits one row evenly across up to three opponents (full)', () => {
    const { scene } = layout({ width: 1280, height: 800 }, 4);
    expect(scene.opponents).toHaveLength(3);
    const ys = new Set(scene.opponents.map((f) => f.rect.y));
    expect(ys.size).toBe(1); // one row
    const widths = new Set(scene.opponents.map((f) => f.rect.w));
    expect(widths.size).toBe(1); // even split
  });

  it('wraps beyond three opponents into two rows (full)', () => {
    const { scene } = layout({ width: 1920, height: 1080 }, 8);
    expect(scene.opponents).toHaveLength(7);
    const ys = [...new Set(scene.opponents.map((f) => f.rect.y))].sort((a, b) => a - b);
    expect(ys).toHaveLength(2);
  });

  it('stacks opponent panels vertically on compact, receiver largest and last', () => {
    const { scene } = layout({ width: 390, height: 844 }, 3);
    expect(scene.opponents).toHaveLength(2);
    const [a, b] = scene.opponents;
    expect(a!.rect.y).toBeLessThan(b!.rect.y);
    // The receiver's panel sits below every opponent's and is at least as tall.
    expect(scene.you.rect.y).toBeGreaterThanOrEqual(b!.rect.y + b!.rect.h);
    expect(scene.you.rect.h).toBeGreaterThanOrEqual(b!.rect.h);
  });

  it('keeps the receiver a tier step ahead: duel field boards, crowded support boards', () => {
    expect(layout({ width: 1280, height: 800 }, 2).scene.tiers).toEqual({
      you: 'field',
      opp: 'support',
    });
    expect(layout({ width: 1280, height: 800 }, 4).scene.tiers).toEqual({
      you: 'support',
      opp: 'mini',
    });
    expect(layout({ width: 390, height: 844 }, 2).scene.tiers).toEqual({
      you: 'support',
      opp: 'mini',
    });
  });

  it('fans the hand only on the compact composition', () => {
    expect(layout({ width: 1280, height: 800 }, 2).scene.handFan).toBe(false);
    expect(layout({ width: 390, height: 844 }, 2).scene.handFan).toBe(true);
  });
});

describe('phone-portrait summary-tile composition + focus (issue #400)', () => {
  const PHONE: Viewport = { width: 390, height: 844, pointer: 'coarse' };

  it('collapses opponents to summary tiles only at phone-portrait with 3–4 seats', () => {
    // A duel keeps both battlefields in full (no tiles).
    expect(layout(PHONE, 2).summaryTiles).toBe(false);
    expect(layout(PHONE, 2).scene.opponents.every((f) => !f.summary)).toBe(true);
    // 3 and 4 seats change kind to summary tiles.
    for (const seats of [3, 4]) {
      const computed = layout(PHONE, seats);
      expect(computed.composition).toBe('compact');
      expect(computed.summaryTiles).toBe(true);
      expect(computed.scene.opponents).toHaveLength(seats - 1);
      // Every opponent frame is a collapsed tile when none is focused.
      expect(computed.scene.opponents.every((f) => f.summary === true)).toBe(true);
      // Tiles carry no card area; the receiver keeps a full battlefield below them.
      for (const tile of computed.scene.opponents) {
        expect(tile.content.w).toBe(0);
        expect(tile.content.h).toBe(0);
        expect(tile.rect.h).toBeGreaterThanOrEqual(44); // one touch-sized tap target
        expect(tile.rect.y + tile.rect.h).toBeLessThanOrEqual(computed.scene.you.rect.y);
      }
      expect(computed.scene.you.summary).toBeFalsy();
      expect(rectArea(computed.scene.you.content)).toBeGreaterThan(0);
    }
  });

  it('never resolves the tile composition on the full composition, whatever the seats', () => {
    for (const seats of [3, 4]) {
      const computed = layout({ width: 1280, height: 800 }, seats);
      expect(computed.summaryTiles).toBe(false);
      expect(computed.scene.opponents.every((f) => !f.summary)).toBe(true);
    }
  });

  for (const seats of [3, 4]) {
    describe(`${seats} seats @ phone-portrait`, () => {
      it('expands exactly the focused opponent in place, tiling the rest', () => {
        const opponents = seats - 1;
        const focusIdx = opponents - 1; // the last opponent
        const computed = layout(PHONE, seats, { opponent: focusIdx });
        computed.scene.opponents.forEach((frame, i) => {
          if (i === focusIdx) {
            expect(frame.summary).toBeFalsy();
            expect(rectArea(frame.content)).toBeGreaterThan(0);
          } else {
            expect(frame.summary).toBe(true);
            expect(frame.content.w).toBe(0);
          }
        });
        // The expanded battlefield is taller than a collapsed tile.
        const collapsed = computed.scene.opponents.find((_, i) => i !== focusIdx)!;
        expect(computed.scene.opponents[focusIdx]!.rect.h).toBeGreaterThan(collapsed.rect.h);
      });

      it('keeps opponent frames + receiver non-overlapping and top-to-bottom in seat order', () => {
        const computed = layout(PHONE, seats, { opponent: 0 });
        const frames = [...computed.scene.opponents, computed.scene.you];
        for (let i = 1; i < frames.length; i += 1) {
          // Each frame starts at or below the previous frame's bottom (stacked).
          expect(frames[i]!.rect.y).toBeGreaterThanOrEqual(
            frames[i - 1]!.rect.y + frames[i - 1]!.rect.h,
          );
        }
        // Every frame stays inside the carved canvas.
        for (const f of frames) {
          expect(f.rect.x).toBeGreaterThanOrEqual(0);
          expect(f.rect.y).toBeGreaterThanOrEqual(0);
          expect(f.rect.y + f.rect.h).toBeLessThanOrEqual(computed.scene.height + 1);
        }
      });
    });
  }

  it('ignores an out-of-range focus index (no opponent expands)', () => {
    const computed = layout(PHONE, 4, { opponent: 9 });
    expect(computed.scene.opponents.every((f) => f.summary === true)).toBe(true);
  });

  it('ignores focus entirely on the full composition (focus is presentation, not geometry)', () => {
    const withFocus = layout({ width: 1280, height: 800 }, 4, { opponent: 1 });
    const without = layout({ width: 1280, height: 800 }, 4);
    expect(withFocus).toEqual(without);
  });

  it('is deterministic for a given focus (ephemeral, but pure)', () => {
    expect(layout(PHONE, 4, { opponent: 1 })).toEqual(layout(PHONE, 4, { opponent: 1 }));
  });
});

describe('layout is a pure, deterministic function', () => {
  it('returns identical output for identical input', () => {
    const a = layout({ width: 1440, height: 900, pointer: 'fine' }, 3);
    const b = layout({ width: 1440, height: 900, pointer: 'fine' }, 3);
    expect(a).toEqual(b);
  });

  it('defaults an absent pointer to fine (SSR/tests) and echoes a supplied one', () => {
    expect(layout({ width: 1280, height: 800 }, 2).viewport.pointer).toBe('fine');
    expect(layout({ width: 1280, height: 800, pointer: 'coarse' }, 2).viewport.pointer).toBe(
      'coarse',
    );
    expect(DEFAULT_VIEWPORT.pointer).toBe('fine');
  });

  it('guards degenerate (zero) geometry into finite rects', () => {
    const computed = layout({ width: 0, height: 0 }, 2);
    for (const region of Object.values(computed.regions)) {
      for (const value of [region.rect.x, region.rect.y, region.rect.w, region.rect.h]) {
        expect(Number.isFinite(value)).toBe(true);
      }
    }
    expect(computed.playerCount).toBe(2);
  });
});

describe('the carved scene geometry feeds the scene with no horizontal scroll', () => {
  for (const { name, viewport } of GEOMETRIES) {
    for (const playerCount of [2, 4]) {
      it(`a dense board fits the canvas width @ ${name} ${playerCount}p`, () => {
        const computed = layout(viewport, playerCount);
        // A dense board (30 permanents per player) wrapped inside the carved
        // panels must never place a card past the canvas width → no sideways scroll.
        const controllers = Array.from({ length: playerCount }, (_, i) => `p${i + 1}`);
        const scene = buildTableScene(boardView(controllers, 30), undefined, computed.scene);
        const rects = scene.bands.flatMap((b) => b.cards).map((c) => c.rect);
        // Every rendered band lays its 30 permanents; a collapsed summary tile
        // (phone-portrait multiplayer, issue #400) renders none — so the rendered
        // count follows the non-summary bands, and the no-scroll invariant holds for
        // exactly the cards that are drawn.
        const renderedBands = scene.bands.filter((b) => !b.summary).length;
        expect(rects.length).toBe(renderedBands * 30);
        for (const rect of rects) {
          expect(rect.x).toBeGreaterThanOrEqual(0);
          expect(rect.x + rect.w).toBeLessThanOrEqual(scene.width);
        }
        expect(scene.width).toBe(computed.regions.canvas.rect.w);
      });
    }
  }
});
