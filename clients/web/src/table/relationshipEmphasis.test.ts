/**
 * The §4.4 / §9.3 emphasis policy (issue #535): focus isolates one object's
 * relationships, a crowded board drops to endpoint-only, and nothing is ever
 * silently removed. Pure — no rects, no GPU, no view.
 */
import { describe, expect, it } from 'vitest';
import { COMBAT_LINK } from '../tokens';
import { SCENE_HUES, SCENE_NEUTRALS } from '../sceneTokens';
import type { PersistentEffect } from './effects';
import { applyRelationshipEmphasis, relationshipTouches } from './relationshipEmphasis';

function link(id: string, from: string, to: string): PersistentEffect {
  return {
    id,
    category: 'blocker-link',
    from: { ref: from },
    to: { ref: to },
    accent: SCENE_HUES.orange.value,
    state: 'confirmed',
  };
}

const many = (n: number): PersistentEffect[] =>
  Array.from({ length: n }, (_, i) => link(`l${i}`, `b${i}`, `a${i}`));

describe('relationshipTouches', () => {
  it('matches an entity, its seat cluster, and its stack slot', () => {
    expect(relationshipTouches(link('l', 'p1', 'x'), 'p1')).toBe(true);
    expect(relationshipTouches(link('l', 'x', 'seat:p1'), 'p1')).toBe(true);
    expect(relationshipTouches(link('l', 'stack:s1', 'x'), 's1')).toBe(true);
    expect(relationshipTouches(link('l', 'a', 'b'), 'p1')).toBe(false);
  });
});

describe('applyRelationshipEmphasis (§4.4)', () => {
  it('leaves a quiet board completely alone', () => {
    const effects = many(2);
    expect(applyRelationshipEmphasis(effects, null)).toEqual(effects);
  });

  it('isolates the focused object and CALMS the rest — never hides one (§9.3)', () => {
    const effects = [link('l1', 'blk', 'atk'), link('l2', 'other', 'far')];
    const out = applyRelationshipEmphasis(effects, 'blk');
    expect(out).toHaveLength(2);
    expect(out[0]!.state).toBe('confirmed');
    expect(out[1]!.state).toBe('calmed');
  });

  it('drops to endpoint-only past the crowded threshold with nothing focused', () => {
    const under = applyRelationshipEmphasis(many(COMBAT_LINK.crowdedThreshold), null);
    expect(under.every((effect) => effect.state === 'confirmed')).toBe(true);
    const over = applyRelationshipEmphasis(many(COMBAT_LINK.crowdedThreshold + 1), null);
    expect(over.every((effect) => effect.state === 'endpoint-only')).toBe(true);
    // The relationship is reduced, never lost: the count is unchanged.
    expect(over).toHaveLength(COMBAT_LINK.crowdedThreshold + 1);
  });

  it('never touches the live targeting session the player is answering', () => {
    const live: PersistentEffect = {
      id: 'target:preview',
      category: 'targeting-path',
      from: { ref: 'src' },
      to: { ref: 'dst' },
      accent: SCENE_HUES.orange.value,
      state: 'pending',
    };
    const provisional: PersistentEffect = { ...live, id: 'target:0', state: 'provisional' };
    const crowded = applyRelationshipEmphasis(
      [live, provisional, ...many(COMBAT_LINK.crowdedThreshold + 1)],
      null,
    );
    expect(crowded[0]!.state).toBe('pending');
    expect(crowded[1]!.state).toBe('provisional');
    // A crowded board still calms everything ELSE.
    expect(crowded[2]!.state).toBe('endpoint-only');
    // And so does an isolation that the session does not touch.
    const isolated = applyRelationshipEmphasis([live, link('l', 'a', 'b')], 'zzz');
    expect(isolated[0]!.state).toBe('pending');
    expect(isolated[1]!.state).toBe('calmed');
  });

  it('never re-states an attachment bracket — structure is not emphasis (R9)', () => {
    const bracket: PersistentEffect = {
      id: 'attach:aura',
      category: 'attachment-bracket',
      from: { ref: 'aura' },
      to: { ref: 'host' },
      accent: SCENE_NEUTRALS.text,
      state: 'confirmed',
    };
    for (const isolation of [null, 'someone-else']) {
      const out = applyRelationshipEmphasis([bracket, ...many(8)], isolation);
      expect(out[0]).toEqual(bracket);
    }
  });

  it('does not count attachments toward the crowding threshold', () => {
    const brackets: PersistentEffect[] = Array.from({ length: 8 }, (_, i) => ({
      id: `attach:${i}`,
      category: 'attachment-bracket',
      from: { ref: `a${i}` },
      to: { ref: `h${i}` },
      accent: SCENE_NEUTRALS.text,
      state: 'confirmed',
    }));
    const out = applyRelationshipEmphasis([...brackets, link('l', 'b', 'a')], null);
    expect(out[out.length - 1]!.state).toBe('confirmed');
  });
});
