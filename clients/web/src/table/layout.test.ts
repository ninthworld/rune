import { describe, expect, it } from 'vitest';
import { buildTableScene } from './scene';
import { normalizeGameView } from '../wire';
import type { GameView } from '../protocol';
import {
  battlefieldWidth,
  layout,
  rectArea,
  rectsOverlap,
  type Mode,
  type RegionId,
  type TableLayout,
  type Viewport,
} from './layout';

/**
 * The supported-geometry matrix the shell must resolve identically from: a
 * portrait phone, a small landscape window, a 16:9 desktop, and an ultrawide.
 * Portrait, landscape, and ultrawide all come out of the one pure function.
 */
const GEOMETRIES: { name: string; viewport: Viewport }[] = [
  { name: 'portrait phone', viewport: { width: 390, height: 844, pointer: 'coarse' } },
  { name: 'small landscape', viewport: { width: 668, height: 375, pointer: 'coarse' } },
  { name: '16:9 desktop', viewport: { width: 1920, height: 1080, pointer: 'fine' } },
  { name: 'ultrawide', viewport: { width: 3440, height: 1440, pointer: 'fine' } },
  { name: 'tall portrait tablet', viewport: { width: 768, height: 1024, pointer: 'coarse' } },
];

const PLAYER_COUNTS = [2, 4, 8];
const DOCKED: RegionId[] = ['indicator', 'opponentHud', 'battlefield', 'rail'];

/** The docked regions actually docked at this geometry (the rail may float). */
function dockedRegions(computed: TableLayout) {
  return DOCKED.map((id) => computed.regions[id]).filter((r) => r.layer === 'docked');
}

/** A board with `perController` permanents each, for the horizontal-scroll check. */
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

describe('layout region geometry', () => {
  for (const { name, viewport } of GEOMETRIES) {
    for (const playerCount of PLAYER_COUNTS) {
      describe(`${name} @ ${playerCount}p`, () => {
        const computed = layout(viewport, 'overview', playerCount);

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

        it('never overlaps two docked regions', () => {
          const docked = dockedRegions(computed);
          for (let i = 0; i < docked.length; i += 1) {
            for (let j = i + 1; j < docked.length; j += 1) {
              expect(rectsOverlap(docked[i]!.rect, docked[j]!.rect)).toBe(false);
            }
          }
        });

        it('keeps floating chrome clear of the docked top/side chrome', () => {
          // Floating regions may overlay the battlefield (they sit above it), but
          // must not collide with the docked top chrome or a docked rail.
          const floating = Object.values(computed.regions).filter((r) => r.layer === 'floating');
          const barriers = [computed.regions.indicator, computed.regions.opponentHud].concat(
            computed.regions.rail.layer === 'docked' ? [computed.regions.rail] : [],
          );
          for (const region of floating) {
            for (const barrier of barriers) {
              expect(rectsOverlap(region.rect, barrier.rect)).toBe(false);
            }
          }
        });

        it('gives the battlefield the majority of the viewport', () => {
          // The battlefield alone (it visually contains the hand the scene draws)
          // claims well over half the viewport at every geometry.
          const total = viewport.width * viewport.height;
          expect(rectArea(computed.regions.battlefield.rect)).toBeGreaterThan(total * 0.5);
        });

        it('makes the battlefield the single largest region', () => {
          const bf = rectArea(computed.regions.battlefield.rect);
          for (const region of Object.values(computed.regions)) {
            if (region.id === 'battlefield') continue;
            expect(bf).toBeGreaterThanOrEqual(rectArea(region.rect));
          }
        });
      });
    }
  }

  it('derives orientation from the aspect, never a device list', () => {
    expect(layout({ width: 390, height: 844 }, 'overview', 2).orientation).toBe('portrait');
    expect(layout({ width: 1920, height: 1080 }, 'overview', 2).orientation).toBe('landscape');
    expect(layout({ width: 800, height: 800 }, 'overview', 2).orientation).toBe('landscape');
  });

  it('collapses the rail to a floating badge on narrow width', () => {
    const narrow = layout({ width: 400, height: 800 }, 'overview', 2);
    expect(narrow.railCollapsed).toBe(true);
    expect(narrow.regions.rail.layer).toBe('floating');
    // A collapsed rail returns the full width to the battlefield.
    expect(narrow.regions.battlefield.rect.w).toBe(400);

    const wide = layout({ width: 1600, height: 900 }, 'overview', 2);
    expect(wide.railCollapsed).toBe(false);
    expect(wide.regions.rail.layer).toBe('docked');
    expect(wide.regions.battlefield.rect.w).toBeLessThan(1600);
  });

  it('reflows the opponent HUD taller as the seat count grows (capped)', () => {
    const two = layout({ width: 768, height: 1024 }, 'overview', 2);
    const eight = layout({ width: 768, height: 1024 }, 'overview', 8);
    expect(eight.regions.opponentHud.rect.h).toBeGreaterThan(two.regions.opponentHud.rect.h);
    // Even crowded, top chrome never exceeds 30% of the height (board stays majority).
    const topH = eight.regions.indicator.rect.h + eight.regions.opponentHud.rect.h;
    expect(topH).toBeLessThanOrEqual(Math.floor(1024 * 0.3));
  });

  it('floats the action tray above the hand band', () => {
    const computed = layout({ width: 1280, height: 800 }, 'overview', 2);
    const { tray, hand } = computed.regions;
    expect(tray.rect.y + tray.rect.h).toBeLessThanOrEqual(hand.rect.y);
    // The tray clears the local dock on its left.
    expect(tray.rect.x).toBeGreaterThanOrEqual(
      computed.regions.localDock.rect.x + computed.regions.localDock.rect.w,
    );
  });

  it('keeps the receiver bottom-anchored regardless of opponent count (issue #348)', () => {
    // The receiver's hand and dock stay pinned to the viewport bottom whether the
    // table seats one opponent or three — opponents are added toward the top, never
    // by displacing the receiver's bottom interaction area.
    for (const { viewport } of GEOMETRIES) {
      const { height } = { height: Math.max(1, Math.floor(viewport.height)) };
      for (const playerCount of [2, 3, 4]) {
        const computed = layout(viewport, 'overview', playerCount);
        const { hand, localDock } = computed.regions;
        // The hand band's bottom edge sits at the very bottom of the viewport.
        expect(hand.rect.y + hand.rect.h).toBe(height);
        // The local dock is anchored to the bottom too (within its bottom pad).
        expect(localDock.rect.y + localDock.rect.h).toBeLessThanOrEqual(height);
        expect(localDock.rect.y + localDock.rect.h).toBeGreaterThan(height - 24);
        // The opponent HUD strip stays at the top, above the receiver's band.
        expect(computed.regions.opponentHud.rect.y).toBeLessThan(hand.rect.y);
      }
    }
  });
});

describe('layout is mode-invariant (regions never move between overview and focus)', () => {
  for (const { name, viewport } of GEOMETRIES) {
    it(`places identical regions in both modes @ ${name}`, () => {
      const overview = layout(viewport, 'overview', 4);
      const focus = layout(viewport, 'focus', 4);
      // Only the echoed mode differs; every region rect is identical.
      expect(focus.regions).toEqual(overview.regions);
      expect(focus.mode).toBe('focus');
      expect(overview.mode).toBe('overview');
    });
  }
});

describe('layout is a pure, deterministic function', () => {
  it('returns identical output for identical input', () => {
    const a = layout({ width: 1440, height: 900, pointer: 'fine' }, 'overview', 3);
    const b = layout({ width: 1440, height: 900, pointer: 'fine' }, 'overview', 3);
    expect(a).toEqual(b);
  });

  it('guards degenerate (zero) geometry into a well-formed layout', () => {
    const computed = layout({ width: 0, height: 0 }, 'overview', 2);
    for (const region of Object.values(computed.regions)) {
      expect(region.rect.w).toBeGreaterThanOrEqual(0);
      expect(region.rect.h).toBeGreaterThanOrEqual(0);
    }
  });
});

describe('battlefield sizing feeds the scene with no horizontal scroll', () => {
  for (const { name, viewport } of GEOMETRIES) {
    for (const playerCount of PLAYER_COUNTS) {
      it(`the scene fits the battlefield width @ ${name} ${playerCount}p`, () => {
        const computed = layout(viewport, 'overview', playerCount);
        const width = battlefieldWidth(computed);
        // A dense board (50 permanents per player) wrapped within the battlefield
        // width must never report a scene wider than that width → no sideways scroll.
        const controllers = Array.from({ length: playerCount }, (_, i) => `p${i + 1}`);
        const scene = buildTableScene(boardView(controllers, 50), undefined, width);
        const maxRight = Math.max(
          ...scene.bands.flatMap((b) => b.cards).map((c) => c.rect.x + c.rect.w),
        );
        expect(scene.width).toBeLessThanOrEqual(width);
        expect(maxRight).toBeLessThanOrEqual(width);
      });
    }
  }
});

// Type-only sanity: the exported Mode union is what the Table passes through.
const _modes: Mode[] = ['overview', 'focus'];
void _modes;
