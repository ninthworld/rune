import { describe, expect, it } from 'vitest';
import { normalizeGameView } from '../wire';
import { SAMPLE_GAME_VIEW } from '../game-view.fixture';
import type { GameView } from '../protocol';
import { deriveColorIdentity } from './colorIdentity';
import { buildTableScene } from './scene';

describe('deriveColorIdentity', () => {
  it('frames any land as land regardless of cost', () => {
    expect(deriveColorIdentity({ id: 'x', name: 'Forest', type_line: 'Basic Land — Forest' })).toBe(
      'L',
    );
    expect(
      deriveColorIdentity({
        id: 'x',
        name: 'Ancient Tomb',
        type_line: 'Land',
        mana_cost: '{2}',
      }),
    ).toBe('L');
  });

  it('reads a single color from the mana cost', () => {
    expect(
      deriveColorIdentity({ id: 'x', name: 'Bears', type_line: 'Creature', mana_cost: '{1}{G}' }),
    ).toBe('G');
  });

  it('marks two or more colors as multicolor', () => {
    expect(
      deriveColorIdentity({ id: 'x', name: 'Bolt', type_line: 'Instant', mana_cost: '{W}{U}' }),
    ).toBe('M');
  });

  it('treats hybrid pips as the colors they name', () => {
    expect(
      deriveColorIdentity({ id: 'x', name: 'Hybrid', type_line: 'Creature', mana_cost: '{W/U}' }),
    ).toBe('M');
  });

  it('falls back to colorless for generic-only or absent costs', () => {
    expect(
      deriveColorIdentity({ id: 'x', name: 'Sol Ring', type_line: 'Artifact', mana_cost: '{1}' }),
    ).toBe('C');
    expect(
      deriveColorIdentity({ id: 'x', name: 'Ornithopter', type_line: 'Artifact Creature' }),
    ).toBe('C');
  });
});

describe('buildTableScene local player', () => {
  it('identifies the receiver straight from view.you', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW);
    expect(scene.localPlayerId).toBe('p1');
    expect(scene.bands.at(-1)?.isLocal).toBe(true);
  });

  it('resolves the local band at game start, before any public zone exists', () => {
    // The heuristic this replaces returned undefined on an empty opening board;
    // `view.you` names the receiver even with nothing on the table yet.
    const opening: GameView = {
      ...SAMPLE_GAME_VIEW,
      you: 'p1',
      battlefield: [],
      graveyards: [],
      exile: [],
      priority_player: undefined,
    };
    const scene = buildTableScene(opening);
    expect(scene.localPlayerId).toBe('p1');
    // A local band is still laid out for the receiver even with no permanents.
    expect(scene.bands.map((b) => b.playerId)).toEqual(['p2', 'p1']);
    expect(scene.bands.at(-1)?.isLocal).toBe(true);
  });

  it('treats an absent view.you (older server) as unknown', () => {
    const legacy = normalizeGameView({ ...JSON.parse(JSON.stringify(SAMPLE_GAME_VIEW)), you: '' });
    const scene = buildTableScene(legacy);
    expect(scene.localPlayerId).toBeUndefined();
    // No band is flagged local when the receiver is unknown.
    expect(scene.bands.every((b) => !b.isLocal)).toBe(true);
  });
});

describe('buildTableScene', () => {
  it('groups the battlefield into per-controller bands with the local band last', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW);
    expect(scene.bands.map((b) => b.playerId)).toEqual(['p2', 'p1']);
    const local = scene.bands.at(-1);
    expect(local?.isLocal).toBe(true);
    expect(local?.cards.map((c) => c.entityId)).toEqual(['perm_xyz']);
  });

  it('passes P/T, tapped and counters through verbatim (no game logic)', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW);
    const bear = scene.bands.at(-1)?.cards[0];
    expect(bear?.data.power).toBe('2');
    expect(bear?.data.toughness).toBe('2');
    expect(bear?.data.tapped).toBe(true);
    expect(bear?.data.counters).toEqual([{ kind: '+1/+1', count: 2 }]);
    expect(bear?.data.colorIdentity).toBe('G');
  });

  it('routes each subject-action onto its entity, leaving others non-interactive', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW);
    const bear = scene.bands.at(-1)?.cards[0];
    // The activate-ability action names perm_xyz, so it rides on the card.
    expect(bear?.actions.map((a) => a.id)).toEqual(['a2']);
    // The hand card has no subject-action → no on-entity interactivity.
    expect(scene.hand[0]?.entityId).toBe('c1');
    expect(scene.hand[0]?.actions).toEqual([]);
  });

  it('renders the local hand at hand tier', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW);
    expect(scene.hand.map((c) => c.tier)).toEqual(['hand']);
  });

  it('marks the selected entity so its card draws a ring', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW, 'perm_xyz');
    expect(scene.bands.at(-1)?.cards[0]?.data.selected).toBe(true);
    expect(scene.hand[0]?.data.selected).toBe(false);
  });

  it('is a pure function of its inputs: identical view → identical scene', () => {
    const a = buildTableScene(SAMPLE_GAME_VIEW, 'perm_xyz');
    const b = buildTableScene(SAMPLE_GAME_VIEW, 'perm_xyz');
    expect(a).toEqual(b);
  });

  it('rebuilds wholesale from a replacement view (reconstruct-from-one-GameView)', () => {
    // A completely different second frame: the scene must reflect only it, with
    // no residue of the first — the reconnect/replay invariant.
    const next = normalizeGameView({
      my_hand: [{ id: 'h9', name: 'Opt', type_line: 'Instant', mana_cost: '{U}' }],
      opponents: [{ player_id: 'p2', hand_size: 4, life: 12, library_size: 40, graveyard_size: 3 }],
      battlefield: [
        {
          id: 'perm_new',
          controller: 'p2',
          owner: 'p2',
          card: { id: 'perm_new', name: 'Island', type_line: 'Basic Land — Island' },
        },
      ],
      phase: 'end',
      valid_actions: [],
    });
    const scene = buildTableScene(next);
    const allBattlefield = scene.bands.flatMap((b) => b.cards.map((c) => c.entityId));
    expect(allBattlefield).toEqual(['perm_new']);
    expect(allBattlefield).not.toContain('perm_xyz');
    expect(scene.hand.map((c) => c.entityId)).toEqual(['h9']);
    // No valid_actions → nothing interactive anywhere.
    expect(scene.bands.flatMap((b) => b.cards).every((c) => c.actions.length === 0)).toBe(true);
  });
});
