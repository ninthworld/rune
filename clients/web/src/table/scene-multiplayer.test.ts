import { describe, expect, it } from 'vitest';
import { normalizeGameView, parseGameView } from '../wire';
import type { GameView } from '../protocol';
import { FOUR_PLAYER_GAME_VIEW_JSON, COMBAT_GAME_VIEW_JSON } from '../game-view.fixture';
import { buildTableScene } from './scene';
import { rectsOverlap } from './layout';
import { build, GEO4, allCards } from './scene.fixture';

describe('buildTableScene multiplayer table (3–4 players, issue #348)', () => {
  /** Build against the four-seat carve — one panel frame per opponent. */
  const build4 = (view: GameView) => buildTableScene(view, undefined, GEO4);

  /** A view whose `seat_order` lists the opponents in a scrambled order relative to
   * `view.opponents`, to prove the arrangement follows `seat_order`, not projection
   * order. `p1` local, opponents p2/p3/p4, seat order p1,p4,p3,p2. */
  function scrambledSeatOrder(): GameView {
    return normalizeGameView({
      you: 'p1',
      my_hand: [],
      opponents: [
        {
          player_id: 'p2',
          hand_size: 0,
          life: 20,
          library_size: 40,
          graveyard_size: 0,
        },
        {
          player_id: 'p3',
          hand_size: 0,
          life: 20,
          library_size: 40,
          graveyard_size: 0,
        },
        {
          player_id: 'p4',
          hand_size: 0,
          life: 20,
          library_size: 40,
          graveyard_size: 0,
        },
      ],
      battlefield: [
        {
          id: 'p3_perm',
          controller: 'p3',
          owner: 'p3',
          card: {
            id: 'p3_perm',
            name: 'Bear',
            type_line: 'Creature — Bear',
            power: '2',
            toughness: '2',
          },
        },
      ],
      phase: 'precombat_main',
      seat_order: ['p1', 'p4', 'p3', 'p2'],
      valid_actions: [],
    });
  }

  it('lays a band for every seat with the receiver anchored last (bottom)', () => {
    const scene = build4(parseGameView(FOUR_PLAYER_GAME_VIEW_JSON));
    // Four seats → four bands; the local band is last and flagged local.
    expect(scene.bands).toHaveLength(4);
    expect(scene.bands.at(-1)?.isLocal).toBe(true);
    expect(scene.bands.at(-1)?.playerId).toBe('p1');
    expect(scene.bands.slice(0, -1).every((b) => !b.isLocal)).toBe(true);
    // The local panel sits below every opponent panel.
    const local = scene.bands.at(-1)!.rect;
    for (const opp of scene.bands.slice(0, -1)) {
      expect(local.y).toBeGreaterThanOrEqual(opp.rect.y + opp.rect.h);
    }
  });

  it('assigns each opponent their own carved panel frame', () => {
    const scene = build4(parseGameView(FOUR_PLAYER_GAME_VIEW_JSON));
    const rects = scene.bands.map((b) => b.rect);
    // Panels are the fixed anatomy's homes: pairwise disjoint by construction.
    for (let i = 0; i < rects.length; i += 1) {
      for (let j = i + 1; j < rects.length; j += 1) {
        expect(rectsOverlap(rects[i]!, rects[j]!)).toBe(false);
      }
    }
    // The hand region sits below the local panel — the receiver keeps the bottom.
    const local = scene.bands.at(-1)!.rect;
    expect(scene.handRegion.rect.y).toBeGreaterThanOrEqual(local.y + local.h);
  });

  it('stacks opponent areas in seat order, not projection order', () => {
    const scene = build4(scrambledSeatOrder());
    // seat_order p1,p4,p3,p2 → opponents render p4, p3, p2, local last.
    expect(scene.bands.map((b) => b.playerId)).toEqual(['p4', 'p3', 'p2', 'p1']);
  });

  it('keeps opponent areas in a stable arrangement across a view update', () => {
    // The same table, one turn later (life/hand totals changed): the seat order —
    // and therefore the band order — must be identical, so opponents never reshuffle.
    const first = build4(scrambledSeatOrder());
    const later = scrambledSeatOrder();
    later.opponents = later.opponents.map((o) => ({ ...o, life: o.life - 3 }));
    const second = build4(later);
    expect(second.bands.map((b) => b.playerId)).toEqual(first.bands.map((b) => b.playerId));
  });

  it("carries each seat's zone-pile counts, including an eliminated seat's", () => {
    const scene = build4(parseGameView(FOUR_PLAYER_GAME_VIEW_JSON));
    const byId = new Map(scene.bands.map((b) => [b.playerId, b]));
    expect(byId.get('p2')?.zones.graveyard).toBe(2);
    // p3 is eliminated with an empty battlefield but still shows its public piles.
    expect(byId.get('p3')?.zones.graveyard).toBe(3);
    expect(byId.get('p3')?.isEmpty).toBe(true);
    expect(byId.get('p4')?.zones.graveyard).toBe(1);
  });

  it('keeps combat treatments and links legible across opponent areas', () => {
    const scene = build4(parseGameView(FOUR_PLAYER_GAME_VIEW_JSON));
    const cards = allCards(scene);
    // Both of the local player's split attackers read as attacking…
    const rhino = cards.find((c) => c.entityId === 'p1_atk_a');
    const falcon = cards.find((c) => c.entityId === 'p1_atk_b');
    expect(rhino?.data.attacking).toBe(true);
    expect(falcon?.data.attacking).toBe(true);
    // …each carrying whom it attacks on its face (issue #347)…
    expect(rhino?.data.attackingPlayer).toBe('p2');
    expect(falcon?.data.attackingPlayer).toBe('p4');
    // …and the blocker→attacker link spanning p2's area is reconstructed from the view.
    expect(scene.combatLinks).toContainEqual({
      blocker: 'p2_blk',
      attacker: 'p1_atk_a',
    });
  });

  it('reconstructs who-attacks-whom from the view alone (fresh-mount readable)', () => {
    // A player mounting mid-combat (only the view, no history) derives the same split
    // attack assignments — attacker → attacked player — as one who watched declaration.
    const scene = build4(parseGameView(FOUR_PLAYER_GAME_VIEW_JSON));
    expect(scene.attackTargets).toEqual([
      { attacker: 'p1_atk_a', defender: 'p2' },
      { attacker: 'p1_atk_b', defender: 'p4' },
    ]);
    // Deterministic: two fresh builds of the same view produce identical assignments.
    const again = build4(parseGameView(FOUR_PLAYER_GAME_VIEW_JSON));
    expect(again.attackTargets).toEqual(scene.attackTargets);
  });

  it('has no attack targets in a two-player view (sole opponent implied)', () => {
    // COMBAT_GAME_VIEW is a duel: attackers carry no `attacking_player`, so the scene
    // reports no split-attack assignments — the two-player render is unchanged.
    const scene = build(parseGameView(COMBAT_GAME_VIEW_JSON));
    expect(scene.attackTargets).toEqual([]);
  });

  it('renders three opponent areas even when some are empty', () => {
    // A three-opponent table where two opponents control nothing still shows three
    // opponent bands — density degrades, areas never disappear.
    const scene = build4(
      normalizeGameView({
        you: 'p1',
        my_hand: [],
        opponents: [
          {
            player_id: 'p2',
            hand_size: 1,
            life: 20,
            library_size: 40,
            graveyard_size: 0,
          },
          {
            player_id: 'p3',
            hand_size: 1,
            life: 20,
            library_size: 40,
            graveyard_size: 0,
          },
          {
            player_id: 'p4',
            hand_size: 1,
            life: 20,
            library_size: 40,
            graveyard_size: 0,
          },
        ],
        battlefield: [],
        phase: 'precombat_main',
        seat_order: ['p1', 'p2', 'p3', 'p4'],
        valid_actions: [],
      }),
    );
    expect(scene.bands.map((b) => b.playerId)).toEqual(['p2', 'p3', 'p4', 'p1']);
    expect(scene.bands.every((b) => b.isEmpty)).toBe(true);
  });
});
