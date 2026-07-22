import { describe, expect, it } from 'vitest';
import { parseGameView } from '../wire';
import { COMBAT_GAME_VIEW_JSON, SAMPLE_GAME_VIEW } from '../game-view.fixture';
import type { TableScene } from './scene';
import { build, allCards } from './scene.fixture';

describe('buildTableScene combat state (issue #332)', () => {
  const combat = () => build(parseGameView(COMBAT_GAME_VIEW_JSON));
  const byId = (scene: TableScene) => new Map(allCards(scene).map((c) => [c.entityId, c]));

  it('passes the attacking flag and marked damage straight through to the face', () => {
    const cards = byId(combat());
    const atk = cards.get('atk_1')!;
    expect(atk.data.attacking).toBe(true);
    expect(atk.data.markedDamage).toBe(2);
    // A merely-tapped, non-attacking permanent is not marked attacking.
    const bear = build(SAMPLE_GAME_VIEW).bands.at(-1)!.cards[0]!;
    expect(bear.data.tapped).toBe(true);
    expect(bear.data.attacking ?? false).toBe(false);
  });

  it('carries every blocker→attacker link, reconstructed from the view alone', () => {
    expect(combat().combatLinks).toEqual([
      { blocker: 'blk_1', attacker: 'atk_1' },
      { blocker: 'blk_2', attacker: 'atk_2' },
      { blocker: 'blk_3', attacker: 'atk_2' },
    ]);
  });

  it('marks a blocker and counts the blockers facing each attacker', () => {
    const cards = byId(combat());
    expect(cards.get('blk_1')!.data.blocking).toBe(true);
    // atk_1 is blocked once, atk_2 twice — the readable "blocked ×N" count.
    expect(cards.get('atk_1')!.data.blockedBy).toBe(1);
    expect(cards.get('atk_2')!.data.blockedBy).toBe(2);
  });

  it('never folds a combat participant into an ×N stack', () => {
    // Two identical attackers would fold outside combat; attacking keeps them apart
    // so each keeps its own treatment and its own blocker→attacker link.
    const scene = build(
      parseGameView(
        JSON.stringify({
          you: 'p1',
          my_hand: [],
          opponents: [{ player_id: 'p2', hand_size: 0, life: 20, library_size: 40 }],
          battlefield: [
            {
              id: 'a',
              controller: 'p1',
              owner: 'p1',
              tapped: true,
              attacking: true,
              card: {
                id: 'a',
                name: 'Goblin',
                type_line: 'Creature — Goblin',
                power: '1',
                toughness: '1',
              },
            },
            {
              id: 'b',
              controller: 'p1',
              owner: 'p1',
              tapped: true,
              attacking: true,
              card: {
                id: 'b',
                name: 'Goblin',
                type_line: 'Creature — Goblin',
                power: '1',
                toughness: '1',
              },
            },
          ],
          phase: 'declare_blockers',
        }),
      ),
    );
    const local = scene.bands.at(-1)!;
    expect(local.cards).toHaveLength(2);
    expect(local.cards.every((c) => c.stackCount === 1 && c.data.attacking)).toBe(true);
  });

  it('reconstructs identical combat state from one GameView (fresh mount)', () => {
    const view = parseGameView(COMBAT_GAME_VIEW_JSON);
    expect(build(view)).toEqual(build(view));
  });

  it('has no combat links or attacking flags outside combat', () => {
    const scene = build(SAMPLE_GAME_VIEW);
    expect(scene.combatLinks).toEqual([]);
    expect(allCards(scene).every((c) => !(c.data.attacking ?? false))).toBe(true);
  });
});
