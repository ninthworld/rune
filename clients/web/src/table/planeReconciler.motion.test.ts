/**
 * DOM plane reconciler — the motion layer (issue #481, visual-system §8):
 * FLIP travel with destination rects addressable at 0 ms, travel ghosts for
 * zone changes, staging tweens for slot/focus changes, batch stagger inside
 * the ≤ 800 ms window, retargeting by a newer authoritative scene, the
 * reduced-motion snap (byte-identical to the un-animated path), and the
 * skippability contract. All timestamps are controlled — no wall clock, no
 * requestAnimationFrame.
 */
import { describe, expect, it } from 'vitest';
import { SCENE_BATCH, SCENE_MOTION } from '../sceneTokens';
import { stagePlane, type StagedPlane } from './plane';
import { seatTable, menagerie, DESKTOP } from './plane.fixture';
import { PlaneReconciler, type PlaneFaceRenderer } from './planeReconciler';

const TRAVEL = SCENE_MOTION.zoneTravel.ms;
const STAGING = SCENE_MOTION.staging.ms;

/** A minimal deterministic face renderer. */
function face(): PlaneFaceRenderer {
  return {
    signature: (render) => `${render.name}|${render.tier}|${render.stackCount}`,
    render: (el, render) => {
      el.textContent = render.name;
    },
  };
}

function animated(): PlaneReconciler {
  return new PlaneReconciler(document.createElement('div'), { face: face(), animate: true });
}

function planeOf(perms: Parameters<typeof seatTable>[0]['perms'], staging?: object): StagedPlane {
  return stagePlane(seatTable({ opponents: 1, perms }), DESKTOP, staging);
}

/** The wrapper's current FLIP offset, parsed from its transform. */
function offsetOf(el: HTMLElement): { dx: number; dy: number } | null {
  const match = /translate\((-?[\d.]+)px, (-?[\d.]+)px\)/.exec(el.style.transform);
  return match ? { dx: Number(match[1]), dy: Number(match[2]) } : null;
}

describe('PlaneReconciler FLIP travel (input never gated)', () => {
  it('lands the layout box on the final rect at 0 ms; only a transform decays', () => {
    const r = animated();
    r.reconcile(planeOf(menagerie('p2', 2)));
    r.advance(0);
    r.advance(TRAVEL); // settle the entrances
    const before = { ...r.targetFor('p2_beast_0')! };

    const moved = planeOf(menagerie('p2', 6)); // reflow shifts the row
    r.reconcile(moved);
    const target = r.targetFor('p2_beast_0')!;
    const el = r.elementFor('p2_beast_0')!;
    expect(target).not.toEqual(before);
    // The authoritative rect is the layout box immediately…
    expect(el.style.left).toBe(`${target.x}px`);
    expect(el.style.top).toBe(`${target.y}px`);
    // …while the visual starts offset by exactly the old position (FLIP invert).
    const offset = offsetOf(el)!;
    expect(target.x + offset.dx).toBeCloseTo(before.x);
    expect(target.y + offset.dy).toBeCloseTo(before.y);
    // Effects follow current pixels through the separate visual seam; input
    // keeps using targetFor's already-authoritative destination.
    expect(r.visualFor('p2_beast_0')).toMatchObject({
      x: expect.closeTo(before.x),
      y: expect.closeTo(before.y),
    });

    r.advance(TRAVEL); // anchor the clock
    r.advance(TRAVEL + TRAVEL / 2);
    const mid = offsetOf(el);
    if (mid) expect(Math.abs(mid.dx)).toBeLessThan(Math.abs(offset.dx));
    expect(r.visualFor('p2_beast_0')!.x).not.toBe(target.x);

    r.advance(TRAVEL * 3);
    expect(el.style.transform).toBe('');
    expect(r.visualFor('p2_beast_0')).toEqual(target);
    expect(r.hasPendingAnimations()).toBe(false);
  });

  it('retargets an in-flight move from its current visual offset (no snap)', () => {
    const r = animated();
    r.reconcile(planeOf(menagerie('p2', 2)));
    r.advance(0);
    r.advance(TRAVEL);

    r.reconcile(planeOf(menagerie('p2', 6)));
    r.advance(TRAVEL); // anchor
    r.advance(TRAVEL + TRAVEL / 2); // halfway
    const el = r.elementFor('p2_beast_0')!;
    const midOffset = offsetOf(el)!;
    const midVisualX = r.targetFor('p2_beast_0')!.x + midOffset.dx;

    // A newer authoritative plane arrives mid-flight: the tween re-anchors
    // from the CURRENT visual position toward the new target.
    const third = planeOf(menagerie('p2', 4));
    r.reconcile(third);
    const retargeted = offsetOf(el)!;
    expect(r.targetFor('p2_beast_0')!.x + retargeted.dx).toBeCloseTo(midVisualX, 1);

    r.advance(TRAVEL * 10);
    r.advance(TRAVEL * 12);
    expect(el.style.transform).toBe('');
  });

  it('discards motion with an entity that leaves mid-flight', () => {
    const r = animated();
    r.reconcile(planeOf(menagerie('p2', 3)));
    r.advance(0);
    r.advance(TRAVEL);
    r.reconcile(planeOf(menagerie('p2', 6)));
    // beast_2 leaves while its reflow tween is pending.
    r.reconcile(planeOf(menagerie('p2', 2)));
    expect(r.elementFor('p2_beast_2')).toBeUndefined();
    r.advance(TRAVEL * 20);
    r.advance(TRAVEL * 22);
    expect(r.hasPendingAnimations()).toBe(false);
  });
});

describe('PlaneReconciler travel ghosts (zone changes)', () => {
  it('renders cross-surface draw/cast travel even without a battlefield wrapper', () => {
    const r = animated();
    const empty = planeOf([]);
    r.reconcile(empty);

    r.reconcile(empty, [
      {
        entityId: 'drawn-card',
        category: 'draw',
        from: 'pile:p2',
        to: 'hand:p2',
        durationMs: TRAVEL,
        delayMs: 0,
      },
    ]);

    const proxy = r.root.querySelector<HTMLElement>('[data-motion-proxy="draw"]')!;
    expect(proxy).not.toBeNull();
    expect(proxy.dataset.entityId).toBeUndefined();
    expect(proxy.dataset.motionEntity).toBe('drawn-card');
    expect(proxy.style.pointerEvents).toBe('none');
    r.advance(0);
    r.advance(TRAVEL / 2);
    expect(proxy.style.transform).toContain('translate');
    r.advance(TRAVEL);
    expect(r.root.querySelector('[data-motion-proxy="draw"]')).toBeNull();
  });

  it('discards an older cross-surface proxy when a newer view supersedes it', () => {
    const r = animated();
    const empty = planeOf([]);
    r.reconcile(empty, [
      {
        entityId: 'old-card',
        category: 'cast',
        from: 'hand:p2',
        to: 'stack:old-card',
        durationMs: TRAVEL,
        delayMs: 0,
      },
    ]);
    expect(r.root.querySelector('[data-motion-entity="old-card"]')).not.toBeNull();

    r.discardMotionProxies();
    r.reconcile(empty, [
      {
        entityId: 'new-card',
        category: 'draw',
        from: 'pile:p2',
        to: 'hand:p2',
        durationMs: TRAVEL,
        delayMs: 0,
      },
    ]);

    expect(r.root.querySelector('[data-motion-entity="old-card"]')).toBeNull();
    expect(r.root.querySelector('[data-motion-entity="new-card"]')).not.toBeNull();
  });

  it('enters at the final rect, fading up while a ghost travels from the piles', () => {
    const r = animated();
    r.reconcile(planeOf(menagerie('p2', 1)));
    const el = r.elementFor('p2_beast_0')!;
    // Addressable at the destination at once; pixels fade up.
    expect(r.targetFor('p2_beast_0')).toBeDefined();
    expect(el.style.opacity).toBe('0');
    const ghost = r.root.querySelector<HTMLElement>('[data-ghost]')!;
    expect(ghost).not.toBeNull();
    // The ghost is decorative and never a hit-target.
    expect(ghost.style.pointerEvents).toBe('none');
    expect(ghost.dataset.entityId).toBeUndefined();

    r.advance(0);
    r.advance(TRAVEL / 2);
    expect(Number(el.style.opacity)).toBeGreaterThan(0);
    expect(ghost.style.transform).toContain('translate');

    r.advance(TRAVEL);
    expect(el.style.opacity).toBe('');
    expect(r.root.querySelector('[data-ghost]')).toBeNull();
    expect(r.hasPendingAnimations()).toBe(false);
  });

  it('removes a leaving entity immediately; only its ghost fades out to the piles', () => {
    const r = animated();
    r.reconcile(planeOf(menagerie('p2', 2)));
    r.advance(0);
    r.advance(SCENE_BATCH.windowMs); // settle the staggered entrances fully
    expect(r.hasPendingAnimations()).toBe(false);

    r.reconcile(planeOf(menagerie('p2', 1)));
    // Not addressable the instant it leaves…
    expect(r.elementFor('p2_beast_1')).toBeUndefined();
    expect(r.targetFor('p2_beast_1')).toBeUndefined();
    expect(r.root.querySelectorAll('[data-entity-id]')).toHaveLength(1);
    // …while a decorative ghost fades toward the seat's zone home.
    const ghost = r.root.querySelector<HTMLElement>('[data-ghost]')!;
    r.advance(1000);
    r.advance(1000 + TRAVEL / 2);
    expect(Number(ghost.style.opacity)).toBeLessThan(1);
    r.advance(1000 + TRAVEL * 2);
    expect(r.root.querySelector('[data-ghost]')).toBeNull();
  });

  it('staggers a simultaneous batch inside the ≤ 800 ms window', () => {
    const r = animated();
    // A 12-card swarm enters at once.
    r.reconcile(planeOf(menagerie('p2', 12)));
    r.advance(0);
    r.advance(SCENE_BATCH.staggerMs + 1);
    // The first item is further along than a later one (per-item stagger)…
    const first = r.elementFor('p2_beast_0')!;
    const later = r.elementFor('p2_beast_9')!;
    expect(Number(first.style.opacity)).toBeGreaterThan(Number(later.style.opacity));
    // …and the whole batch lands inside the window.
    r.advance(SCENE_BATCH.windowMs);
    expect(r.hasPendingAnimations()).toBe(false);
    expect(r.root.querySelector('[data-ghost]')).toBeNull();
  });
});

describe('PlaneReconciler staging tweens (slot / focus changes)', () => {
  it('tweens a re-staged slot in the staging class, outlasting card travel', () => {
    const view = seatTable({ opponents: 3, active: 'p2', perms: menagerie('p3', 2) });
    const r = animated();
    r.reconcile(stagePlane(view, DESKTOP));
    r.advance(0);
    r.advance(1000);

    // Manual focus swaps p3 into the far side: its region slot re-stages.
    r.reconcile(stagePlane(view, DESKTOP, { focusSeat: 'p3' }));
    const region = r.root.querySelector<HTMLElement>('[data-key="region:p3"]')!;
    expect(region.dataset.kind).toBe('far');
    expect(region.style.transform).toContain('translate');

    r.advance(1000);
    r.advance(1000 + TRAVEL + 1); // card travel class has finished…
    expect(region.style.transform).toContain('translate'); // …staging has not
    r.advance(1000 + STAGING);
    expect(region.style.transform).toBe('');
    expect(r.hasPendingAnimations()).toBe(false);
  });
});

describe('PlaneReconciler reduced motion and skippability', () => {
  it('reduced motion is byte-identical to the un-animated path, advance inert', () => {
    const sequence = [planeOf(menagerie('p2', 2)), planeOf(menagerie('p2', 5))];
    const reduced = new PlaneReconciler(document.createElement('div'), {
      face: face(),
      animate: { reducedMotion: true },
    });
    const plain = new PlaneReconciler(document.createElement('div'), { face: face() });
    for (const plane of sequence) {
      reduced.reconcile(plane);
      plain.reconcile(plane);
      expect(reduced.root.innerHTML).toBe(plain.root.innerHTML);
    }
    expect(reduced.hasPendingAnimations()).toBe(false);
    reduced.advance(1000);
    expect(reduced.root.innerHTML).toBe(plain.root.innerHTML);
  });

  it('skipTransitions() completes a long composition instantly on the final layout', () => {
    // A batch composition can exceed 600 ms (the ≤ 800 ms window), so it is
    // user-skippable; single-class tweens stay under 600 ms and are not.
    expect(SCENE_BATCH.windowMs).toBeGreaterThan(600);
    for (const spec of Object.values(SCENE_MOTION)) {
      expect(spec.ms).toBeLessThanOrEqual(600);
    }
    const r = animated();
    r.reconcile(planeOf(menagerie('p2', 12)));
    r.advance(0);
    r.advance(50); // mid-composition
    expect(r.hasPendingAnimations()).toBe(true);

    r.skipTransitions();
    expect(r.hasPendingAnimations()).toBe(false);
    expect(r.root.querySelector('[data-ghost]')).toBeNull();
    // The skipped result is the exact layout the plane made authoritative.
    const plain = new PlaneReconciler(document.createElement('div'), { face: face() });
    plain.reconcile(planeOf(menagerie('p2', 12)));
    expect(r.root.innerHTML).toBe(plain.root.innerHTML);
  });
});
