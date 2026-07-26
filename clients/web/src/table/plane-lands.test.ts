/**
 * Land staging (issue #463 (c), card-representation §15.9 / decision 19).
 *
 * The land row's baseline rung is the digest `chip`, which draws no name at all
 * (`TIER.chip.name === 0`, §8.4). That is right for a basic — its emblem is its
 * identity — and wrong for a nonbasic, which arrives with no emblem and would
 * otherwise stage as an anonymous 48 px art field. `CardFace` has always drawn
 * the §15.9 name strip; until this change nothing on the plane ever gave a
 * nonbasic land a tier that could show it.
 *
 * jsdom performs no layout, so these are assertions about the staged model —
 * the tier and box the plane reserves — not about painted pixels.
 */
import { describe, expect, it } from 'vitest';
import { faceMetrics } from '../card/dom/theme';
import { landRenderTier } from './scene/card-helpers';
import { regionsOf, seatTable, stage } from './plane.fixture';
import type { PlaneRender } from './plane';

/** One nonbasic land for a controller (no basic emblem is derivable from it). */
function nonbasic(controller: string, id: string, name: string) {
  return { id, controller, name, type_line: 'Land' };
}

/** One basic land for a controller. */
function basic(controller: string, id: string) {
  return { id, controller, name: 'Forest', type_line: 'Basic Land — Forest' };
}

/** Every staged render of the whole plane, flattened. */
function allRenders(view: Parameters<typeof stage>[0]): PlaneRender[] {
  return regionsOf(stage(view)).flatMap((region) => region.renders);
}

describe('landRenderTier (issue #463)', () => {
  it('keeps a basic land on the chip rung — its emblem carries identity', () => {
    expect(landRenderTier('Basic Land — Forest', 'chip')).toBe('chip');
  });

  it('lifts a nonbasic land off the chip rung so its name can be drawn', () => {
    expect(landRenderTier('Land', 'chip')).toBe('mini');
    expect(landRenderTier('Land — Desert', 'chip')).toBe('mini');
  });

  it('never overrides a row tier that is already above chip', () => {
    for (const tier of ['mini', 'support', 'field'] as const) {
      expect(landRenderTier('Land', tier), tier).toBe(tier);
    }
  });

  it('is why the promotion is needed: chip draws no name, mini does', () => {
    expect(faceMetrics('chip', 'land').name).toBe(0);
    expect(faceMetrics('mini', 'land').name).toBeGreaterThanOrEqual(11);
  });
});

describe('stagePlane — nonbasic lands are never anonymous chips', () => {
  it('stages a nonbasic land above the chip rung and a basic one on it', () => {
    const view = seatTable({
      opponents: 1,
      perms: [basic('p1', 'p1_forest'), nonbasic('p1', 'p1_causeway', 'Moonlit Causeway')],
    });
    const renders = allRenders(view);
    const forest = renders.find((r) => r.entityId === 'p1_forest');
    const causeway = renders.find((r) => r.entityId === 'p1_causeway');
    expect(forest?.tier).toBe('chip');
    expect(causeway?.tier).toBe('mini');
    // Both still sort into the land row — the promotion changes the tier only.
    expect(causeway?.row).toBe('lands');
  });

  it('promotes a nonbasic land on every seat, receiver and opponent alike', () => {
    const view = seatTable({
      opponents: 3,
      active: 'p2',
      perms: [
        nonbasic('p1', 'p1_causeway', 'Moonlit Causeway'),
        nonbasic('p2', 'p2_causeway', 'Moonlit Causeway'),
        nonbasic('p3', 'p3_causeway', 'Moonlit Causeway'),
      ],
    });
    const promoted = allRenders(view).filter((r) => r.row === 'lands');
    expect(promoted).toHaveLength(3);
    expect(promoted.every((r) => r.tier !== 'chip')).toBe(true);
  });

  it('reserves the promoted tile a bigger box than a chip — the readable one', () => {
    const view = seatTable({
      opponents: 1,
      perms: [basic('p1', 'p1_forest'), nonbasic('p1', 'p1_causeway', 'Moonlit Causeway')],
    });
    const renders = allRenders(view);
    const forest = renders.find((r) => r.entityId === 'p1_forest')!;
    const causeway = renders.find((r) => r.entityId === 'p1_causeway')!;
    expect(causeway.rect.w).toBeGreaterThan(forest.rect.w);
    // Every battlefield object keeps its ≥ 44 px hit target either way.
    for (const render of [forest, causeway]) {
      expect(render.hitRect.w).toBeGreaterThanOrEqual(44);
      expect(render.hitRect.h).toBeGreaterThanOrEqual(44);
    }
  });

  it('keeps identical nonbasic lands foldable into one ×N pile', () => {
    const view = seatTable({
      opponents: 1,
      perms: Array.from({ length: 24 }, (_, i) =>
        nonbasic('p2', `p2_causeway_${i}`, 'Moonlit Causeway'),
      ),
    });
    const lands = allRenders(view).filter((r) => r.row === 'lands');
    // Whatever rung the ladder settles on, the pile is still one render with
    // every member addressable, and it never falls back to the chip rung.
    expect(lands.every((r) => r.tier !== 'chip')).toBe(true);
    expect(lands.reduce((sum, r) => sum + r.memberIds.length, 0)).toBe(24);
  });
});
