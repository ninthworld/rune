import { describe, expect, it } from 'vitest';
import { normalizeGameView } from '../wire';
import { SAMPLE_GAME_VIEW } from '../game-view.fixture';
import type { GameView } from '../protocol';
import { TIER } from '../tokens';
import { deriveColorIdentity } from './colorIdentity';
import { buildTableScene, DEFAULT_VIEWPORT_WIDTH, type TableScene } from './scene';

/** A `GameView` whose battlefield holds `perController` permanents for each id. */
function boardView(controllers: string[], perController: number): GameView {
  const battlefield = controllers.flatMap((controller) =>
    Array.from({ length: perController }, (_, i) => ({
      id: `${controller}_perm_${i}`,
      controller,
      owner: controller,
      card: {
        id: `${controller}_perm_${i}`,
        name: `Servo ${i}`,
        type_line: 'Artifact Creature — Servo',
        power: '1',
        toughness: '1',
      },
    })),
  );
  return normalizeGameView({
    you: controllers[0],
    my_hand: [],
    opponents: controllers.slice(1).map((player_id) => ({
      player_id,
      hand_size: 0,
      life: 20,
      library_size: 40,
    })),
    battlefield,
    phase: 'precombat_main',
    valid_actions: [],
  });
}

/** Every rendered card in the scene (all bands + hand), position included. */
function allCards(scene: TableScene) {
  return [...scene.bands.flatMap((b) => b.cards), ...scene.hand];
}

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

  it('leaves nothing targetable outside targeting mode', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW);
    const all = [...scene.bands.flatMap((b) => b.cards), ...scene.hand];
    expect(all.every((c) => c.targetable === false)).toBe(true);
    expect(all.every((c) => c.data.targeting === undefined || c.data.targeting === false)).toBe(
      true,
    );
  });
});

describe('buildTableScene targeting mode (ADR 0009 §Client)', () => {
  it('highlights exactly the server candidates and dims everything else', () => {
    // perm_xyz is a legal target; the hand card c1 is not.
    const scene = buildTableScene(SAMPLE_GAME_VIEW, undefined, 1280, { candidates: ['perm_xyz'] });
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

  it('suppresses the selection ring while targeting (a target is not a selection)', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW, 'perm_xyz', 1280, { candidates: ['perm_xyz'] });
    // Even though perm_xyz was the selected id, targeting mode clears `selected`.
    expect(scene.bands.at(-1)?.cards[0]?.data.selected).toBe(false);
  });

  it('stays a pure function of its inputs in targeting mode', () => {
    const a = buildTableScene(SAMPLE_GAME_VIEW, undefined, 1280, { candidates: ['perm_xyz'] });
    const b = buildTableScene(SAMPLE_GAME_VIEW, undefined, 1280, { candidates: ['perm_xyz'] });
    expect(a).toEqual(b);
  });

  it('marks a chosen multi-select candidate as selected (issue #143)', () => {
    // A candidate already toggled into the answer is `chosen` and draws the
    // selection ring; a candidate not yet chosen stays merely targetable.
    const scene = buildTableScene(SAMPLE_GAME_VIEW, undefined, 1280, {
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
    const scene = buildTableScene(SAMPLE_GAME_VIEW, undefined, 1280, {
      candidates: ['perm_xyz'],
      selected: [],
    });
    const bear = scene.bands.at(-1)?.cards[0];
    expect(bear?.chosen).toBe(false);
    expect(bear?.data.selected).toBe(false);
  });

  it('lays a single small band as one row (no wrapping when everything fits)', () => {
    const scene = buildTableScene(boardView(['p1'], 3), undefined, 1280);
    const ys = new Set(scene.bands[0]?.cards.map((c) => c.rect.y));
    expect(ys.size).toBe(1); // all three share one row
    // Columns advance by card width + gap; the first card sits at the margin.
    const [a, b] = scene.bands[0]!.cards;
    expect(a?.rect.x).toBe(16);
    expect(b?.rect.x).toBe(16 + TIER.field.w + 12);
  });

  it('wraps a band into rows bounded by the viewport width', () => {
    // A 260px budget fits exactly two field cards per row (16*2 margins, 12 gap).
    const scene = buildTableScene(boardView(['p1'], 5), undefined, 260);
    const cards = scene.bands[0]!.cards;
    const perRowY = cards[0]!.rect.y;
    // Row 0 holds cards 0 and 1 at the same y; card 2 starts a new, lower row.
    expect(cards[1]?.rect.y).toBe(perRowY);
    expect(cards[2]?.rect.y).toBeGreaterThan(perRowY);
    expect(cards[2]?.rect.x).toBe(16); // wraps back to the left margin
    // Three rows for five cards at two per row.
    expect(new Set(cards.map((c) => c.rect.y)).size).toBe(3);
    // The band never runs past the width the scene reports.
    const maxRight = Math.max(...cards.map((c) => c.rect.x + c.rect.w));
    expect(scene.width).toBeGreaterThanOrEqual(maxRight);
    expect(scene.width).toBeLessThanOrEqual(260);
  });

  it('keeps at least one card per row even in an absurdly narrow viewport', () => {
    const scene = buildTableScene(boardView(['p1'], 3), undefined, 10);
    const cards = scene.bands[0]!.cards;
    // One per row → three distinct rows, each card at the left margin.
    expect(new Set(cards.map((c) => c.rect.y)).size).toBe(3);
    expect(cards.every((c) => c.rect.x === 16)).toBe(true);
  });

  it('lays out 100 permanents across bands with no horizontal page scroll', () => {
    // 55 permanents for the local player, 45 for the opponent — the 100-permanent
    // envelope (ui-requirements §11). Everything must fit inside the width budget.
    const scene = buildTableScene(boardView(['p1', 'p2'], 50), undefined, DEFAULT_VIEWPORT_WIDTH);
    const cards = allCards(scene);
    expect(cards).toHaveLength(100);

    // Hard requirement: nothing extends past the reported width, and the reported
    // width stays within the viewport budget → the board never scrolls sideways.
    const maxRight = Math.max(...cards.map((c) => c.rect.x + c.rect.w));
    expect(maxRight).toBeLessThanOrEqual(scene.width);
    expect(scene.width).toBeLessThanOrEqual(DEFAULT_VIEWPORT_WIDTH);

    // Each 50-card band must have wrapped into multiple rows (not one long row).
    for (const band of scene.bands) {
      expect(new Set(band.cards.map((c) => c.rect.y)).size).toBeGreaterThan(1);
    }
    // The board grows downward instead: its height exceeds a single band.
    expect(scene.height).toBeGreaterThan(TIER.field.h * 3);
  });

  it('is deterministic: identical view + width → identical layout', () => {
    const a = buildTableScene(boardView(['p1', 'p2'], 50), undefined, DEFAULT_VIEWPORT_WIDTH);
    const b = buildTableScene(boardView(['p1', 'p2'], 50), undefined, DEFAULT_VIEWPORT_WIDTH);
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
