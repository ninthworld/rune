/**
 * The directional relationship grammar (issue #535, against
 * `docs/design/stack-and-relationships.md` §4 and §5).
 *
 * Structural draw-program assertions in the ADR 0011 idiom: these prove the
 * *program* — which constituents exist, in what geometry, with what widths and
 * what alphas — and deliberately not the pixels. What a stroke looks like once
 * Pixi has drawn it is the maintainer's browser check, listed in the issue.
 *
 * The grammar's whole claim is that a viewer can read **who acts on whom**
 * without motion and without colour, so nearly every case below asserts a
 * shape channel: the taper's direction, the asymmetry of the two caps, the
 * absence of an arrowhead on a block, the squareness of an attachment terminal.
 */
import { describe, expect, it } from 'vitest';
import { COMBAT_LINK } from '../tokens';
import { SCENE_HUES, SCENE_NEUTRALS, SCENE_RELATIONSHIP } from '../sceneTokens';
import {
  fanGroups,
  relationshipAnimates,
  relationshipOps,
  relationshipState,
  type DrawOp,
  type DrawPart,
  type PersistentEffect,
  type RelationshipContext,
} from './effects';
import type { Rect } from './scene';

const SOURCE: Rect = { x: 100, y: 400, w: 66, h: 92 };
const CARD: Rect = { x: 500, y: 140, w: 66, h: 92 };
const CREST: Rect = { x: 520, y: 40, w: 52, h: 52 };

function path(over: Partial<PersistentEffect> = {}): PersistentEffect {
  return {
    id: 'r1',
    category: 'targeting-path',
    from: { ref: 'src' },
    to: { ref: 'dst' },
    accent: SCENE_HUES.orange.value,
    ...over,
  };
}

function ctx(over: Partial<RelationshipContext> = {}): RelationshipContext {
  return { phase: 0, reducedMotion: false, ...over };
}

const parts = (ops: DrawOp[], part: DrawPart): DrawOp[] => ops.filter((op) => op.part === part);
const widths = (ops: DrawOp[]): number[] =>
  parts(ops, 'path')
    .filter((op) => op.op === 'segment')
    .map((op) => (op.op === 'segment' ? op.width : 0));

describe('§4.1 — every relationship renders all four constituents', () => {
  it('gives a card target a source disc, a path, a taper, and a destination cap', () => {
    const ops = relationshipOps(path(), SOURCE, CARD, ctx());
    const source = parts(ops, 'source');
    expect(source).toHaveLength(1);
    expect(source[0]!.op === 'circle' && source[0]!.fill).toBe(true);
    expect(parts(ops, 'path').length).toBeGreaterThan(3);
    // D1 — endpoint asymmetry: the source is FILLED, the destination is OPEN.
    const ring = parts(ops, 'cap').find((op) => op.op === 'circle');
    expect(ring?.op === 'circle' && ring.fill).toBe(false);
    // The four constituents are never optional; a missing one is a bug (§4.1).
    expect(new Set(ops.map((op) => op.part))).toEqual(new Set(['source', 'path', 'cap']));
  });

  it('places the source disc on the source rect EDGE, not its center (§5.1)', () => {
    const ops = relationshipOps(path(), SOURCE, CARD, ctx());
    const disc = parts(ops, 'source')[0]!;
    expect(disc.op).toBe('circle');
    if (disc.op !== 'circle') return;
    // The destination is up and to the right, so the disc leaves the rect on
    // that side rather than sitting at 133, 446.
    expect(disc.x).toBeGreaterThan(SOURCE.x + SOURCE.w / 2);
    expect(disc.y).toBeLessThan(SOURCE.y + SOURCE.h / 2);
    expect(disc.r).toBe(SCENE_RELATIONSHIP.sourceRadius);
  });
});

describe('§4.2 D2 — the monotonic taper is the primary direction device', () => {
  it('widens monotonically from source to destination', () => {
    const drawn = widths(relationshipOps(path({ state: 'confirmed' }), SOURCE, CARD, ctx()));
    expect(drawn.length).toBeGreaterThan(8);
    for (let i = 1; i < drawn.length; i += 1) {
      expect(drawn[i]).toBeGreaterThanOrEqual(drawn[i - 1]!);
    }
    // The first piece's width is sampled at its own midpoint, so it opens a
    // hair above the source width rather than exactly on it.
    expect(drawn[0]).toBeGreaterThanOrEqual(SCENE_RELATIONSHIP.taperFrom);
    expect(drawn[0]).toBeLessThan(SCENE_RELATIONSHIP.taperFrom + 0.2);
    expect(drawn[drawn.length - 1]).toBeLessThanOrEqual(SCENE_RELATIONSHIP.taperTo);
    expect(drawn[drawn.length - 1]).toBeGreaterThan(drawn[0]! * 2);
  });

  it('is LOCALLY readable: any single visible dash states the direction', () => {
    // The device that survives occlusion and bundling. Take the middle third of
    // a dashed pending path — neither endpoint in view — and the widths still
    // increase toward the destination.
    const ops = relationshipOps(path({ state: 'pending' }), SOURCE, CARD, ctx());
    const drawn = widths(ops);
    const third = Math.floor(drawn.length / 3);
    const middle = drawn.slice(third, third * 2);
    expect(middle.length).toBeGreaterThan(1);
    expect(middle[middle.length - 1]).toBeGreaterThan(middle[0]!);
  });

  it('survives reduced motion unchanged — it is static by construction (§7.2)', () => {
    const full = widths(relationshipOps(path({ state: 'pending' }), SOURCE, CARD, ctx()));
    const reduced = widths(
      relationshipOps(path({ state: 'pending' }), SOURCE, CARD, ctx({ reducedMotion: true })),
    );
    expect(reduced[0]).toBeCloseTo(full[0]!, 5);
    expect(reduced[reduced.length - 1]).toBeGreaterThan(reduced[0]!);
  });
});

describe('§4.3 — the nine kinds, each with a geometry of its own', () => {
  it('R1 card target: an open reticle with the chevron INSIDE the ring (§5.2)', () => {
    const ops = relationshipOps(path({ endpoint: 'card' }), SOURCE, CARD, ctx());
    const cap = parts(ops, 'cap');
    const ring = cap.find((op) => op.op === 'circle');
    expect(ring?.op === 'circle' && ring.r).toBe(SCENE_RELATIONSHIP.reticleRadius);
    const wings = cap.filter((op) => op.op === 'segment');
    expect(wings).toHaveLength(2);
    // Both wings start at the ring's centre — the arrowhead lands inside it.
    for (const wing of wings) {
      if (wing.op !== 'segment') continue;
      expect(wing.from.x).toBeCloseTo(CARD.x + CARD.w / 2, 5);
      expect(wing.from.y).toBeCloseTo(CARD.y + CARD.h / 2, 5);
    }
  });

  it('R2 player target: a 90° crest ARC, never a reticle (§5.3, D8)', () => {
    const ops = relationshipOps(path({ endpoint: 'player' }), SOURCE, CREST, ctx());
    const cap = parts(ops, 'cap');
    // A crest is already a circle: a concentric ring would read as decoration.
    expect(cap.some((op) => op.op === 'circle')).toBe(false);
    const chords = cap.filter((op) => op.op === 'segment');
    expect(chords).toHaveLength(SCENE_RELATIONSHIP.crestChords + 2); // arc + arrowhead
    // Every chord rides the crest's own radius.
    const centre = { x: CREST.x + CREST.w / 2, y: CREST.y + CREST.h / 2 };
    for (const chord of chords.slice(0, SCENE_RELATIONSHIP.crestChords)) {
      if (chord.op !== 'segment') continue;
      expect(Math.hypot(chord.from.x - centre.x, chord.from.y - centre.y)).toBeCloseTo(
        CREST.w / 2,
        4,
      );
    }
  });

  it('R3 zone target: a square BRACKET, never a circle (§5.4, D9)', () => {
    const ops = relationshipOps(path({ endpoint: 'zone' }), SOURCE, CARD, ctx());
    const cap = parts(ops, 'cap');
    expect(cap.some((op) => op.op === 'circle')).toBe(false);
    // Three bracket strokes (two arms + a spine) plus the arrowhead's two.
    expect(cap).toHaveLength(5);
    const spine = cap[0]!;
    expect(spine.op).toBe('segment');
    if (spine.op !== 'segment') return;
    expect(Math.hypot(spine.to.x - spine.from.x, spine.to.y - spine.from.y)).toBeCloseTo(
      SCENE_RELATIONSHIP.bracketSpine,
      4,
    );
  });

  it('R4 stack target: an INSET reticle and a short hop that stays local (§5.5)', () => {
    const stack: Rect = { x: 560, y: 300, w: 48, h: 68 };
    const inset = relationshipOps(path({ endpoint: 'stack' }), SOURCE, stack, ctx());
    const ring = parts(inset, 'cap').find((op) => op.op === 'circle');
    expect(ring?.op === 'circle' && ring.r).toBe(SCENE_RELATIONSHIP.reticleInsetRadius);
    // A counterspell's arc is a short hop between two entries, not a trip across
    // the arena: the stack lift is far shallower than the corridor lift.
    const hop = parts(inset, 'path').filter((op) => op.op === 'segment');
    const card = parts(
      relationshipOps(path({ endpoint: 'card' }), SOURCE, stack, ctx()),
      'path',
    ).filter((op) => op.op === 'segment');
    const apex = (ops: DrawOp[]): number =>
      Math.min(...ops.map((op) => (op.op === 'segment' ? op.from.y : Infinity)));
    expect(apex(hop)).toBeGreaterThan(apex(card));
  });

  it('R8 block: a doubled parallel stroke with NO arrowhead — the absence is the semantic (D7)', () => {
    const ops = relationshipOps(
      path({ category: 'blocker-link', accent: COMBAT_LINK.color }),
      SOURCE,
      CARD,
      ctx(),
    );
    expect(parts(ops, 'path')).toHaveLength(2);
    expect(parts(ops, 'cap')).toHaveLength(0);
    // A node at the blocker end, and nothing at the attacker end: a block is a
    // bind, not a directed effect.
    const node = parts(ops, 'source');
    expect(node).toHaveLength(1);
    expect(node[0]!.op === 'circle' && node[0]!.fill).toBe(true);
    // Straight, with no lift: both strokes run between the two centres.
    const stroke = parts(ops, 'path')[0]!;
    expect(stroke.op === 'segment' && stroke.from.y).toBeGreaterThan(CARD.y);
  });

  it('R9 attachment: an elbow bracket with SYMMETRIC SQUARE terminals, never an arc (D6)', () => {
    const ops = relationshipOps(
      path({ category: 'attachment-bracket', accent: SCENE_NEUTRALS.text }),
      SOURCE,
      CARD,
      ctx(),
    );
    const strokes = parts(ops, 'path');
    expect(strokes).toHaveLength(4);
    // Axis-aligned, every one: this is what "right-angle connector" means, and
    // it is the hard separation from a lifted target arc.
    for (const stroke of strokes) {
      if (stroke.op !== 'segment') continue;
      const axis = stroke.from.x === stroke.to.x || stroke.from.y === stroke.to.y;
      expect(axis).toBe(true);
    }
    // Symmetric caps: both ends wear the SAME square. No source/destination
    // asymmetry, because attachment is not directed.
    const terminals = parts(ops, 'terminal');
    expect(terminals).toHaveLength(2);
    for (const terminal of terminals) expect(terminal.op).toBe('rect');
    expect(terminals.every((op) => op.op === 'rect' && op.w === op.h)).toBe(true);
    // Neutral line work, never a relationship hue.
    for (const op of ops) {
      expect(op.color).toBe(SCENE_NEUTRALS.text);
      expect(op.alpha).toBe(SCENE_RELATIONSHIP.lineworkAlpha);
    }
  });

  it('R9 source tether uses the identical shape as attachment', () => {
    const shape = (category: PersistentEffect['category']): unknown =>
      relationshipOps(path({ category, accent: SCENE_NEUTRALS.text }), SOURCE, CARD, ctx()).map(
        (op) => ({ ...op, category: '' }),
      );
    expect(shape('source-tether')).toEqual(shape('attachment-bracket'));
  });
});

describe('§4.3 R5 / D12 — multi-target is a trunk and a fan, not a starburst', () => {
  const resolved = [
    { effect: path({ id: 'a' }), source: { x: 100, y: 400 }, destination: { x: 500, y: 200 } },
    { effect: path({ id: 'b' }), source: { x: 100, y: 400 }, destination: { x: 700, y: 260 } },
  ];

  it('splits one source with several destinations at a shared node 40% along', () => {
    const groups = fanGroups(resolved);
    expect(groups.size).toBe(2);
    const a = groups.get('a')!;
    const b = groups.get('b')!;
    expect(a.node).toEqual(b.node);
    // 40 % of the way to the destinations' centroid (600, 230).
    expect(a.node.x).toBeCloseTo(100 + (600 - 100) * SCENE_RELATIONSHIP.fanAt, 4);
    // Exactly one member owns the trunk, chosen by id so it is stable frame to
    // frame rather than by screen order.
    expect([a.trunk, b.trunk].filter(Boolean)).toHaveLength(1);
    expect(a.trunk).toBe(true);
  });

  it('leaves a lone destination unfanned', () => {
    expect(fanGroups([resolved[0]!]).size).toBe(0);
  });

  it('draws the trunk and the hollow node once, and one source disc for the group', () => {
    const groups = fanGroups(resolved);
    const trunkOps = relationshipOps(
      path({ id: 'a' }),
      SOURCE,
      CARD,
      ctx({ fan: groups.get('a') }),
    );
    const branchOps = relationshipOps(
      path({ id: 'b' }),
      SOURCE,
      CARD,
      ctx({ fan: groups.get('b') }),
    );
    expect(parts(trunkOps, 'trunk').length).toBeGreaterThan(0);
    const node = parts(trunkOps, 'fan');
    expect(node).toHaveLength(1);
    expect(node[0]!.op === 'circle' && node[0]!.fill).toBe(false);
    expect(parts(trunkOps, 'source')).toHaveLength(1);
    // The branch adds neither a second disc nor a second node — that is what
    // stops N arcs from one source hiding the source.
    expect(parts(branchOps, 'trunk')).toHaveLength(0);
    expect(parts(branchOps, 'fan')).toHaveLength(0);
    expect(parts(branchOps, 'source')).toHaveLength(0);
  });

  it('keeps the taper monotonic ACROSS the trunk/branch join', () => {
    const groups = fanGroups(resolved);
    const ops = relationshipOps(path({ id: 'a' }), SOURCE, CARD, ctx({ fan: groups.get('a') }));
    const trunk = parts(ops, 'trunk').map((op) => (op.op === 'segment' ? op.width : 0));
    const branch = widths(ops);
    expect(Math.max(...trunk)).toBeLessThanOrEqual(Math.min(...branch) + 1e-9);
  });
});

describe('§4.5 — the numeral is the ordering channel', () => {
  it('draws one pip per place in the SERVER’s target list', () => {
    for (const numeral of [1, 2, 3]) {
      const ops = relationshipOps(path({ numeral }), SOURCE, CARD, ctx());
      expect(parts(ops, 'numeral')).toHaveLength(numeral);
    }
    expect(parts(relationshipOps(path(), SOURCE, CARD, ctx()), 'numeral')).toHaveLength(0);
  });

  it('survives reduced motion — order is never carried by motion (§7.2)', () => {
    const ops = relationshipOps(path({ numeral: 3 }), SOURCE, CARD, ctx({ reducedMotion: true }));
    expect(parts(ops, 'numeral')).toHaveLength(3);
  });
});

describe('§4.4 — the path states, each legible without motion', () => {
  const stateOf = (state: PersistentEffect['state']): DrawOp[] =>
    relationshipOps(path({ state }), SOURCE, CARD, ctx({ phase: 4 }));

  it('separates pending from confirmed by DASH PATTERN, which survives reduced motion', () => {
    const pending = parts(stateOf('pending'), 'path');
    const confirmed = parts(stateOf('confirmed'), 'path');
    // A dashed path is cut into fewer, shorter pieces than the solid polyline.
    const span = (ops: DrawOp[]): number =>
      ops.reduce(
        (sum, op) =>
          op.op === 'segment' ? sum + Math.hypot(op.to.x - op.from.x, op.to.y - op.from.y) : sum,
        0,
      );
    expect(span(pending)).toBeLessThan(span(confirmed) * 0.75);
    // Under reduced motion the PATTERN stays; only the crawl stops (§7.2).
    const still = parts(
      relationshipOps(path({ state: 'pending' }), SOURCE, CARD, ctx({ reducedMotion: true })),
      'path',
    );
    expect(span(still)).toBeLessThan(span(confirmed) * 0.75);
  });

  it('holds a provisional path still while a pending one crawls', () => {
    const at = (state: PersistentEffect['state'], phase: number): DrawOp[] =>
      parts(relationshipOps(path({ state }), SOURCE, CARD, ctx({ phase })), 'path');
    expect(at('pending', 0)).not.toEqual(at('pending', 7));
    expect(at('provisional', 0)).toEqual(at('provisional', 7));
  });

  it('calms a path to a lower alpha while keeping every constituent', () => {
    const calmed = stateOf('calmed');
    expect(calmed.every((op) => op.alpha === SCENE_RELATIONSHIP.alpha.calmed)).toBe(true);
    expect(parts(calmed, 'path').length).toBeGreaterThan(3);
    expect(parts(calmed, 'cap').length).toBeGreaterThan(0);
  });

  it('endpoint-only is exactly TWO circle ops — the crowded-board floor (D11)', () => {
    const ops = stateOf('endpoint-only');
    expect(ops).toHaveLength(2);
    expect(ops.every((op) => op.op === 'circle')).toBe(true);
    // Both caps survive, so the relationship is never silently lost — and D1's
    // asymmetry still states which end acts.
    expect(ops[0]!.op === 'circle' && ops[0]!.fill).toBe(true);
    expect(ops[1]!.op === 'circle' && ops[1]!.fill).toBe(false);
    expect(parts(ops, 'path')).toHaveLength(0);
  });

  it('retracts a resolving path source → destination and fades it out (§6.2 F6)', () => {
    const start = parts(relationshipOps(path({ state: 'resolving' }), SOURCE, CARD, ctx()), 'path');
    const late = parts(
      relationshipOps(path({ state: 'resolving' }), SOURCE, CARD, ctx({ progress: 0.8 })),
      'path',
    );
    const head = (ops: DrawOp[]): number => (ops[0]!.op === 'segment' ? ops[0]!.from.x : 0);
    // The stroke's head has travelled away from the source toward the target.
    expect(head(late)).toBeGreaterThan(head(start));
    expect(late[0]!.alpha).toBeLessThan(start[0]!.alpha);
    // Reduced motion drops the travel entirely: the path is simply gone in the
    // frame the state applies, leaving the caps that state the fact.
    const reduced = relationshipOps(
      path({ state: 'resolving' }),
      SOURCE,
      CARD,
      ctx({ reducedMotion: true, progress: 0 }),
    );
    expect(parts(reduced, 'path')).toHaveLength(0);
    expect(parts(reduced, 'source')).toHaveLength(1);
  });
});

describe('§8.4 / IN1 — which relationships need another frame', () => {
  it('animates ONLY pending and resolving paths', () => {
    expect(relationshipAnimates(path({ state: 'pending' }))).toBe(true);
    expect(relationshipAnimates(path({ state: 'resolving' }))).toBe(true);
    for (const state of ['provisional', 'confirmed', 'calmed', 'endpoint-only'] as const) {
      expect(relationshipAnimates(path({ state }))).toBe(false);
    }
  });

  it('never animates a block or an attachment, whatever state is declared', () => {
    for (const category of ['blocker-link', 'attachment-bracket', 'source-tether'] as const) {
      expect(relationshipAnimates(path({ category, state: 'pending' }))).toBe(false);
    }
  });

  it('defaults a targeting path to pending and everything else to confirmed', () => {
    expect(relationshipState(path())).toBe('pending');
    expect(relationshipState(path({ category: 'attack-path' }))).toBe('confirmed');
    expect(relationshipState(path({ category: 'blocker-link' }))).toBe('confirmed');
    // An attack is a server-stated fact, so it is solid and costs no frames.
    expect(relationshipAnimates(path({ category: 'attack-path' }))).toBe(false);
  });
});

describe('§8.1 — the draw-op accounting the budgets bind', () => {
  const count = (effect: PersistentEffect, to: Rect = CARD): number =>
    relationshipOps(effect, SOURCE, to, ctx()).length;

  it('holds each kind at or under its budgeted op count', () => {
    // Solid card path: 24 polyline segments + 1 disc + 1 ring + 2 chevron = 28,
    // against the §8.1 line of ~24 segments + disc + reticle + arrowhead.
    expect(count(path({ state: 'confirmed' }))).toBeLessThanOrEqual(32);
    // Dashed pending path: ~28 segments plus the caps.
    expect(count(path({ state: 'pending' }))).toBeLessThanOrEqual(40);
    // The blocker link is unchanged from the shipped shape: 2 + 1.
    expect(count(path({ category: 'blocker-link' }))).toBe(3);
    // The attachment bracket is 4 strokes + 2 terminals, exactly as budgeted.
    expect(count(path({ category: 'attachment-bracket' }))).toBe(6);
    // The crowded floor.
    expect(count(path({ state: 'endpoint-only' }))).toBe(2);
  });

  it('costs the taper ZERO extra ops — width is already a segment field (IN4)', () => {
    const tapered = relationshipOps(path({ state: 'confirmed' }), SOURCE, CARD, ctx());
    const flat = parts(tapered, 'path');
    expect(new Set(flat.map((op) => (op.op === 'segment' ? op.width : 0))).size).toBeGreaterThan(1);
    // One op per polyline sample, whatever the widths are.
    expect(flat).toHaveLength(24);
  });

  it('spawns no particles: every element of every path is a stroke (§8.2)', () => {
    for (const state of ['pending', 'confirmed', 'calmed', 'endpoint-only'] as const) {
      for (const op of relationshipOps(path({ state }), SOURCE, CARD, ctx())) {
        expect(['segment', 'circle', 'rect']).toContain(op.op);
      }
    }
  });

  it('stays inside the §8.1 worst declared stress state', () => {
    // The spec's worst case: 3 full focused paths, 7 endpoint-only, 6 combat
    // links, 4 attack paths, 4 attachments — budgeted at ~272 ops in one pass.
    let ops = 0;
    for (let i = 0; i < 3; i += 1) ops += count(path({ state: 'pending' }));
    for (let i = 0; i < 7; i += 1) ops += count(path({ state: 'endpoint-only' }));
    for (let i = 0; i < 6; i += 1) ops += count(path({ category: 'blocker-link' }));
    for (let i = 0; i < 4; i += 1)
      ops += count(path({ category: 'attack-path', endpoint: 'player' }), CREST);
    for (let i = 0; i < 4; i += 1) ops += count(path({ category: 'attachment-bracket' }));
    expect(ops).toBeLessThanOrEqual(300);
  });
});

describe('§9.4 — no two kinds are separated by hue alone', () => {
  it('gives every kind a distinct op signature at one identical accent', () => {
    const signature = (effect: PersistentEffect, to: Rect = CARD): string =>
      relationshipOps({ ...effect, accent: '#FFFFFF' }, SOURCE, to, ctx())
        .map((op) => `${op.op}:${op.part}`)
        .join('|');
    const signatures = [
      signature(path({ state: 'confirmed', endpoint: 'card' })),
      signature(path({ state: 'confirmed', endpoint: 'player' }), CREST),
      signature(path({ state: 'confirmed', endpoint: 'zone' })),
      signature(path({ category: 'blocker-link' })),
      signature(path({ category: 'attachment-bracket' })),
      signature(path({ state: 'endpoint-only' })),
    ];
    expect(new Set(signatures).size).toBe(signatures.length);
  });
});
