/**
 * The seat identity cluster as the reconciler mounts it (issue #532).
 *
 * `seat-cluster.test.ts` proves the staged geometry; this file proves the DOM
 * that geometry becomes: one element per cluster part, the state channels as
 * data attributes a stylesheet can select on, the portrait URL and the seat
 * accent as custom properties (neither can be an `attr()`), fresh-mount
 * equivalence after a reconcile, and the scene-node cost at six seats.
 *
 * jsdom applies no CSS module and performs no layout, so nothing here asserts a
 * painted ring, a clip-path, or a contrast reading — only that the inputs the
 * stylesheet needs are present and correct.
 */
import { describe, expect, it } from 'vitest';
import { stagePlane, type StagedPlane } from './plane';
import { PlaneReconciler, type PlaneFaceRenderer } from './planeReconciler';
import { SCENE_DOM_CEILING } from './live/presentationMode';
import { clusterTable } from './seat-cluster.fixture';
import { DESKTOP } from './plane.fixture';

const face: PlaneFaceRenderer = {
  signature: (render) => `${render.entityId}:${render.tier}`,
  render: (el, render) => {
    el.textContent = render.name;
  },
};

function seats(n: number, overrides: Record<number, Record<string, unknown>> = {}) {
  return Array.from({ length: n }, (_, i) => ({
    id: `p${i + 1}`,
    name: `Player ${i + 1}`,
    ...(overrides[i] ?? {}),
  }));
}

function mount(plane: StagedPlane): PlaneReconciler {
  const reconciler = new PlaneReconciler(document.createElement('div'), { face });
  reconciler.reconcile(plane);
  return reconciler;
}

function slot(r: PlaneReconciler, kind: string, seat: string): HTMLElement | null {
  return r.root.querySelector(`[data-slot="${kind}"][data-seat="${seat}"]`);
}

describe('seat cluster DOM — one element per part of the cluster', () => {
  it('mounts the medallion, nameplate, life medallion, and hand pip for every seat', () => {
    const plane = stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), {
      width: 1680,
      height: 945,
    });
    const r = mount(plane);
    for (const kind of ['crest', 'plate', 'life', 'pip']) {
      expect(r.root.querySelectorAll(`[data-slot="${kind}"]`)).toHaveLength(4);
    }
    // No library pip, at any rung: the library count is the library pile's
    // (`zone-geography.md` §4.1).
    expect(r.root.querySelectorAll('[data-slot="librarypip"]')).toHaveLength(0);
  });

  it('carries the life numeral, the hand count, and the glyph-count size step', () => {
    const plane = stagePlane(
      clusterTable({ seats: seats(2, { 0: { life: 999, hand: 20 } }) }),
      DESKTOP,
    );
    const r = mount(plane);
    expect(slot(r, 'life', 'p1')?.dataset.glyph).toBe('999');
    expect(slot(r, 'life', 'p1')?.dataset.glyphs).toBe('3');
    expect(slot(r, 'pip', 'p1')?.dataset.count).toBe('20');
    // The crest keeps the never-degrading readout it has always carried.
    expect(slot(r, 'crest', 'p1')?.dataset.life).toBe('999');
    expect(slot(r, 'crest', 'p1')?.dataset.hand).toBe('20');
  });

  it('publishes every state channel as its own attribute', () => {
    const plane = stagePlane(
      clusterTable({
        seats: seats(4, { 1: { command: ['{G}'] } }),
        active: 'p2',
        priority: 'p2',
        attacks: [['p3', 'p2']],
      }),
      { width: 1680, height: 945 },
    );
    const r = mount(plane);
    const crest = slot(r, 'crest', 'p2')!;
    expect(crest.dataset).toMatchObject({
      priority: 'true',
      active: 'true',
      attacked: 'true',
      focused: 'true',
      commander: 'true',
      eliminated: 'false',
      // Dormant until a per-seat connection field lands (§11, issue #553).
      disconnected: 'false',
    });
    const quiet = slot(r, 'crest', 'p4')!;
    expect(quiet.dataset).toMatchObject({ priority: 'false', active: 'false', attacked: 'false' });
  });

  it('swaps the life numeral for the struck rune on an eliminated seat', () => {
    const plane = stagePlane(
      clusterTable({ seats: seats(2, { 1: { eliminated: true } }) }),
      DESKTOP,
    );
    const r = mount(plane);
    expect(slot(r, 'life', 'p2')?.dataset.glyph).toBe('⊘');
    expect(slot(r, 'life', 'p2')?.dataset.eliminated).toBe('true');
    expect(slot(r, 'plate', 'p2')?.dataset.eliminated).toBe('true');
    // Count pips and the status rail are removed; the seat keeps its slot.
    expect(slot(r, 'pip', 'p2')).toBeNull();
    expect(r.root.querySelector('[data-slot="chip"][data-seat="p2"]')).toBeNull();
    expect(slot(r, 'crest', 'p2')).not.toBeNull();
  });

  it('carries the portrait URL and the seat accent as custom properties', () => {
    // `attr()` cannot produce a `url()`, and the accent is a token value rather
    // than an enum, so both ride as custom properties — which keeps the
    // stylesheet free of both hashes and hex.
    const plane = stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), DESKTOP);
    const r = mount(plane);
    const crest = slot(r, 'crest', 'p1')!;
    expect(crest.style.getPropertyValue('--portrait-src')).toMatch(
      /^url\("\/assets\/portraits\/.+\.webp"\)$/,
    );
    expect(crest.style.getPropertyValue('--seat-accent')).toMatch(/^#[0-9A-Fa-f]{6}$/);
    expect(crest.style.getPropertyValue('--cluster-d')).toMatch(/^\d+px$/);
    expect(crest.dataset.portrait).toBe('true');
    // The monogram rides beside it and is never empty — it is what the aperture
    // draws with no plate available, and that path can never regress away.
    expect(crest.dataset.monogram).toBeTruthy();
  });

  it('lays the status rail out as one element per drawn chip', () => {
    const plane = stagePlane(
      clusterTable({
        seats: seats(4, { 1: { statuses: ['monarch', 'initiative', 'ring-tempts'] } }),
        active: 'p2',
        attacks: [['p3', 'p2']],
      }),
      { width: 1680, height: 945 },
    );
    const r = mount(plane);
    const chips = [...r.root.querySelectorAll('[data-slot="chip"][data-seat="p2"]')];
    expect(chips.map((c) => (c as HTMLElement).dataset.kind)).toEqual([
      'attacked',
      'status',
      'overflow',
    ]);
    expect((chips[0] as HTMLElement).dataset.value).toBe('×1');
    expect((chips[2] as HTMLElement).dataset.value).toBe('2');
  });

  it('draws the identity gem only when the command zone names the seat', () => {
    const withCommander = stagePlane(
      clusterTable({ seats: seats(2, { 0: { command: ['{2}{B}'] } }) }),
      DESKTOP,
    );
    expect(slot(mount(withCommander), 'gem', 'p1')?.dataset.identity).toBe('B');
    const without = stagePlane(clusterTable({ seats: seats(2) }), DESKTOP);
    expect(slot(mount(without), 'gem', 'p1')).toBeNull();
  });

  it('gives a compact tile the seat’s portrait and accent, at no extra node', () => {
    const plane = stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), {
      width: 390,
      height: 844,
    });
    expect(plane.tiles.length).toBeGreaterThan(0);
    const r = mount(plane);
    const tile = r.root.querySelector(
      `[data-slot="tile"][data-seat="${plane.tiles[0]!.seat}"]`,
    ) as HTMLElement;
    expect(tile.style.getPropertyValue('--portrait-src')).toMatch(/^url\(".+"\)$/);
    expect(tile.style.getPropertyValue('--seat-accent')).toMatch(/^#[0-9A-Fa-f]{6}$/);
  });
});

describe('seat cluster DOM — reconcile hygiene and budget', () => {
  it('updates a changed life in place and touches nothing else', () => {
    const before = stagePlane(clusterTable({ seats: seats(2, { 0: { life: 20 } }) }), DESKTOP);
    const after = stagePlane(clusterTable({ seats: seats(2, { 0: { life: 17 } }) }), DESKTOP);
    const r = mount(before);
    const el = slot(r, 'life', 'p1')!;
    r.reconcile(after);
    expect(slot(r, 'life', 'p1')).toBe(el);
    expect(el.dataset.glyph).toBe('17');
  });

  it('does zero work when the same plane is applied twice', () => {
    const plane = stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), DESKTOP);
    const r = mount(plane);
    r.reconcile(stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), DESKTOP));
    expect(r.lastStats.chrome).toBe(0);
  });

  it('is byte-identical to a fresh mount after a reconcile (fresh-mount equivalence)', () => {
    const first = stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), DESKTOP);
    const second = stagePlane(
      clusterTable({
        seats: seats(4, { 1: { life: 12, statuses: ['monarch'] } }),
        active: 'p3',
        priority: 'p3',
        attacks: [['p2', 'p1']],
      }),
      DESKTOP,
    );
    const incremental = mount(first);
    incremental.reconcile(second);
    expect(incremental.root.innerHTML).toBe(mount(second).root.innerHTML);
  });

  it('stays far inside the scene-node ceiling at six seats', () => {
    const plane = stagePlane(
      clusterTable({
        seats: seats(6, {
          1: { statuses: ['monarch', 'initiative'], command: ['{W}{U}'] },
          2: { statuses: ['x'] },
        }),
        active: 'p2',
        priority: 'p2',
        attacks: [
          ['p2', 'p1'],
          ['p3', 'p4'],
        ],
        commanderDamage: [['p2', 'p1', 18]],
        commanderTax: [['p2', 4]],
      }),
      { width: 1920, height: 1080 },
    );
    const r = mount(plane);
    const clusterNodes = r.root.querySelectorAll(
      '[data-slot="crest"], [data-slot="plate"], [data-slot="life"], [data-slot="pip"], [data-slot="gem"], [data-slot="chip"]',
    ).length;
    // Six seats, every state lit: the whole identity layer is a couple of dozen
    // elements against a 15 000-node scene ceiling.
    expect(clusterNodes).toBeLessThanOrEqual(6 * 10);
    expect(r.root.querySelectorAll('*').length).toBeLessThan(SCENE_DOM_CEILING / 10);
  });
});
