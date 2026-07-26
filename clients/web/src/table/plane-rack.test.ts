/**
 * The per-seat zone rack (`docs/design/zone-geography.md`), staged by
 * `table/plane/rack.ts` for issue #531.
 *
 * Every assertion below names the clause it holds: the fixed slot order (§1),
 * the anchor table and command rule (§2.2), the pitch / 44 px packing rule and
 * its `u ≥ 30 px` digest trigger (§2.3 [D4]), corridor clearance with the 12 px
 * halo (§2.4 [D5]), the four variants (§6), the anchor keys (§7), and count
 * ownership (§4 / I5).
 *
 * jsdom limits, stated rather than papered over: nothing here proves a rendered
 * pile, a real 44 px touch target on glass, or that a graveyard and an exile are
 * distinguishable by outline. Those are the maintainer's browser checks. What is
 * proven is the declared geometry and the data contract the renderer reads.
 */
import { describe, expect, it } from 'vitest';
import type { GameView } from '../protocol';
import { rectsOverlap, type Rect } from './scene';
import { PLANE, RACK_ZONES, digestExpansionRects, type SeatRack } from './plane';
import { DESKTOP, TABLET, ULTRAWIDE, regionsOf, seatTable, stage } from './plane.fixture';

const BASELINE = { width: 1680, height: 945 };

/** Grow a rect by a halo on every side. */
function halo(rect: Rect, by: number): Rect {
  return { x: rect.x - by, y: rect.y - by, w: rect.w + 2 * by, h: rect.h + 2 * by };
}

/** A view whose commander data is present, so the command slot exists (§5). */
function commanderTable(opts: Parameters<typeof seatTable>[0]): GameView {
  const view = seatTable(opts);
  return {
    ...view,
    commander_tax: [{ commander: 'p1', casts: 1, tax: 2 }],
  };
}

describe('zone rack — the fixed slot order and the anchor table (§1, §2.2)', () => {
  it('orders library, graveyard, exile along the reading axis at every seat', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2' }), BASELINE);
    for (const region of regionsOf(plane)) {
      const zones = region.rack.slots.map((slot) => slot.zone);
      // §I6: exactly the reserved anchors, in the fixed order, no generic bucket.
      expect(zones.every((zone) => RACK_ZONES.includes(zone))).toBe(true);
      expect(zones.slice(0, 3)).toEqual(['library', 'graveyard', 'exile']);
      if (region.rack.variant === 'digest') continue;
      // "Order is fixed and never reverses" — monotone along the rack's axis.
      const along = region.rack.slots
        .filter((slot) => slot.zone !== 'command')
        .map((slot) => (region.rack.axis === 'vertical' ? slot.rect.y : slot.rect.x));
      for (let i = 1; i < along.length; i += 1) {
        expect(along[i]!).toBeGreaterThan(along[i - 1]!);
      }
    }
  });

  it('holds the §2.3 pitch of 1.60u between pile slots', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2' }), BASELINE);
    for (const region of regionsOf(plane)) {
      const rack = region.rack;
      if (rack.variant === 'digest') continue;
      const piles = rack.slots.filter((slot) => slot.zone !== 'command');
      for (let i = 1; i < piles.length; i += 1) {
        const step =
          rack.axis === 'vertical'
            ? piles[i]!.rect.y - piles[i - 1]!.rect.y
            : piles[i]!.rect.x - piles[i - 1]!.rect.x;
        expect(step).toBeCloseTo(PLANE.rack.pitch * rack.u, 6);
      }
    }
  });

  it('sizes a pile card u × 1.4u and the command slot 1.35u wide (§2.1, §2.2)', () => {
    const plane = stage(commanderTable({ opponents: 3, active: 'p2' }), BASELINE);
    const local = plane.receiver!.rack;
    expect(local.variant).toBe('local');
    const library = local.slots.find((slot) => slot.zone === 'library')!;
    expect(library.rect.w).toBeCloseTo(local.u, 6);
    expect(library.rect.h).toBeCloseTo(local.u * PLANE.rack.pileAspect, 6);
    const command = local.slots.find((slot) => slot.zone === 'command')!;
    expect(command.rect.w).toBeCloseTo(local.u * PLANE.rack.commandScale, 6);
    // "The command slot is the largest element" — §1 fact 5.
    expect(command.rect.w).toBeGreaterThan(library.rect.w);
    expect(command.rect.h).toBeGreaterThan(library.rect.h);
  });

  it('lands the command slot inboard of the whole cluster at every seat (§1 fact 4)', () => {
    const plane = stage(commanderTable({ opponents: 3, active: 'p2' }), BASELINE);
    for (const region of regionsOf(plane)) {
      const rack = region.rack;
      if (rack.variant === 'digest') continue;
      const command = rack.slots.find((slot) => slot.zone === 'command');
      if (!command) continue;
      const piles = rack.slots.filter((slot) => slot.zone !== 'command');
      const centre = (rect: Rect) => ({ x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 });
      const towardCentre = (rect: Rect) =>
        rack.axis === 'vertical'
          ? Math.abs(centre(rect).x - plane.width / 2)
          : Math.abs(centre(rect).y - plane.height / 2);
      // Closer to the table centre than every pile in the strip.
      for (const pile of piles) {
        expect(towardCentre(command.rect)).toBeLessThan(towardCentre(pile.rect));
      }
    }
  });

  it('omits the command slot entirely when the view carries no commander data (§5, G3)', () => {
    // "Absent. The slot is not drawn, not reserved, not spaced for." — the
    // documented fail-closed answer while `GameView` has no format signal.
    const plane = stage(seatTable({ opponents: 3, active: 'p2' }), BASELINE);
    for (const region of regionsOf(plane)) {
      expect(region.rack.slots.map((s) => s.zone)).not.toContain('command');
      expect(region.rack.slots).toHaveLength(3);
    }
    // And it appears the moment the wire names a commander.
    const commander = stage(commanderTable({ opponents: 3, active: 'p2' }), BASELINE);
    for (const region of regionsOf(commander)) {
      expect(region.rack.slots).toHaveLength(4);
    }
  });
});

describe('zone rack — packing, the 44 px floor, and the digest trigger (§2.3)', () => {
  const geometries = [DESKTOP, BASELINE, TABLET, ULTRAWIDE];

  it('never overlaps two hit rects inside one rack, at any count or geometry', () => {
    for (const viewport of geometries) {
      for (const opponents of [1, 2, 3, 4, 5]) {
        for (const region of regionsOf(
          stage(commanderTable({ opponents, active: 'p2' }), viewport),
        )) {
          const rack = region.rack;
          if (rack.variant === 'digest') continue;
          for (let i = 0; i < rack.slots.length; i += 1) {
            for (let j = i + 1; j < rack.slots.length; j += 1) {
              expect({
                seat: region.seat,
                a: rack.slots[i]!.zone,
                b: rack.slots[j]!.zone,
                overlap: rectsOverlap(rack.slots[i]!.hitRect, rack.slots[j]!.hitRect),
              }).toEqual({
                seat: region.seat,
                a: rack.slots[i]!.zone,
                b: rack.slots[j]!.zone,
                overlap: false,
              });
            }
          }
        }
      }
    }
  });

  it('keeps every rack hotspot at the 44 px floor, drawn footprint unchanged', () => {
    for (const viewport of geometries) {
      for (const opponents of [1, 2, 3, 4, 5]) {
        for (const region of regionsOf(stage(seatTable({ opponents, active: 'p2' }), viewport))) {
          for (const slot of region.rack.slots) {
            expect(slot.hitRect.w).toBeGreaterThanOrEqual(PLANE.minHit);
            expect(slot.hitRect.h).toBeGreaterThanOrEqual(PLANE.minHit);
            // The hotspot grows around the drawn box; the box never grows to meet it.
            expect(slot.hitRect.w).toBeGreaterThanOrEqual(slot.rect.w);
            expect(slot.hitRect.h).toBeGreaterThanOrEqual(slot.rect.h);
          }
        }
      }
    }
  });

  it('never draws a rack below the u ≥ 30 px floor — it digests instead ([D4])', () => {
    for (const viewport of geometries) {
      for (const opponents of [1, 2, 3, 4, 5]) {
        for (const region of regionsOf(
          stage(commanderTable({ opponents, active: 'p2' }), viewport),
        )) {
          const rack = region.rack;
          if (rack.variant === 'digest') {
            expect(rack.u).toBe(0);
          } else {
            expect(rack.u).toBeGreaterThanOrEqual(PLANE.rack.minU);
            // The floor exists so the along-axis pitch clears 48 px.
            expect(PLANE.rack.pitch * rack.u).toBeGreaterThanOrEqual(48);
          }
        }
      }
    }
  });
});

describe('zone rack — corridor clearance (§2.4)', () => {
  it('keeps the hit-rect union plus a 12 px halo out of the corridor', () => {
    for (const viewport of [DESKTOP, BASELINE, TABLET, ULTRAWIDE]) {
      for (const opponents of [1, 2, 3, 4, 5]) {
        const plane = stage(commanderTable({ opponents, active: 'p2' }), viewport);
        for (const region of regionsOf(plane)) {
          expect({
            seat: region.seat,
            clear: rectsOverlap(halo(region.rack.bounds, PLANE.rack.halo), plane.corridor),
          }).toEqual({ seat: region.seat, clear: false });
        }
      }
    }
  });

  it('never lets two seats’ racks share a clearance zone (§2.4.4)', () => {
    for (const viewport of [DESKTOP, BASELINE, ULTRAWIDE]) {
      for (const opponents of [1, 2, 3, 4, 5]) {
        const racks = regionsOf(stage(commanderTable({ opponents, active: 'p2' }), viewport)).map(
          (region) => region.rack,
        );
        for (let i = 0; i < racks.length; i += 1) {
          for (let j = i + 1; j < racks.length; j += 1) {
            expect(rectsOverlap(racks[i]!.bounds, racks[j]!.bounds)).toBe(false);
          }
        }
      }
    }
  });

  it('never lets a rack overlap the cards of the board it belongs to', () => {
    // The rack owns its region's outer flank; the board stages inboard of it, so
    // zones and cards can never contend for one rect at any rung.
    for (const viewport of [DESKTOP, BASELINE]) {
      for (const opponents of [1, 2, 3, 5]) {
        const plane = stage(
          commanderTable({
            opponents,
            active: 'p2',
            perms: Array.from({ length: 40 }, (_, i) => ({
              id: `p2_c${i}`,
              controller: 'p2',
              name: `Beast ${i}`,
              power: String(1 + (i % 9)),
            })),
          }),
          viewport,
        );
        for (const region of regionsOf(plane)) {
          for (const render of region.renders) {
            expect(rectsOverlap(render.rect, region.rack.bounds)).toBe(false);
          }
        }
      }
    }
  });
});

describe('zone rack — the four variants (§6)', () => {
  it('gives the receiver the local variant and the focused seat the focused one', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2' }), BASELINE);
    expect(plane.receiver!.rack.variant).toBe('local');
    expect(plane.farSide!.rack.variant).toBe('focused');
    // The local rack is the largest; the focused one is a tier down.
    expect(plane.receiver!.rack.u).toBeGreaterThan(plane.farSide!.rack.u);
  });

  it('runs the receiver’s rack vertically and the far side’s horizontally (§2.5)', () => {
    const plane = stage(seatTable({ opponents: 3, active: 'p2' }), BASELINE);
    expect(plane.receiver!.rack.axis).toBe('vertical');
    expect(plane.farSide!.rack.axis).toBe('horizontal');
    for (const wing of plane.wings) expect(wing.rack.axis).toBe('vertical');
  });

  it('digests a wing whose board is a digest baseline (5–6 seats)', () => {
    const plane = stage(seatTable({ opponents: 5, active: 'p2' }), BASELINE);
    expect(plane.wings).toHaveLength(4);
    for (const wing of plane.wings) {
      expect(wing.rung).toBe(4);
      expect(wing.rack.variant).toBe('digest');
      // The digest keeps all four (here three) anchors, resolving to one button.
      expect(wing.rack.slots).toHaveLength(3);
      const rects = new Set(wing.rack.slots.map((s) => `${s.hitRect.x},${s.hitRect.y}`));
      expect(rects.size).toBe(1);
      expect(wing.rack.bounds.w).toBeGreaterThanOrEqual(PLANE.minHit);
      expect(wing.rack.bounds.h).toBeGreaterThanOrEqual(PLANE.minHit);
    }
  });

  /**
   * §6.1's digest form is "one rack button with four **shaped** sub-indicators".
   * The shipped button drew a column of bare numbers — `28 / 1 / 0` in a
   * featureless dark rectangle, nothing saying which number was which zone, and
   * the command count dropped entirely (issue #582 §5). The numbers are the one
   * part of a pile that is NOT its identity, so a digest that keeps only the
   * numbers has kept the wrong half.
   */
  it('keeps every zone’s identity in the digest, one shaped chip each', () => {
    for (const [zones, table] of [
      [3, seatTable],
      [4, commanderTable],
    ] as const) {
      const plane = stage(table({ opponents: 5, active: 'p2' }), BASELINE);
      for (const wing of plane.wings) {
        expect(wing.rack.variant).toBe('digest');
        // One chip per zone anchor, in the fixed §1 order, never fewer.
        expect(wing.rack.indicators.map((i) => i.zone)).toEqual(wing.rack.slots.map((s) => s.zone));
        expect(wing.rack.indicators).toHaveLength(zones);
        // Each carries its own count — the number is tied to a zone rather than
        // floating in a column.
        for (const indicator of wing.rack.indicators) {
          const slot = wing.rack.slots.find((s) => s.zone === indicator.zone)!;
          expect(indicator.count).toBe(slot.count);
        }
        // The chips are separate, drawn boxes inside the button — so a
        // stylesheet can give each its own material and silhouette.
        for (const indicator of wing.rack.indicators) {
          expect(indicator.rect.w).toBeGreaterThan(0);
          expect(indicator.rect.h).toBeGreaterThan(0);
          expect(indicator.rect.x).toBeGreaterThanOrEqual(wing.rack.bounds.x);
          expect(indicator.rect.y).toBeGreaterThanOrEqual(wing.rack.bounds.y);
          expect(indicator.rect.x + indicator.rect.w).toBeLessThanOrEqual(
            wing.rack.bounds.x + wing.rack.bounds.w,
          );
          expect(indicator.rect.y + indicator.rect.h).toBeLessThanOrEqual(
            wing.rack.bounds.y + wing.rack.bounds.h,
          );
        }
        for (let i = 0; i < wing.rack.indicators.length; i += 1) {
          for (let j = i + 1; j < wing.rack.indicators.length; j += 1) {
            expect(rectsOverlap(wing.rack.indicators[i]!.rect, wing.rack.indicators[j]!.rect)).toBe(
              false,
            );
          }
        }
        // …and the button still clears the touch floor around them.
        expect(wing.rack.bounds.w).toBeGreaterThanOrEqual(PLANE.minHit);
        expect(wing.rack.bounds.h).toBeGreaterThanOrEqual(PLANE.minHit);
      }
    }
  });

  it('draws the same digest block for every seat, whichever axis its rack runs on', () => {
    // §2.5 orients a DRAWN rack with its seat, which is the rule the maintainer
    // read as "two different components". The digest is where that reading did
    // real damage, and it is the one form that is identical everywhere: the same
    // grid, the same order, the same chips (issue #582 §5).
    const plane = stage(seatTable({ opponents: 5, active: 'p2' }), BASELINE);
    const shapes = plane.wings.map((wing) =>
      wing.rack.indicators
        .map((i) => `${i.zone}@${i.rect.x - wing.rack.bounds.x},${i.rect.y - wing.rack.bounds.y}`)
        .join('|'),
    );
    expect(new Set(shapes).size).toBe(1);
    // Including the two flanks, whose drawn racks would sit on opposite sides.
    expect(new Set(plane.wings.map((w) => w.side)).size).toBe(2);
  });

  it('drops no zone from a digest — the command slot is drawn, not omitted', () => {
    // The shipped button's `::after` listed library, graveyard, and exile and
    // simply had no fourth line, so a Commander game's digested rack silently
    // lost its command zone (issue #582 §5).
    const plane = stage(commanderTable({ opponents: 5, active: 'p2' }), BASELINE);
    for (const wing of plane.wings) {
      expect(wing.rack.variant).toBe('digest');
      expect(wing.rack.indicators.map((i) => i.zone)).toEqual([...RACK_ZONES]);
    }
  });

  it('states no drawn variant’s sub-indicators — those zones have their own slots', () => {
    const plane = stage(seatTable({ opponents: 1, active: 'p2' }), BASELINE);
    for (const region of [plane.receiver!, plane.farSide!]) {
      if (region.rack.variant === 'digest') continue;
      expect(region.rack.indicators).toEqual([]);
    }
  });

  /**
   * §6.2's expansion, as geometry. A digest rack resolving every zone key to one
   * rect is right for *anchors* (§7) and unusable for *targets*: controls
   * positioned from it coincide, so only the last one painted can be reached by
   * pointer or touch. `digestExpansionRects` is what the interaction layer opens
   * the seat's zones onto, so it owes separable, on-plane, ≥ 44 px rects.
   */
  it('expands a digest rack onto separable, on-plane ≥ 44 px rects (§6.2)', () => {
    const plane = stage(seatTable({ opponents: 5, active: 'p2' }), BASELINE);
    for (const wing of plane.wings) {
      const rects = digestExpansionRects(wing.rack, 2, plane);
      expect(rects).toHaveLength(2);
      for (const rect of [wing.rack.bounds, ...rects]) {
        expect(rect.w).toBeGreaterThanOrEqual(PLANE.minHit);
        expect(rect.h).toBeGreaterThanOrEqual(PLANE.minHit);
        // Never staged off-canvas, whichever end of its flank the rack sits at.
        expect(rect.x).toBeGreaterThanOrEqual(0);
        expect(rect.y).toBeGreaterThanOrEqual(0);
        expect(rect.x + rect.w).toBeLessThanOrEqual(plane.width);
        expect(rect.y + rect.h).toBeLessThanOrEqual(plane.height);
      }
      // The whole point: no two of the three controls share any area.
      const all = [wing.rack.bounds, ...rects];
      for (let i = 0; i < all.length; i += 1) {
        for (let j = i + 1; j < all.length; j += 1) {
          expect(rectsOverlap(all[i]!, all[j]!)).toBe(false);
        }
      }
    }
  });

  it('reverses the expansion rather than running it off the plane', () => {
    const rack: SeatRack = {
      seat: 'p1',
      variant: 'digest',
      axis: 'vertical',
      u: 0,
      origin: { x: 100, y: 400 },
      slots: [],
      indicators: [],
      // Pinned to the plane's bottom edge: expanding downward would leave it.
      bounds: { x: 80, y: 418, w: 44, h: 62 },
      inset: { left: 0, right: 0 },
    };
    const rects = digestExpansionRects(rack, 2, { width: 640, height: 480 });
    for (const rect of rects) {
      expect(rect.y).toBeLessThan(rack.bounds.y);
      expect(rect.y).toBeGreaterThanOrEqual(0);
    }
    expect(rectsOverlap(rects[0]!, rects[1]!)).toBe(false);
  });

  it('draws a real wing rack where the flank has room, and digests where it does not', () => {
    // The fit rule is a rule, not a constant: the same 4-player staging draws a
    // three-slot wing rack at 1680×945 and falls to the digest at 1280×800,
    // where the visible flank cannot hold three separable 44 px targets beside a
    // board worth drawing.
    const wide = stage(seatTable({ opponents: 3, active: 'p2' }), BASELINE);
    expect(wide.wings.every((w) => w.rack.variant === 'wing')).toBe(true);
    const narrow = stage(seatTable({ opponents: 3, active: 'p2' }), DESKTOP);
    expect(narrow.wings.every((w) => w.rack.variant === 'digest')).toBe(true);
    // Either way the wing still draws a board — the rack degrades, not the cards.
    expect(narrow.wings.every((w) => w.rung < 4)).toBe(true);
  });

  it('mirrors only the command slot on the right-hand flank (§2.2, [D1])', () => {
    const plane = stage(commanderTable({ opponents: 3, active: 'p2' }), BASELINE);
    const stripSideOf = (rack: SeatRack): number => {
      const library = rack.slots.find((s) => s.zone === 'library')!;
      return library.rect.x + library.rect.w / 2 - rack.origin.x;
    };
    for (const region of regionsOf(plane)) {
      if (region.rack.variant === 'digest') continue;
      // The strip is screen-right of the identity anchor at every vertical rack
      // and screen-below it at the horizontal one — never mirrored.
      if (region.rack.axis === 'vertical') expect(stripSideOf(region.rack)).toBeGreaterThan(0);
      else {
        const library = region.rack.slots.find((s) => s.zone === 'library')!;
        expect(library.rect.y + library.rect.h / 2 - region.rack.origin.y).toBeGreaterThan(0);
      }
    }
  });
});

describe('zone rack — count ownership and the anchor keys (§4, §7)', () => {
  it('carries each zone’s count on that zone’s own slot and nowhere else', () => {
    const view = seatTable({ opponents: 3, active: 'p2' });
    view.graveyards = [
      { player_id: 'p2', cards: [{ id: 'g1', name: 'Shock', type_line: 'Instant' }] },
    ];
    view.exile = [
      {
        player_id: 'p2',
        cards: [
          { id: 'x1', name: 'Gone', type_line: 'Creature' },
          { id: 'x2', name: 'Also Gone', type_line: 'Creature' },
        ],
      },
    ];
    const rack = stage(view, BASELINE).farSide!.rack;
    const countOf = (zone: string) => rack.slots.find((s) => s.zone === zone)!.count;
    expect(countOf('library')).toBe(60);
    expect(countOf('graveyard')).toBe(1);
    expect(countOf('exile')).toBe(2);
  });

  it('publishes a resolvable rect for every reserved anchor, at 0 ms', () => {
    // §7: "Anchors exist at their final rects the moment the scene is built",
    // including for empty zones — an animation is never allowed to invent a
    // target, so a motion is retargeted rather than retired.
    for (const viewport of [DESKTOP, BASELINE, ULTRAWIDE]) {
      for (const opponents of [1, 2, 3, 4, 5]) {
        for (const region of regionsOf(
          stage(commanderTable({ opponents, active: 'p2' }), viewport),
        )) {
          for (const slot of region.rack.slots) {
            expect(slot.hitRect.w).toBeGreaterThan(0);
            expect(slot.hitRect.h).toBeGreaterThan(0);
          }
          // The union is the `zone:<seat>:rack` / `pile:<seat>` anchor and covers
          // every slot, so the fallback chain always lands somewhere real.
          for (const slot of region.rack.slots) {
            expect(slot.hitRect.x).toBeGreaterThanOrEqual(region.piles.x - 1e-6);
            expect(slot.hitRect.y).toBeGreaterThanOrEqual(region.piles.y - 1e-6);
            expect(slot.hitRect.x + slot.hitRect.w).toBeLessThanOrEqual(
              region.piles.x + region.piles.w + 1e-6,
            );
            expect(slot.hitRect.y + slot.hitRect.h).toBeLessThanOrEqual(
              region.piles.y + region.piles.h + 1e-6,
            );
          }
        }
      }
    }
  });

  it('costs a bounded number of scene nodes: one element per drawn zone anchor', () => {
    // zone-geography §13 counts the rack's anatomy against the ≤ 15 000-node
    // scene budget. This pins the *staged* cost — the geometry the reconciler
    // turns into elements — at six seats, the worst staging the layout permits.
    const plane = stage(commanderTable({ opponents: 5, active: 'p2' }), BASELINE);
    const staged = regionsOf(plane);
    expect(staged).toHaveLength(6);
    const drawn = staged.reduce(
      (sum, region) => sum + (region.rack.variant === 'digest' ? 1 : region.rack.slots.length),
      0,
    );
    // 2 full racks (receiver + focused) × 4 slots + 4 digest buttons = 12.
    expect(drawn).toBe(12);
    expect(drawn).toBeLessThan(15_000);
  });
});
