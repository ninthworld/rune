import { describe, expect, it } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../game-view.fixture';
import { build } from './scene.fixture';

describe('buildTableScene targeting mode (ADR 0009 §Client)', () => {
  it('highlights exactly the server candidates and dims everything else', () => {
    // perm_xyz is a legal target; the hand card c1 is not.
    const scene = build(SAMPLE_GAME_VIEW, undefined, { candidates: ['perm_xyz'] });
    const bear = scene.bands.at(-1)?.cards[0];
    const handCard = scene.hand[0];

    // The candidate is highlighted and pickable, with its normal actions suppressed.
    expect(bear?.entityId).toBe('perm_xyz');
    expect(bear?.targetable).toBe(true);
    expect(bear?.data.targeting).toBe(true);
    expect(bear?.data.dimmed).toBe(false);
    expect(bear?.actions).toEqual([]);

    // Everything else is dimmed and non-interactive — legality came from the
    // server's candidate list, never computed here.
    expect(handCard?.entityId).toBe('c1');
    expect(handCard?.targetable).toBe(false);
    expect(handCard?.data.targeting).toBe(false);
    expect(handCard?.data.dimmed).toBe(true);
    expect(handCard?.actions).toEqual([]);
  });

  it('suppresses the play affordance in targeting mode (issue #277)', () => {
    // Even a card that would otherwise be actionable advertises no play affordance
    // while a target is being picked — the sole interaction is choosing a target.
    const scene = build(SAMPLE_GAME_VIEW, undefined, { candidates: ['perm_xyz'] });
    const all = [...scene.bands.flatMap((b) => b.cards), ...scene.hand];
    expect(all.every((c) => c.data.actionable === false)).toBe(true);
  });

  it('suppresses the selection ring while targeting (a target is not a selection)', () => {
    const scene = build(SAMPLE_GAME_VIEW, 'perm_xyz', { candidates: ['perm_xyz'] });
    // Even though perm_xyz was the selected id, targeting mode clears `selected`.
    expect(scene.bands.at(-1)?.cards[0]?.data.selected).toBe(false);
  });

  it('stays a pure function of its inputs in targeting mode', () => {
    const a = build(SAMPLE_GAME_VIEW, undefined, { candidates: ['perm_xyz'] });
    const b = build(SAMPLE_GAME_VIEW, undefined, { candidates: ['perm_xyz'] });
    expect(a).toEqual(b);
  });

  it('marks a chosen multi-select candidate as selected (issue #143)', () => {
    // A candidate already toggled into the answer is `chosen` and draws the
    // selection ring; a candidate not yet chosen stays merely targetable.
    const scene = build(SAMPLE_GAME_VIEW, undefined, {
      candidates: ['perm_xyz'],
      selected: ['perm_xyz'],
    });
    const bear = scene.bands.at(-1)?.cards[0];
    expect(bear?.targetable).toBe(true);
    expect(bear?.chosen).toBe(true);
    expect(bear?.data.selected).toBe(true);
    expect(bear?.data.targeting).toBe(true);
  });

  it('does not mark an unchosen candidate as selected', () => {
    const scene = build(SAMPLE_GAME_VIEW, undefined, {
      candidates: ['perm_xyz'],
      selected: [],
    });
    const bear = scene.bands.at(-1)?.cards[0];
    expect(bear?.chosen).toBe(false);
    expect(bear?.data.selected).toBe(false);
  });
});
