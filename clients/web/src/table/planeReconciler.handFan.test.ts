/**
 * Opponent hand fans **as the reconciler mounts them** (issue #533).
 *
 * `plane-handfan.test.ts` proves the staged geometry; this file proves what
 * actually reaches the DOM and what §9's travel resolves against:
 *
 * - one element per drawn back, and nothing on it that could identify a card;
 * - the same `--card-back-image` the library pile paints, with no second
 *   card-back path (`card-representation.md` §13: one silhouette across hand,
 *   library, travel, and piles);
 * - `hand:<seat>` terminating on a real fan slot, so `zone-geography.md` §9's
 *   draw row lands on the card's destination rather than the crest fallback.
 *
 * **jsdom limits.** Vitest applies no CSS module, so the stylesheet is read as
 * text where it is the thing under test — the idiom `card/back/cardBack.test.tsx`
 * uses. Nothing here shows a rendered back, a rotation, or a fan that looks like
 * a fan; that is the maintainer's browser check.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { SCENE_MOTION } from '../sceneTokens';
import { stagePlane, type StagedPlane } from './plane';
import { seatTable } from './plane.fixture';
import { PlaneReconciler, type PlaneFaceRenderer } from './planeReconciler';

const TRAVEL = SCENE_MOTION.zoneTravel.ms;
const BASELINE = { width: 1680, height: 945 };

function face(): PlaneFaceRenderer {
  return {
    signature: (render) => `${render.name}|${render.tier}`,
    render: (el, render) => {
      el.textContent = render.name;
    },
  };
}

function mount(animate = false): PlaneReconciler {
  return new PlaneReconciler(document.createElement('div'), { face: face(), animate });
}

function planeOf(opponents: number, handSize: number, viewport = BASELINE): StagedPlane {
  return stagePlane(seatTable({ opponents, handSize }), viewport);
}

/** Every fan element the reconciler mounted, in DOM order. */
function fanElements(r: PlaneReconciler): HTMLElement[] {
  return Array.from(r.root.querySelectorAll<HTMLElement>('[data-slot="handfan"]'));
}

describe('the fan reaches the DOM, one element per back', () => {
  it('mounts a back per drawn slot for every opponent, and none for the receiver', () => {
    const r = mount();
    const plane = planeOf(3, 7);
    r.reconcile(plane);
    const elements = fanElements(r);
    expect(elements).toHaveLength(3 * 7);
    const seats = new Set(elements.map((el) => el.dataset.seat));
    expect(seats).toEqual(new Set(['p2', 'p3', 'p4']));
    expect(elements.some((el) => el.dataset.seat === 'p1')).toBe(false);
  });

  it('keys each back by seat and slot so a reconcile never rebuilds the fan', () => {
    const r = mount();
    r.reconcile(planeOf(1, 5));
    const before = fanElements(r);
    expect(before.map((el) => el.dataset.key)).toEqual([
      'handfan:p2:0',
      'handfan:p2:1',
      'handfan:p2:2',
      'handfan:p2:3',
      'handfan:p2:4',
    ]);
    // A draw adds one back and keeps the other five elements — the fan reflows,
    // it does not flash.
    r.reconcile(planeOf(1, 6));
    const after = fanElements(r);
    expect(after).toHaveLength(6);
    for (let i = 0; i < 5; i += 1) expect(after[i]).toBe(before[i]);
  });

  it('costs nothing when the plane does not change', () => {
    const r = mount();
    const plane = planeOf(3, 12);
    r.reconcile(plane);
    r.reconcile(plane);
    expect(r.lastStats.chrome).toBe(0);
    expect(r.lastStats.reordered).toBe(false);
  });

  it('removes the backs a discard takes away', () => {
    const r = mount();
    r.reconcile(planeOf(1, 4));
    expect(fanElements(r)).toHaveLength(4);
    r.reconcile(planeOf(1, 1));
    expect(fanElements(r)).toHaveLength(1);
    r.reconcile(planeOf(1, 0));
    expect(fanElements(r)).toHaveLength(0);
  });

  it('is byte-identical to a fresh mount of the same plane', () => {
    // Fresh-mount equivalence, the reconciler's standing invariant, extended to
    // the new chrome kind.
    const reconciled = mount();
    reconciled.reconcile(planeOf(3, 4));
    reconciled.reconcile(planeOf(3, 9));
    const fresh = mount();
    fresh.reconcile(planeOf(3, 9));
    expect(reconciled.root.innerHTML).toBe(fresh.root.innerHTML);
  });
});

describe('hidden-information safety in the DOM (§13.1)', () => {
  it('publishes exactly two data attributes per back: its seat and its slot', () => {
    const r = mount();
    r.reconcile(planeOf(2, 9));
    for (const el of fanElements(r)) {
      expect(Object.keys(el.dataset).sort()).toEqual(['index', 'key', 'seat', 'slot']);
      // Nothing that could name a card, and — deliberately — not the count
      // either: the count has one home, the cluster's pip (§4/I5).
      expect(el.dataset.key!.startsWith(`handfan:${el.dataset.seat}:`)).toBe(true);
      expect(el.textContent).toBe('');
      expect(el.children).toHaveLength(0);
    }
  });

  it('carries one custom property — the slot’s rotation — and no other', () => {
    const r = mount();
    r.reconcile(planeOf(1, 7));
    for (const el of fanElements(r)) {
      const names = Array.from({ length: el.style.length }, (_, i) => el.style.item(i)).filter(
        (name) => name.startsWith('--'),
      );
      expect(names).toEqual(['--fan-angle']);
      expect(el.style.getPropertyValue('--fan-angle')).toMatch(/^-?\d+(\.\d+)?deg$/);
    }
  });

  it('gives every back in one fan the same box — no per-card width or height', () => {
    const r = mount();
    r.reconcile(planeOf(1, 11));
    const boxes = fanElements(r).map((el) => `${el.style.width}×${el.style.height}`);
    expect(new Set(boxes).size).toBe(1);
  });

  it('is identical for two seats holding the same number of cards', () => {
    // Same rung, same count ⇒ same fan, slot for slot, differing only in where
    // it sits. Nothing about the cards can leak through a difference.
    const r = mount();
    r.reconcile(stagePlane(seatTable({ opponents: 4, handSize: 6 }), BASELINE));
    const wings = fanElements(r).filter((el) => ['p3', 'p4'].includes(el.dataset.seat!));
    const left = wings.filter((el) => el.dataset.seat === 'p3');
    const right = wings.filter((el) => el.dataset.seat === 'p4');
    expect(left).toHaveLength(6);
    expect(right).toHaveLength(6);
    for (let i = 0; i < 6; i += 1) {
      expect(right[i]!.style.getPropertyValue('--fan-angle')).toBe(
        left[i]!.style.getPropertyValue('--fan-angle'),
      );
      expect(right[i]!.style.width).toBe(left[i]!.style.width);
      expect(right[i]!.style.top).toBe(left[i]!.style.top);
    }
  });
});

describe('the card back is the one already resolved (§13)', () => {
  const css = readFileSync(resolve(process.cwd(), 'src/table/live/live-plane.module.css'), 'utf8');
  const rule = css.slice(css.indexOf("[data-slot='handfan']"), css.length);

  it('paints `--card-back-image`, the same property the library pile paints', () => {
    expect(rule).toContain('--card-back-image');
    // …published once for the whole plane, so there is no per-fan channel.
    expect(css.match(/--card-back-image/g)!.length).toBeGreaterThanOrEqual(2);
  });

  it('keys its appearance on nothing but the slot itself', () => {
    // The same structural check `cardBack.test.tsx` makes for the library: no
    // selector on this path may read a card attribute.
    for (const forbidden of ['data-name', 'data-top', 'data-card', 'data-count']) {
      expect(rule, forbidden).not.toContain(forbidden);
    }
  });

  it('is never a hit target — an opponent’s hand offers nothing to pick', () => {
    expect(rule).toContain('pointer-events: none;');
  });
});

describe('`hand:<seat>` travel (`zone-geography.md` §9)', () => {
  it('lands a draw on the opponent’s fan slot, not on the crest fallback', () => {
    const r = mount(true);
    const before = planeOf(1, 5);
    r.reconcile(before);
    const after = planeOf(1, 6);
    r.reconcile(after, [
      {
        entityId: 'drawn',
        category: 'draw',
        from: 'pile:p2',
        to: 'hand:p2',
        durationMs: TRAVEL,
        delayMs: 0,
      },
    ]);
    const proxy = r.root.querySelector<HTMLElement>('[data-motion-proxy="draw"]')!;
    expect(proxy).not.toBeNull();
    // The proxy is sized by its DESTINATION, so its box is the fan's card box —
    // the 48×68 generic placeholder the old fallback produced is gone.
    const fan = after.farSide!.handFan!;
    expect(proxy.style.width).toBe(`${fan.card.w}px`);
    expect(proxy.style.height).toBe(`${fan.card.h}px`);
    expect(fan.anchor).toEqual(fan.slots[5]!.rect);
  });

  it('starts a discard at the fan slot the card was in', () => {
    const r = mount(true);
    const plane = planeOf(1, 6);
    r.reconcile(plane);
    r.reconcile(plane, [
      {
        entityId: 'pitched',
        category: 'discard',
        from: 'hand:p2',
        to: 'pile:p2',
        durationMs: TRAVEL,
        delayMs: 0,
      },
    ]);
    const proxy = r.root.querySelector<HTMLElement>('[data-motion-proxy="discard"]')!;
    const fan = plane.farSide!.handFan!;
    expect(proxy.style.left).toBe(`${fan.anchor.x}px`);
    expect(proxy.style.top).toBe(`${fan.anchor.y}px`);
  });

  it('still resolves for a seat whose rung draws no fan (rung 5 tiles)', () => {
    // A motion is retargeted, never retired (§7's resolution chain). On phone
    // portrait a peripheral seat is a summary tile with no fan, and the draw
    // terminates there rather than nowhere.
    const r = mount(true);
    const plane = stagePlane(seatTable({ opponents: 3, handSize: 6 }), {
      width: 390,
      height: 844,
    });
    expect(plane.tiles.length).toBeGreaterThan(0);
    const tileSeat = plane.tiles[0]!.seat;
    r.reconcile(plane);
    r.reconcile(plane, [
      {
        entityId: 'drawn',
        category: 'draw',
        from: `pile:${tileSeat}`,
        to: `hand:${tileSeat}`,
        durationMs: TRAVEL,
        delayMs: 0,
      },
    ]);
    expect(r.root.querySelector('[data-motion-proxy="draw"]')).not.toBeNull();
  });

  it('keeps the receiver’s own hand anchor a screen-space approximation', () => {
    // ADR 0032 §7: the receiver's hand is a shell region with no plane home, so
    // its anchor stays the box below their band — deliberately unchanged.
    const r = mount(true);
    const plane = planeOf(1, 5);
    expect(plane.receiver!.handFan).toBeUndefined();
    r.reconcile(plane);
    r.reconcile(plane, [
      {
        entityId: 'drawn',
        category: 'draw',
        from: 'pile:p1',
        to: 'hand:p1',
        durationMs: TRAVEL,
        delayMs: 0,
      },
    ]);
    const proxy = r.root.querySelector<HTMLElement>('[data-motion-proxy="draw"]')!;
    expect(proxy.style.width).toBe('48px');
    expect(proxy.style.height).toBe('68px');
  });
});
