/**
 * The seat identity cluster (issue #532) against its binding specification,
 * `docs/design/seat-identity.md`.
 *
 * What this file proves: the cluster's anatomy at every rung (§1, §2), the
 * information matrix's *available* rows (§4), the value rules for life,
 * commander tax, commander damage, and named statuses (§5), every state channel
 * and the disjointness that lets them co-occur (§6), the stress states (§7),
 * placement and orientation (§8), the accessible sentence (§9), the data-source
 * column (§10), and — the load-bearing negative — that every slot §11 marks as
 * blocked renders **nothing**.
 *
 * What it cannot prove, and does not claim: jsdom applies no CSS module and
 * performs no layout, so nothing here shows a rendered pixel, a contrast
 * reading, the priority bloom against an environment plate, or whether the
 * cluster reads as the baseline at a glance. Those are the maintainer's, in a
 * real browser, per issue #532's closure gate.
 */
import { describe, expect, it } from 'vitest';
import { PLANE, stagePlane, stageSeatCluster, type SeatCluster, type StagedPlane } from './plane';
import { seatColorIdentity, worstCommanderDamage } from './seatIdentity';
// A namespace import, so the §1.3 removal can be asserted as an absence: the
// monogram helpers must not come back as exports of this module.
import * as seatPortraits from './seatPortraits';
import { clusterTable, type ClusterTableSpec } from './seat-cluster.fixture';
import { DESKTOP, TABLET, ULTRAWIDE, WIDE16, clusterRects } from './plane.fixture';

const { LOCAL_PORTRAIT, OPPONENT_PORTRAITS } = seatPortraits;

const VIEWPORTS = [
  ['desktop 1280×800', DESKTOP],
  ['baseline 1680×945', { width: 1680, height: 945 }],
  ['16:9 1920×1080', WIDE16],
  ['ultrawide 2560×1080', ULTRAWIDE],
  ['tablet floor 1180×820', TABLET],
] as const;

/** `n` seats, `p1` the receiver, with per-seat overrides applied by index. */
function seats(n: number, overrides: Partial<ClusterTableSpec['seats'][number]>[] = []) {
  return Array.from({ length: n }, (_, i) => ({
    id: `p${i + 1}`,
    name: `Player ${i + 1}`,
    ...(overrides[i] ?? {}),
  }));
}

function planeOf(spec: ClusterTableSpec, viewport = DESKTOP): StagedPlane {
  return stagePlane(clusterTable(spec), viewport);
}

function clusterOf(plane: StagedPlane, seat: string): SeatCluster {
  const region = [plane.receiver, plane.farSide, ...plane.wings].find((r) => r?.seat === seat);
  if (!region) throw new Error(`no region staged for ${seat}`);
  return region.cluster;
}

/** Every cluster on a plane, keyed by seat. */
function clustersOf(plane: StagedPlane): Map<string, SeatCluster> {
  const map = new Map<string, SeatCluster>();
  for (const region of [plane.receiver, plane.farSide, ...plane.wings]) {
    if (region) map.set(region.seat, region.cluster);
  }
  return map;
}

describe('seat cluster §1/§2 — one component family, five rungs', () => {
  it('draws local, focused, wing, and compact variants from one anatomy', () => {
    // §2's variant table: four attested rungs, each a scale of the SAME set of
    // elements. Every one of them carries a portrait, a life medallion, a
    // nameplate, and the hand pip — that is what makes them one family.
    const plane = planeOf({ seats: seats(4), active: 'p2' }, { width: 1680, height: 945 });
    const variants = [...clustersOf(plane).values()].map((c) => c.variant);
    expect(new Set(variants)).toEqual(new Set(['local', 'focused', 'wing']));
    for (const cluster of clustersOf(plane).values()) {
      expect(cluster.portrait.w).toBe(cluster.d);
      expect(cluster.portrait.h).toBe(cluster.d);
      expect(cluster.life.w).toBeCloseTo(cluster.d * PLANE.cluster.life.d, 6);
      expect(cluster.plate).toBeDefined();
      expect(cluster.pip).toBeDefined();
    }
  });

  it('steps D down the §1.1 ladder — local > focused > wing, never below 44 px', () => {
    const plane = planeOf({ seats: seats(4), active: 'p2' }, { width: 1680, height: 945 });
    const local = clusterOf(plane, 'p1');
    const focused = clusterOf(plane, 'p2');
    const wing = clusterOf(plane, 'p3');
    expect(local.d).toBeGreaterThan(focused.d);
    expect(focused.d).toBeGreaterThan(wing.d);
    for (const cluster of [local, focused, wing]) {
      expect(cluster.d).toBeGreaterThanOrEqual(PLANE.cluster.d.minimal);
      expect(cluster.hit.w).toBeGreaterThanOrEqual(PLANE.minHit);
      expect(cluster.hit.h).toBeGreaterThanOrEqual(PLANE.minHit);
    }
  });

  it('caps D on a phone rather than staging a medallion wider than the plane', () => {
    const plane = planeOf({ seats: seats(2) }, { width: 390, height: 844 });
    const local = clusterOf(plane, 'p1');
    expect(local.d).toBeLessThan(PLANE.cluster.d.local);
    expect(local.d).toBeGreaterThanOrEqual(PLANE.cluster.d.minimal);
  });

  it('digests a wing to the compact rung, where the pip becomes the under-slung tab', () => {
    // §2: the peripheral/compact variant swaps the free-standing hexagon for the
    // under-slung shield — the same datum, a second attachment mode.
    const plane = planeOf({ seats: seats(4), active: 'p2' }, DESKTOP);
    const wing = clusterOf(plane, 'p3');
    expect(wing.variant).toBe('compact');
    expect(wing.pip?.shape).toBe('tab');
    const focused = clusterOf(plane, 'p2');
    expect(focused.pip?.shape).toBe('hex');
  });

  it('gives every cluster the crest as its ≥ 44 px activation rect', () => {
    // §9: the cluster IS the pick surface for player targeting and focus.
    for (const [, viewport] of VIEWPORTS) {
      for (const count of [2, 3, 4, 5, 6]) {
        const plane = planeOf({ seats: seats(count), active: 'p2' }, viewport);
        for (const region of [plane.receiver, plane.farSide, ...plane.wings]) {
          if (!region) continue;
          expect(region.crest).toEqual(region.cluster.hit);
          expect(region.crest.w).toBeGreaterThanOrEqual(PLANE.minHit);
        }
      }
    }
  });

  it('stages every drawn element of every cluster on the plane at 2–6 seats', () => {
    for (const [label, viewport] of VIEWPORTS) {
      for (const count of [2, 3, 4, 5, 6]) {
        const plane = planeOf({ seats: seats(count), active: 'p2' }, viewport);
        for (const cluster of clustersOf(plane).values()) {
          for (const rect of clusterRects(cluster)) {
            expect({ label, count, seat: cluster.seat, ok: rect.x >= 0 && rect.y >= 0 }).toEqual({
              label,
              count,
              seat: cluster.seat,
              ok: true,
            });
            expect(rect.x + rect.w).toBeLessThanOrEqual(viewport.width);
            expect(rect.y + rect.h).toBeLessThanOrEqual(viewport.height);
          }
        }
      }
    }
  });
});

describe('seat cluster §8 — placement and orientation', () => {
  it('anchors the local cluster bottom centre and the focused one top centre', () => {
    // §8's anchor table, transcribed from `rune-battlefield-environments.jpg`.
    const plane = planeOf({ seats: seats(4), active: 'p2' }, { width: 1680, height: 945 });
    const local = clusterOf(plane, 'p1');
    const focused = clusterOf(plane, 'p2');
    const centre = (c: SeatCluster) => c.portrait.x + c.portrait.w / 2;
    expect(centre(local)).toBeCloseTo(840, 0);
    expect(centre(focused)).toBeCloseTo(840, 0);
    expect(local.portrait.y).toBeGreaterThan(focused.portrait.y);
  });

  it('mirrors wing clusters per side and never rotates one', () => {
    // §8: clusters MUST NOT rotate — text stays horizontal at every seat. The
    // model carries no rotation at all, which is the strongest form of that.
    const plane = planeOf({ seats: seats(6), active: 'p2' }, { width: 1920, height: 1080 });
    const left = [...clustersOf(plane).values()].filter((c) => c.portrait.x < 400);
    const right = [...clustersOf(plane).values()].filter((c) => c.portrait.x > 1400);
    expect(left.length).toBeGreaterThan(0);
    expect(right.length).toBeGreaterThan(0);
    for (const cluster of [...left, ...right]) {
      expect(cluster.plate).toBeDefined();
      expect(Object.keys(cluster)).not.toContain('rotation');
    }
  });

  it('never runs a nameplate through the seat’s own zone rack', () => {
    // The documented departure in `cluster.ts`: §8's outboard direction holds
    // wherever it fits, and the plate steps around the rack rather than through
    // it wherever #531's rack-shared anchor leaves no outboard run.
    for (const [label, viewport] of VIEWPORTS) {
      for (const count of [2, 4, 6]) {
        const plane = planeOf({ seats: seats(count), active: 'p2' }, viewport);
        for (const region of [plane.receiver, plane.farSide, ...plane.wings]) {
          if (!region?.cluster.plate) continue;
          const a = region.cluster.plate.rect;
          const b = region.piles;
          const hit = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
          expect({ label, count, seat: region.seat, hit }).toEqual({
            label,
            count,
            seat: region.seat,
            hit: false,
          });
        }
      }
    }
  });

  it('puts the hand pip opposite the nameplate so the two never collide', () => {
    const plane = planeOf({ seats: seats(4), active: 'p2' }, { width: 1680, height: 945 });
    for (const cluster of clustersOf(plane).values()) {
      const plate = cluster.plate;
      const pip = cluster.pip;
      if (!plate || !pip || plate.direction === 'below' || pip.shape === 'tab') continue;
      const centre = cluster.portrait.x + cluster.portrait.w / 2;
      const pipRight = pip.rect.x + pip.rect.w / 2 > centre;
      expect(pipRight).toBe(plate.direction === 'left');
    }
  });
});

describe('seat cluster §5.1 — life, verbatim, with urgency as a shape', () => {
  it('shows the server’s number and steps only the numeral’s size', () => {
    for (const [life, glyphs] of [
      [20, 2],
      [7, 1],
      [999, 3],
      [-99, 3],
      [-100, 4],
    ] as const) {
      const plane = planeOf({ seats: seats(2, [{ life }]) });
      const cluster = clusterOf(plane, 'p1');
      expect(cluster.lifeGlyph).toBe(String(life));
      expect(cluster.lifeGlyphs).toBe(glyphs);
      // The medallion never resizes; only the numeral does.
      expect(cluster.life.w).toBeCloseTo(cluster.d * PLANE.cluster.life.d, 6);
    }
  });

  it('truncates only beyond five glyphs, keeping the exact value accessible', () => {
    const plane = planeOf({ seats: seats(2, [{ life: 1234567 }]) });
    const cluster = clusterOf(plane, 'p1');
    expect(cluster.lifeGlyph).toBe('999+');
    expect(cluster.ariaLabel).toContain('1234567 life');
  });

  it('escalates urgency by shape at ≤ 5 and again at ≤ 0, never declaring a loss', () => {
    const at = (life: number) => clusterOf(planeOf({ seats: seats(2, [{ life }]) }), 'p1');
    expect(at(6).urgency).toBe('none');
    expect(at(5).urgency).toBe('low');
    expect(at(1).urgency).toBe('low');
    expect(at(0).urgency).toBe('zero');
    expect(at(-4).urgency).toBe('zero');
    // A seat at 0 is still in the game until the server says otherwise.
    expect(at(0).channels.eliminated).toBe(false);
    expect(at(0).lifeGlyph).toBe('0');
  });
});

describe('seat cluster §4 — the crest carries the hand count and only the hand count', () => {
  it('shows the hand count and never a library pip (zone-geography §4.1)', () => {
    const plane = planeOf({ seats: seats(2, [{ hand: 20, library: 137 }]) });
    const cluster = clusterOf(plane, 'p1');
    expect(cluster.pip?.count).toBe(20);
    // The library's count is the library pile's, and appears in the cluster only
    // through the accessible sentence.
    expect(clusterRects(cluster)).toHaveLength(
      3 + (cluster.plate ? 1 : 0) + (cluster.gem ? 1 : 0) + cluster.chips.length,
    );
    expect(cluster.ariaLabel).toContain('137 in library');
  });

  it('reads an opponent’s public hand size and its own hand’s real length', () => {
    const plane = planeOf({ seats: seats(2, [{ hand: 4 }, { hand: 9 }]) });
    expect(clusterOf(plane, 'p1').pip?.count).toBe(4);
    expect(clusterOf(plane, 'p2').pip?.count).toBe(9);
  });
});

describe('seat cluster §6 — every state gets a distinct shape or placement', () => {
  it('carries priority, active turn, and the attack as three separate channels', () => {
    const plane = planeOf({
      seats: seats(4),
      active: 'p2',
      priority: 'p2',
      attacks: [['p3', 'p2']],
    });
    const focused = clusterOf(plane, 'p2');
    expect(focused.channels).toMatchObject({ priority: true, active: true, attacked: true });
    // §7 "every state at once": the three occupy disjoint placements — a ring
    // OUTSIDE the rim, a pennant at 12 o'clock, and a dashed ring INSIDE it —
    // so none of them has to arbitrate with another.
    const other = clusterOf(plane, 'p4');
    expect(other.channels).toMatchObject({ priority: false, active: false, attacked: false });
  });

  it('counts the attackers on a seat as a rail chip, without computing combat', () => {
    const plane = planeOf({
      seats: seats(4),
      active: 'p2',
      attacks: [
        ['p2', 'p3'],
        ['p2', 'p3'],
        ['p2', 'p4'],
      ],
    });
    const chip = clusterOf(plane, 'p3').chips.find((c) => c.kind === 'attacked');
    expect(chip?.value).toBe('×2');
    expect(chip?.label).toBe('Attacked by 2');
    expect(clusterOf(plane, 'p4').chips.find((c) => c.kind === 'attacked')?.value).toBe('×1');
  });

  it('replaces an eliminated seat’s life numeral with a struck rune and strips its rail', () => {
    // §6.5: the life NUMBER is removed rather than shown as `0`, because a live
    // seat may legitimately sit at `0` for an instant before state-based actions.
    const plane = planeOf({
      seats: seats(4, [{}, { eliminated: true, statuses: ['monarch'] }]),
      active: 'p1',
      attacks: [['p3', 'p2']],
    });
    const dead = clusterOf(plane, 'p2');
    expect(dead.channels.eliminated).toBe(true);
    expect(dead.lifeGlyph).toBe('⊘');
    expect(dead.urgency).toBe('none');
    expect(dead.pip).toBeUndefined();
    expect(dead.chips).toEqual([]);
    // The slot stays and the seat stays reachable — its piles stay browsable.
    expect(dead.hit.w).toBeGreaterThanOrEqual(PLANE.minHit);
    expect(dead.plate?.text).toContain('Player 2');
  });

  it('keeps the deadline and the auto-passed chip on the receiver alone', () => {
    // §10.3: `action_deadline` and `auto_passed` are receiver-only by
    // construction — no opponent view carries either.
    const plane = planeOf({ seats: seats(4), active: 'p1', autoPassed: true, deadline: 12_000 });
    expect(clusterOf(plane, 'p1').channels.deadline).toBe(true);
    expect(clusterOf(plane, 'p1').chips.some((c) => c.kind === 'autoPassed')).toBe(true);
    for (const seat of ['p2', 'p3', 'p4']) {
      expect(clusterOf(plane, seat).channels.deadline).toBe(false);
      expect(clusterOf(plane, seat).chips.some((c) => c.kind === 'autoPassed')).toBe(false);
    }
  });
});

describe('seat cluster §5.3/§5.4 — commander presence, tax, and damage', () => {
  it('marks commander presence from the command pile or a tax entry', () => {
    const withPile = planeOf({ seats: seats(2, [{ command: ['{2}{G}'] }]) });
    expect(clusterOf(withPile, 'p1').channels.commander).toBe(true);
    const withTax = planeOf({ seats: seats(2), commanderTax: [['p2', 4]] });
    expect(clusterOf(withTax, 'p2').channels.commander).toBe(true);
    const neither = planeOf({ seats: seats(2) });
    expect(clusterOf(neither, 'p1').channels.commander).toBe(false);
  });

  it('shows exactly one worst shield for five commander-damage sources', () => {
    // §7's stress row and §5.4: the collapsed cluster carries the single highest
    // nonzero incoming value; the full matrix is the opened surface's job.
    const plane = planeOf({
      seats: seats(6),
      active: 'p2',
      commanderDamage: [
        ['p2', 'p1', 3],
        ['p3', 'p1', 17],
        ['p4', 'p1', 0],
        ['p5', 'p1', 9],
        ['p6', 'p1', 12],
      ],
    });
    const shields = clusterOf(plane, 'p1').chips.filter((c) => c.kind === 'commanderDamage');
    expect(shields).toHaveLength(1);
    expect(shields[0]!.value).toBe('17');
    expect(shields[0]!.escalation).toBe('doubled');
  });

  it('never draws a zero tally on the collapsed cluster', () => {
    const plane = planeOf({ seats: seats(2), commanderDamage: [['p2', 'p1', 0]] });
    expect(clusterOf(plane, 'p1').chips.some((c) => c.kind === 'commanderDamage')).toBe(false);
  });

  it('escalates the shield by shape at 15+, 18–20, and 21+', () => {
    const escalationAt = (amount: number) =>
      clusterOf(planeOf({ seats: seats(2), commanderDamage: [['p2', 'p1', amount]] }), 'p1')
        .chips[0]?.escalation;
    expect(escalationAt(14)).toBe('plain');
    expect(escalationAt(15)).toBe('doubled');
    expect(escalationAt(18)).toBe('notched');
    expect(escalationAt(21)).toBe('terminal');
    // …and the client still does NOT eliminate the seat itself at 21.
    const plane = planeOf({ seats: seats(2), commanderDamage: [['p2', 'p1', 21]] });
    expect(clusterOf(plane, 'p1').channels.eliminated).toBe(false);
  });

  it('duplicates the commander tax only where the command pile is not drawn', () => {
    // §5.3: tax lives on the command-zone pile, where the recast originates. The
    // cluster carries it only when that pile has collapsed out of the rung.
    const plane = planeOf(
      { seats: seats(4), active: 'p2', commanderTax: [['p2', 6]] },
      { width: 1680, height: 945 },
    );
    const focused = clusterOf(plane, 'p2');
    const drawsCommandPile = plane.farSide!.rack.slots.some((slot) => slot.zone === 'command');
    expect(drawsCommandPile).toBe(true);
    expect(focused.chips.some((c) => c.kind === 'commanderTax')).toBe(false);

    const digested = planeOf(
      { seats: seats(6), active: 'p2', commanderTax: [['p3', 6]] },
      { width: 1280, height: 800 },
    );
    const wing = clusterOf(digested, 'p3');
    expect(digested.wings.find((w) => w.seat === 'p3')!.rack.variant).toBe('digest');
    expect(wing.chips.find((c) => c.kind === 'commanderTax')?.value).toBe('+6');
  });

  it('draws the identity gem only while the commander is in the command zone', () => {
    // §11 / #553: there is no per-player colour-identity field, so the gem is
    // honestly absent rather than held over or guessed off the battlefield.
    const inZone = planeOf({ seats: seats(2, [{ command: ['{1}{U}{U}'] }]) });
    expect(clusterOf(inZone, 'p1').gem?.identity).toBe('U');
    const cast = planeOf({ seats: seats(2) });
    expect(clusterOf(cast, 'p1').gem).toBeUndefined();
  });

  it('reads a multicolour command zone as one multicolour gem', () => {
    expect(
      seatColorIdentity(clusterTable({ seats: seats(2, [{ command: ['{W}{U}'] }]) }), 'p1'),
    ).toBe('M');
    expect(worstCommanderDamage([], 'p1')).toBeUndefined();
  });
});

describe('seat cluster §5.6/§7 — the status rail and its cap', () => {
  it('keeps the server’s array order and never ranks a status', () => {
    const plane = planeOf({
      seats: seats(2, [{}, { statuses: ['zeta', 'alpha', 'monarch'] }]),
    });
    const chips = clusterOf(plane, 'p2').chips;
    expect(chips.map((c) => c.label)).toEqual(['zeta', 'alpha', '1 more: monarch']);
  });

  it('caps the rail at two plus an overflow at the local and focused rungs', () => {
    const plane = planeOf(
      { seats: seats(4, [{}, { statuses: ['a', 'b', 'c', 'd', 'e', 'f'] }]), active: 'p2' },
      { width: 1680, height: 945 },
    );
    const chips = clusterOf(plane, 'p2').chips;
    expect(chips).toHaveLength(3);
    expect(chips[2]!.kind).toBe('overflow');
    expect(chips[2]!.value).toBe('4');
    // The `+N` label still names every hidden status, so nothing is lost to a
    // screen reader by the visual cap.
    expect(chips[2]!.label).toContain('c, d, e, f');
  });

  it('caps the rail at one plus an overflow at a wing rung', () => {
    const plane = planeOf(
      { seats: seats(4, [{}, {}, { statuses: ['a', 'b', 'c'] }]), active: 'p2' },
      { width: 1680, height: 945 },
    );
    const chips = clusterOf(plane, 'p3').chips;
    expect(chips).toHaveLength(2);
    expect(chips[1]!.value).toBe('2');
  });

  it('lays the rail on the §1.2 arc, outside the priority ring, never overlapping', () => {
    const plane = planeOf(
      { seats: seats(4, [{}, { statuses: ['a', 'b'] }]), active: 'p2', attacks: [['p3', 'p2']] },
      { width: 1680, height: 945 },
    );
    const cluster = clusterOf(plane, 'p2');
    const cx = cluster.portrait.x + cluster.portrait.w / 2;
    const cy = cluster.portrait.y + cluster.portrait.h / 2;
    expect(cluster.chips.length).toBeGreaterThan(1);
    for (const chip of cluster.chips) {
      const dx = chip.rect.x + chip.rect.w / 2 - cx;
      const dy = chip.rect.y + chip.rect.h / 2 - cy;
      expect(Math.hypot(dx, dy) / cluster.d).toBeGreaterThan(PLANE.cluster.priorityOuter);
    }
    for (let i = 1; i < cluster.chips.length; i += 1) {
      const a = cluster.chips[i - 1]!.rect;
      const b = cluster.chips[i]!.rect;
      expect(a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h).toBe(false);
    }
  });

  it('puts the attacked chip in the rail’s first slot (§6.3)', () => {
    const plane = planeOf({
      seats: seats(2, [{}, { statuses: ['monarch'] }]),
      attacks: [['p1', 'p2']],
    });
    expect(clusterOf(plane, 'p2').chips[0]!.kind).toBe('attacked');
  });
});

describe('seat cluster §7 — stress states', () => {
  it('middle-ellipsises a long name and keeps the whole one accessible', () => {
    const long = 'Verynamedplayerwithanabsurdlylongname';
    const plane = planeOf({ seats: seats(2, [{ name: long }]) }, { width: 1680, height: 945 });
    const plate = clusterOf(plane, 'p1').plate!;
    expect(plate.truncated).toBe(true);
    expect(plate.text).toContain('…');
    expect(plate.text.length).toBeLessThan(long.length);
    expect(plate.rect.w).toBeLessThanOrEqual(
      clusterOf(plane, 'p1').d * PLANE.cluster.plate.maxLen + 0.001,
    );
    expect(clusterOf(plane, 'p1').ariaLabel).toContain(long);
  });

  it('keeps the head and tail of a name that just fits the full-length plate', () => {
    const plane = planeOf(
      { seats: seats(2, [{ name: 'Alexanderthegreatest' }]) },
      { width: 1920, height: 1080 },
    );
    const plate = clusterOf(plane, 'p1').plate!;
    const { headGraphemes, tailGraphemes } = PLANE.cluster.plate;
    expect(plate.text.startsWith('Alexanderthegreatest'.slice(0, headGraphemes))).toBe(true);
    expect(plate.text.endsWith('Alexanderthegreatest'.slice(-tailGraphemes))).toBe(true);
  });

  it('falls back to `Seat N` when the server sent no name for a seat', () => {
    // §7's "no name" row: never blank, never a raw `p{N}` id.
    const plane = planeOf({ seats: [{ id: 'p1' }, { id: 'p2' }] });
    expect(clusterOf(plane, 'p2').ariaLabel.startsWith('Seat 2')).toBe(true);
  });

  it('never lets a nameplate push the portrait', () => {
    const short = planeOf({ seats: seats(2, [{ name: 'Al' }]) }, { width: 1680, height: 945 });
    const long = planeOf(
      { seats: seats(2, [{ name: 'Verynamedplayerwithanabsurdlylongname' }]) },
      { width: 1680, height: 945 },
    );
    expect(clusterOf(long, 'p1').portrait).toEqual(clusterOf(short, 'p1').portrait);
  });

  it('holds a 20-card hand and a 3-digit library without changing shape', () => {
    const small = planeOf({ seats: seats(2, [{ hand: 3, library: 40 }]) });
    const big = planeOf({ seats: seats(2, [{ hand: 20, library: 137 }]) });
    expect(clusterOf(big, 'p1').pip!.rect).toEqual(clusterOf(small, 'p1').pip!.rect);
    expect(clusterOf(big, 'p1').pip!.count).toBe(20);
  });

  it('composes priority, active turn, attack, commander, and a status at once', () => {
    // §7's "every state at once" row: five disjoint placements plus the rail.
    const plane = planeOf({
      seats: seats(4, [{}, { statuses: ['monarch'], command: ['{W}{U}'] }]),
      active: 'p2',
      priority: 'p2',
      attacks: [['p3', 'p2']],
      commanderDamage: [['p3', 'p2', 19]],
    });
    const cluster = clusterOf(plane, 'p2');
    expect(cluster.channels).toMatchObject({
      priority: true,
      active: true,
      attacked: true,
      commander: true,
    });
    expect(cluster.gem?.identity).toBe('M');
    expect(cluster.chips.map((c) => c.kind)).toEqual(['attacked', 'commanderDamage', 'overflow']);
  });
});

describe('seat cluster §1.3/§10.1 — portrait plates', () => {
  it('gives the receiver the local plate and opponents the opponent cycle', () => {
    const plane = planeOf({ seats: seats(4), active: 'p2' });
    expect(clusterOf(plane, 'p1').portraitSrc).toBe(LOCAL_PORTRAIT?.src);
    for (const seat of ['p2', 'p3', 'p4']) {
      const src = clusterOf(plane, seat).portraitSrc;
      expect(OPPONENT_PORTRAITS.map((p) => p.src)).toContain(src);
      expect(src).not.toBe(LOCAL_PORTRAIT?.src);
    }
  });

  it('gives distinct plates to distinct seats at six seats', () => {
    const plane = planeOf({ seats: seats(6), active: 'p2' });
    const srcs = [...clustersOf(plane).values()].map((c) => c.portraitSrc);
    expect(new Set(srcs).size).toBe(srcs.length);
  });

  it('holds a seat’s face for the whole game across eliminations and reorders', () => {
    // §1.3: assignment is keyed by `seat_order`, which is server-authoritative
    // and never reorders — NOT by `opponents[]`, which an elimination shortens.
    const before = planeOf({ seats: seats(5), active: 'p2' });
    const faces = new Map([...clustersOf(before)].map(([seat, c]) => [seat, c.portraitSrc]));

    // p2 is eliminated and the server drops it from `opponents[]`; `seat_order`
    // still carries it, and priority and focus have moved.
    const after = stagePlane(
      clusterTable({
        seats: [
          { id: 'p1', name: 'Player 1' },
          { id: 'p3', name: 'Player 3' },
          { id: 'p4', name: 'Player 4' },
          { id: 'p5', name: 'Player 5' },
        ],
        seatOrder: ['p1', 'p2', 'p3', 'p4', 'p5'],
        active: 'p4',
        priority: 'p4',
      }),
      DESKTOP,
    );
    for (const [seat, cluster] of clustersOf(after)) {
      expect({ seat, src: cluster.portraitSrc }).toEqual({ seat, src: faces.get(seat) });
    }
  });

  it('keeps a seat’s face stable when the same view is staged at another size', () => {
    const spec: ClusterTableSpec = { seats: seats(4), active: 'p2' };
    const a = planeOf(spec, DESKTOP);
    const b = planeOf(spec, ULTRAWIDE);
    for (const [seat, cluster] of clustersOf(a)) {
      expect(clustersOf(b).get(seat)!.portraitSrc).toBe(cluster.portraitSrc);
    }
  });

  it('assigns from `seat_order` even when the server orders `opponents[]` differently', () => {
    const straight = planeOf({ seats: seats(4), active: 'p2' });
    const shuffled = stagePlane(
      clusterTable({
        seats: [
          { id: 'p1', name: 'Player 1' },
          { id: 'p4', name: 'Player 4' },
          { id: 'p3', name: 'Player 3' },
          { id: 'p2', name: 'Player 2' },
        ],
        you: 'p1',
        seatOrder: ['p1', 'p2', 'p3', 'p4'],
        active: 'p2',
      }),
      DESKTOP,
    );
    for (const [seat, cluster] of clustersOf(straight)) {
      expect({ seat, src: clustersOf(shuffled).get(seat)!.portraitSrc }).toEqual({
        seat,
        src: cluster.portraitSrc,
      });
    }
  });

  it('draws no substitute glyph beside the plate, and publishes none', () => {
    // §1.3, as rewritten when the portraits shipped: "the aperture keeps its
    // token background and accessible player name but draws no substitute
    // glyph". The procedural rune monogram is gone from the module, from the
    // facts, and from the staged cluster — a portrait-less aperture publishes
    // nothing for a stylesheet to paint a mark from.
    expect(Object.keys(seatPortraits)).not.toContain('monogramFor');
    expect(Object.keys(seatPortraits)).not.toContain('PORTRAIT_MONOGRAMS');
    const plane = planeOf({ seats: seats(4), active: 'p2' });
    for (const [seat, cluster] of clustersOf(plane)) {
      expect({ seat, keys: Object.keys(cluster) }).toEqual({
        seat,
        keys: expect.not.arrayContaining(['monogram']),
      });
    }
  });

  it('keeps the seat legible with no plate at all: token aperture plus the name', () => {
    // The state §1.3 describes — a plate still loading, a plate that failed, or
    // a build that ships none. The aperture's token background is a stylesheet
    // layer painting UNDER `--portrait-src`, so it needs no field; what the
    // staged cluster must still carry is the absent-plate state and the name.
    const cluster = stageSeatCluster({
      seat: 'p2',
      variant: 'focused',
      anchor: { x: 300, y: 200 },
      viewport: { width: 1280, height: 800 },
      outboard: 'left',
      facts: {
        label: 'Veyra',
        local: false,
        life: 28,
        handCount: 6,
        libraryCount: 51,
        commanderPresent: false,
        statuses: [],
        attackedCount: 0,
        autoPassed: false,
        deadline: false,
        // No `portrait` at all — the fallback path, with nothing to fall back to.
        accent: '#4D7EC9',
        eliminated: false,
        priority: false,
        active: false,
        focused: false,
        attacked: false,
      },
    });
    expect(cluster.portraitSrc).toBeUndefined();
    // The medallion still exists at full size, so the aperture is a token
    // surface and not a hole, and it still reads as one sentence.
    expect(cluster.portrait.w).toBe(cluster.d);
    expect(cluster.ariaLabel).toContain('Veyra');
    expect(cluster.ariaLabel).toContain('28 life');
  });
});

describe('seat cluster §9/§11 — accessibility and the dormant slots', () => {
  it('reads the whole seat as one sentence', () => {
    const plane = planeOf({
      seats: seats(4, [{}, { name: 'Veyra', life: 28, hand: 6, library: 51 }]),
      active: 'p2',
      priority: 'p2',
    });
    const label = clusterOf(plane, 'p2').ariaLabel;
    expect(label.startsWith('Veyra, 28 life, 6 in hand, 51 in library')).toBe(true);
    expect(label).toContain('active turn');
    expect(label).toContain('has priority');
    expect(label).toContain('focused');
  });

  it('renders nothing for poison, counters, disconnection, or an AI marker', () => {
    // §11's standing prohibition: a status is free-form display text, so no
    // number is parsed out of it, no threshold is inferred, and a dormant slot
    // shows no zero and no "unknown".
    const plane = planeOf({
      seats: seats(2, [{}, { statuses: ['poison:3', 'poison 7', 'disconnected'] }]),
    });
    const cluster = clusterOf(plane, 'p2');
    expect(cluster.channels.disconnected).toBe(false);
    for (const chip of cluster.chips.filter((c) => c.kind === 'status')) {
      // A status medallion carries a glyph and the raw string — never a number
      // lifted out of it, and never a threshold inferred from it.
      expect(chip.value).toBeUndefined();
    }
    expect(cluster.chips.map((c) => c.label)).toEqual([
      'poison:3',
      'poison 7',
      '1 more: disconnected',
    ]);
  });

  it('shows the receiver no named statuses at all, because SelfView carries none', () => {
    const plane = planeOf({ seats: seats(2), attacks: [] });
    expect(clusterOf(plane, 'p1').chips).toEqual([]);
  });

  it('never eliminates the local seat, because SelfView has no `eliminated`', () => {
    const plane = planeOf({ seats: seats(2, [{ life: -5 }]) });
    expect(clusterOf(plane, 'p1').channels.eliminated).toBe(false);
    expect(clusterOf(plane, 'p1').lifeGlyph).toBe('-5');
  });

  it('renders no opponent mana anywhere in the cluster (§5.2)', () => {
    const plane = planeOf({ seats: seats(4), active: 'p2' });
    for (const cluster of clustersOf(plane).values()) {
      expect(JSON.stringify(cluster)).not.toContain('mana');
    }
  });
});
