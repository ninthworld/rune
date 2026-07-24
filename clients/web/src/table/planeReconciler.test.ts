/**
 * DOM plane reconciler — reconcile-by-id mechanics and the carried invariants
 * (issue #481): fresh-mount equivalence after any single reconcile, zero work
 * when nothing changes, full rebuilds reserved for reconnect, and determinism
 * over real GameView sequences. Test style carried from
 * `sceneReconciler.test.ts`: structural (serialized-DOM) snapshots and no wall
 * clock — the un-animated paths here need no clock at all.
 */
import { describe, expect, it } from 'vitest';
import type { GameView } from '../protocol';
import { toDisplayData } from './scene/card-helpers';
import { stagePlane, type PlaneStagingState, type StagedPlane } from './plane';
import { seatTable, bears, menagerie, DESKTOP } from './plane.fixture';
import { PlaneReconciler, planeRenders, type PlaneFaceRenderer } from './planeReconciler';
import { cardFaceRenderer } from './planeFaceRenderer';

/** A cheap deterministic face renderer that counts its render calls. */
function testFace(): PlaneFaceRenderer & { renderCalls: number } {
  const face = {
    renderCalls: 0,
    signature: (render: Parameters<PlaneFaceRenderer['signature']>[0]) =>
      [render.name, render.tier, render.tapped, render.stackCount, render.candidate].join('|'),
    render(el: HTMLElement, render: Parameters<PlaneFaceRenderer['render']>[1]) {
      face.renderCalls += 1;
      el.dataset.face = face.signature(render);
      el.textContent = render.name;
    },
  };
  return face;
}

/** A reconciler on a fresh detached root. */
function make(face: PlaneFaceRenderer = testFace()): PlaneReconciler {
  return new PlaneReconciler(document.createElement('div'), { face });
}

/** Mount a plane into a brand-new reconciler and return its serialized DOM. */
function freshMount(plane: StagedPlane, face: () => PlaneFaceRenderer = testFace): string {
  const reconciler = new PlaneReconciler(document.createElement('div'), { face: face() });
  reconciler.reconcile(plane);
  return reconciler.root.innerHTML;
}

/** Stage the fixture view on the desktop plane. */
function planeOf(view: GameView, staging?: PlaneStagingState): StagedPlane {
  return stagePlane(view, DESKTOP, staging);
}

describe('PlaneReconciler add/update/move/remove (by entity id)', () => {
  it('mounts one wrapper per staged render plus the slot chrome', () => {
    const plane = planeOf(seatTable({ opponents: 3, active: 'p2', perms: menagerie('p2', 3) }));
    const r = make();
    r.reconcile(plane);
    const wrappers = r.root.querySelectorAll('[data-entity-id]');
    expect(wrappers).toHaveLength(planeRenders(plane).length);
    // Slot chrome: one region + crest + piles per staged seat.
    expect(r.root.querySelectorAll('[data-slot="region"]')).toHaveLength(4);
    expect(r.root.querySelectorAll('[data-slot="crest"]')).toHaveLength(4);
    expect(r.root.querySelectorAll('[data-slot="piles"]')).toHaveLength(4);
    expect(r.lastStats.created).toBe(planeRenders(plane).length);
  });

  it('moves a wrapper in place on a position-only change — no face re-render', () => {
    const face = testFace();
    const r = make(face);
    r.reconcile(planeOf(seatTable({ opponents: 1, perms: menagerie('p2', 2) })));
    const el = r.elementFor('p2_beast_0')!;
    const renders = face.renderCalls;

    // A third creature reflows the row; existing faces are reused untouched.
    r.reconcile(planeOf(seatTable({ opponents: 1, perms: menagerie('p2', 3) })));
    expect(r.elementFor('p2_beast_0')).toBe(el);
    expect(face.renderCalls).toBe(renders + 1); // only the new card drew
    expect(r.lastStats.moved).toBeGreaterThan(0);
    expect(r.lastStats.updatedFaces).toBe(0);
  });

  it('re-renders a face in place when its signature changes', () => {
    const face = testFace();
    const r = make(face);
    r.reconcile(planeOf(seatTable({ opponents: 1, perms: bears('p2', 2) })));
    const el = r.elementFor('p2_bear_0')!;

    const tapped = seatTable({
      opponents: 1,
      perms: [
        { id: 'p2_bear_0', controller: 'p2', name: 'Bear', tapped: true },
        { id: 'p2_bear_1', controller: 'p2', name: 'Bear' },
      ],
    });
    r.reconcile(planeOf(tapped));

    expect(r.elementFor('p2_bear_0')).toBe(el); // same wrapper, new face
    expect(r.lastStats.updatedFaces).toBeGreaterThan(0);
    expect(el.dataset.face).toContain('true'); // tapped rode the signature
  });

  it('removes entities that leave the plane and keeps the rest', () => {
    const r = make();
    r.reconcile(planeOf(seatTable({ opponents: 1, perms: menagerie('p2', 3) })));
    const kept = r.elementFor('p2_beast_0');
    r.reconcile(planeOf(seatTable({ opponents: 1, perms: menagerie('p2', 1) })));
    expect(r.elementFor('p2_beast_0')).toBe(kept);
    expect(r.elementFor('p2_beast_2')).toBeUndefined();
    expect(r.targetFor('p2_beast_2')).toBeUndefined();
    expect(r.lastStats.removed).toBe(2);
  });

  it('does ZERO work when nothing changed (ADR 0030 binding rule)', () => {
    const face = testFace();
    const r = make(face);
    const view = seatTable({ opponents: 3, active: 'p2', perms: menagerie('p2', 4) });
    r.reconcile(planeOf(view));
    const html = r.root.innerHTML;
    const renders = face.renderCalls;

    r.reconcile(planeOf(view));
    expect(r.lastStats).toEqual({
      created: 0,
      updatedFaces: 0,
      moved: 0,
      removed: 0,
      chrome: 0,
      reordered: false,
    });
    expect(face.renderCalls).toBe(renders);
    expect(r.root.innerHTML).toBe(html);
  });
});

describe('PlaneReconciler fresh-mount equivalence (the cache is never load-bearing)', () => {
  /** Frames exercising add, reflow, tap, fold, focus change, and elimination. */
  const frames: { view: GameView; staging?: PlaneStagingState }[] = [
    { view: seatTable({ opponents: 3, active: 'p2', perms: menagerie('p2', 2) }) },
    {
      view: seatTable({
        opponents: 3,
        active: 'p2',
        perms: [...menagerie('p2', 4), ...bears('p3', 30), ...menagerie('p1', 3)],
      }),
    },
    {
      // Manual focus re-stages p4 into the far side (slots swap seats).
      view: seatTable({
        opponents: 3,
        active: 'p2',
        perms: [...menagerie('p2', 4), ...bears('p3', 30)],
      }),
      staging: { focusSeat: 'p4' },
    },
    {
      view: seatTable({
        opponents: 3,
        active: 'p3',
        eliminated: ['p2'],
        perms: [...menagerie('p3', 6), ...menagerie('p1', 5)],
      }),
    },
  ];

  it('matches a fresh mount after every reconcile of a real GameView sequence', () => {
    const r = make();
    for (const frame of frames) {
      const plane = planeOf(frame.view, frame.staging);
      r.reconcile(plane);
      expect(r.root.innerHTML).toBe(freshMount(plane));
    }
  });

  it('preserves the transform-bearing face node across a tap change (real CardFace)', () => {
    // A tap re-render must MORPH the existing face, not replace it: the
    // CSS transition on the face's inner node needs a persistent element to
    // interpolate the ~25° rotation on (tap/untap motion class). Under
    // reduced motion the face's own media query snaps it — the end state
    // below is byte-identical either way.
    const untapped = seatTable({
      opponents: 1,
      perms: [{ id: 'b1', controller: 'p2', name: 'Bear' }],
    });
    const tapped = seatTable({
      opponents: 1,
      perms: [{ id: 'b1', controller: 'p2', name: 'Bear', tapped: true }],
    });
    let current = untapped;
    const makeFace = () =>
      cardFaceRenderer((render) => {
        const perm = current.battlefield.find((p) => p.id === render.entityId)!;
        return toDisplayData(perm.card, {
          tapped: perm.tapped,
          counters: perm.counters,
          selected: false,
          actionable: false,
        });
      });

    const r = new PlaneReconciler(document.createElement('div'), { face: makeFace() });
    r.reconcile(planeOf(untapped));
    const faceRoot = r.elementFor('b1')!.firstElementChild as HTMLElement;
    const inner = faceRoot.querySelector<HTMLElement>('[data-monogram]')!;
    expect(faceRoot.style.getPropertyValue('--tap-rot')).toBe('0deg');

    current = tapped;
    r.reconcile(planeOf(tapped));
    // Same element instances: the rotation var changed ON the persistent
    // nodes, so the face's transform transition has a "from" to tween from.
    expect(r.elementFor('b1')!.firstElementChild).toBe(faceRoot);
    expect(faceRoot.querySelector('[data-monogram]')).toBe(inner);
    expect(faceRoot.dataset.tapped).toBe('true');
    expect(faceRoot.style.getPropertyValue('--tap-rot')).not.toBe('0deg');
    // The morphed DOM is byte-identical to a fresh render (the reduced-motion
    // and no-residue guarantee).
    const fresh = new PlaneReconciler(document.createElement('div'), { face: makeFace() });
    fresh.reconcile(planeOf(tapped));
    expect(r.root.innerHTML).toBe(fresh.root.innerHTML);
  });

  it('matches a fresh mount through the real CardFace renderer (consumes #479)', () => {
    const makeFace = (view: GameView) => {
      const byId = new Map(view.battlefield.map((p) => [p.id, p]));
      return cardFaceRenderer((render) => {
        const perm = byId.get(render.entityId)!;
        return toDisplayData(perm.card, {
          tapped: perm.tapped,
          counters: perm.counters,
          selected: false,
          actionable: false,
        });
      });
    };
    const view = seatTable({
      opponents: 3,
      active: 'p2',
      perms: [...menagerie('p2', 3), ...bears('p3', 20)],
    });
    const moved = seatTable({
      opponents: 3,
      active: 'p2',
      perms: [...menagerie('p2', 5), ...bears('p3', 20)],
    });

    const r = new PlaneReconciler(document.createElement('div'), { face: makeFace(view) });
    r.reconcile(planeOf(view));
    // Faces are the real DOM card component's markup.
    expect(r.root.querySelector('[data-entity-id] [role="img"]')).not.toBeNull();

    const incremental = new PlaneReconciler(document.createElement('div'), {
      face: makeFace(moved),
    });
    incremental.reconcile(planeOf(view));
    incremental.reconcile(planeOf(moved));
    expect(incremental.root.innerHTML).toBe(freshMount(planeOf(moved), () => makeFace(moved)));
  });

  it('reconciles zone-only updates into the pile and tile chrome (slots unmoved)', () => {
    const r = make();
    const before = seatTable({ opponents: 1, perms: menagerie('p2', 2) });
    r.reconcile(planeOf(before));
    const pile = r.root.querySelector<HTMLElement>('[data-key="piles:p2"]')!;
    expect(pile.dataset.library).toBe('60');
    expect(pile.dataset.graveyard).toBe('0');
    expect(pile.dataset.top).toBeUndefined();

    // A draw and a death: library shrinks, the graveyard gains a top card —
    // no slot or render moves, but the pile data is authoritative and must
    // reconcile (and count as work).
    const after = seatTable({ opponents: 1, perms: menagerie('p2', 2) });
    after.opponents[0]!.library_size = 59;
    after.graveyards = [
      {
        player_id: 'p2',
        cards: [{ id: 'g1', name: 'Shock', type_line: 'Instant', mana_cost: '{R}' }],
      },
    ];
    r.reconcile(planeOf(after));
    expect(r.lastStats.chrome).toBeGreaterThan(0);
    expect(pile.dataset.library).toBe('59');
    expect(pile.dataset.graveyard).toBe('1');
    expect(pile.dataset.top).toBe('Shock');
    expect(pile.dataset.topColor).toBe('R');
    expect(r.root.innerHTML).toBe(freshMount(planeOf(after)));
  });

  it('carries the zone counts on compact summary tiles too', () => {
    const r = make();
    const view = seatTable({ opponents: 3, active: 'p2' });
    r.reconcile(stagePlane(view, { width: 390, height: 844 }));
    const tile = r.root.querySelector<HTMLElement>('[data-key="tile:p3"]')!;
    expect(tile.dataset.library).toBe('60');

    const after = seatTable({ opponents: 3, active: 'p2' });
    after.opponents.find((o) => o.player_id === 'p3')!.library_size = 42;
    r.reconcile(stagePlane(after, { width: 390, height: 844 }));
    expect(tile.dataset.library).toBe('42');
    expect(r.lastStats.chrome).toBeGreaterThan(0);
  });

  it('rebuild() (reconnect/fast-forward) equals a fresh mount and drops the cache', () => {
    const r = make();
    r.reconcile(planeOf(seatTable({ opponents: 2, perms: menagerie('p2', 3) })));
    const plane = planeOf(seatTable({ opponents: 3, active: 'p3', perms: menagerie('p3', 5) }));
    r.rebuild(plane);
    expect(r.root.innerHTML).toBe(freshMount(plane));
    expect(r.hasPendingAnimations()).toBe(false);
  });

  it('reasserts slot and entity order after a focus swap', () => {
    const r = make();
    const view = seatTable({
      opponents: 3,
      active: 'p2',
      perms: [...menagerie('p2', 2), ...menagerie('p3', 2)],
    });
    r.reconcile(planeOf(view));
    const swapped = planeOf(view, { focusSeat: 'p3' });
    r.reconcile(swapped);
    expect(r.lastStats.reordered).toBe(true);
    expect(r.root.innerHTML).toBe(freshMount(swapped));
  });
});
