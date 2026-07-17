import { describe, expect, it } from 'vitest';
import { normalizeGameView } from '../wire';
import { SAMPLE_GAME_VIEW } from '../game-view.fixture';
import type { GameView } from '../protocol';
import { TIER } from '../tokens';
import { deriveColorIdentity } from './colorIdentity';
import {
  basicLandGlyph,
  buildTableScene,
  DEFAULT_VIEWPORT_WIDTH,
  rowKindForType,
  type TableScene,
} from './scene';

/** A minimal permanent spec for the type-grouped-band tests (issue #318). */
interface PermSpec {
  id: string;
  type_line: string;
  tapped?: boolean;
  controller?: string;
  name?: string;
  power?: string;
  toughness?: string;
  /** The host this permanent is attached to (issue #333), for clustering tests. */
  attached_to?: string;
}

/** A `GameView` with `p1` local, holding the given permanents (issue #318). */
function permBoard(perms: PermSpec[], validActions: GameView['valid_actions'] = []): GameView {
  return normalizeGameView({
    you: 'p1',
    my_hand: [],
    opponents: [{ player_id: 'p2', hand_size: 0, life: 20, library_size: 40 }],
    battlefield: perms.map((p) => ({
      id: p.id,
      controller: p.controller ?? 'p1',
      owner: p.controller ?? 'p1',
      tapped: p.tapped,
      attached_to: p.attached_to,
      card: {
        id: p.id,
        name: p.name ?? p.id,
        type_line: p.type_line,
        power: p.power,
        toughness: p.toughness,
      },
    })),
    phase: 'precombat_main',
    valid_actions: validActions,
  });
}

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

  it('labels each band by its controller and marks the local one (issue #278)', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW);
    const local = scene.bands.at(-1);
    const opponent = scene.bands[0];
    expect(local?.isLocal).toBe(true);
    expect(local?.label).toBe('p1 (you)');
    expect(opponent?.label).toBe('p2');
  });

  it('gives every band a bounded region, including an empty one (issue #278)', () => {
    const scene = buildTableScene(boardView(['p1'], 0));
    const band = scene.bands[0];
    expect(band?.isEmpty).toBe(true);
    // An empty lane still reserves a labeled, non-zero region a newcomer can see.
    expect(band?.rect.w).toBeGreaterThan(0);
    expect(band?.rect.h).toBeGreaterThan(0);
  });

  it('carries each controller’s zone pile counts straight from the view (issue #278)', () => {
    const view = SAMPLE_GAME_VIEW;
    const scene = buildTableScene(view);
    const local = scene.bands.at(-1);
    const opponent = scene.bands[0];
    // Local library comes from `me`; an opponent's from its redacted view.
    expect(local?.zones.library).toBe(view.me.library_size);
    expect(opponent?.zones.library).toBe(
      view.opponents.find((o) => o.player_id === 'p2')?.library_size ?? -1,
    );
    // Graveyard/exile counts mirror the piles the tiles read.
    expect(local?.zones.graveyard).toBe(
      view.graveyards.find((g) => g.player_id === 'p1')?.cards.length ?? -1,
    );
  });

  it('labels the hand row as its own region (issue #278)', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW);
    expect(scene.handRegion.label).toBe('Your hand');
    expect(scene.handRegion.rect.h).toBeGreaterThan(0);
  });

  it('marks the selected entity so its card draws a ring', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW, 'perm_xyz');
    expect(scene.bands.at(-1)?.cards[0]?.data.selected).toBe(true);
    expect(scene.hand[0]?.data.selected).toBe(false);
  });

  it('marks a card with offered actions as actionable and inert cards not (issue #277)', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW);
    // perm_xyz carries the activate-ability action → the playable affordance.
    expect(scene.bands.at(-1)?.cards[0]?.data.actionable).toBe(true);
    // The hand card has no subject-action → no affordance, purely from the view.
    expect(scene.hand[0]?.data.actionable).toBe(false);
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

  it('suppresses the play affordance in targeting mode (issue #277)', () => {
    // Even a card that would otherwise be actionable advertises no play affordance
    // while a target is being picked — the sole interaction is choosing a target.
    const scene = buildTableScene(SAMPLE_GAME_VIEW, undefined, 1280, {
      candidates: ['perm_xyz'],
    });
    const all = [...scene.bands.flatMap((b) => b.cards), ...scene.hand];
    expect(all.every((c) => c.data.actionable === false)).toBe(true);
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

describe('buildTableScene card-face indicators (issue #320)', () => {
  it('passes server keywords through to the card face verbatim', () => {
    const local = buildTableScene(
      permBoard([{ id: 'drake', type_line: 'Creature — Drake', power: '2', toughness: '2' }]),
    ).bands.at(-1)!;
    // Keywords come from the view; inject via a raw permanent card.
    const withKeywords = normalizeGameView({
      you: 'p1',
      my_hand: [],
      opponents: [],
      battlefield: [
        {
          id: 'drake',
          controller: 'p1',
          owner: 'p1',
          card: {
            id: 'drake',
            name: 'Drake',
            type_line: 'Creature — Drake',
            power: '2',
            toughness: '2',
            keywords: ['flying', 'deathtouch'],
          },
        },
      ],
      phase: 'precombat_main',
      valid_actions: [],
    });
    const card = buildTableScene(withKeywords).bands.at(-1)!.cards[0]!;
    expect(card.data.keywords).toEqual(['flying', 'deathtouch']);
    // Sanity: the keyword-less board carries no keywords.
    expect(local.cards[0]!.data.keywords).toBeUndefined();
  });

  it('marks a latent activated ability from the printed rules text, payable or not', () => {
    const view = normalizeGameView({
      you: 'p1',
      my_hand: [],
      opponents: [],
      battlefield: [
        {
          id: 'pinger',
          controller: 'p1',
          owner: 'p1',
          card: {
            id: 'pinger',
            name: 'Prodigal Sorcerer',
            type_line: 'Creature — Human Wizard',
            power: '1',
            toughness: '1',
            rules_text: '{T}: Deal 1 damage to any target.',
          },
        },
        {
          id: 'vanilla',
          controller: 'p1',
          owner: 'p1',
          card: {
            id: 'vanilla',
            name: 'Grizzly Bears',
            type_line: 'Creature — Bear',
            power: '2',
            toughness: '2',
          },
        },
      ],
      phase: 'precombat_main',
      // No valid_actions → the marker is independent of any offered action.
      valid_actions: [],
    });
    const byId = new Map(
      buildTableScene(view)
        .bands.at(-1)!
        .cards.map((c) => [c.entityId, c]),
    );
    expect(byId.get('pinger')!.data.hasActivatedAbility).toBe(true);
    expect(byId.get('vanilla')!.data.hasActivatedAbility).toBe(false);
  });
});

describe('rowKindForType (issue #318)', () => {
  it('routes any creature/planeswalker/battle to the creatures row', () => {
    expect(rowKindForType('Creature — Bear')).toBe('creatures');
    expect(rowKindForType('Artifact Creature — Golem')).toBe('creatures');
    expect(rowKindForType('Land Creature — Dryad')).toBe('creatures'); // creature wins
    expect(rowKindForType('Planeswalker — Jace')).toBe('creatures');
    expect(rowKindForType('Battle — Siege')).toBe('creatures');
  });
  it('routes a non-creature land to the lands row', () => {
    expect(rowKindForType('Basic Land — Forest')).toBe('lands');
    expect(rowKindForType('Land')).toBe('lands');
  });
  it('routes everything else to the support row', () => {
    expect(rowKindForType('Artifact')).toBe('support');
    expect(rowKindForType('Enchantment — Aura')).toBe('support');
  });
});

describe('basicLandGlyph (issue #318)', () => {
  it('maps each basic land to its glyph', () => {
    expect(basicLandGlyph('Basic Land — Forest')).toBe('land-forest');
    expect(basicLandGlyph('Basic Land — Island')).toBe('land-island');
    expect(basicLandGlyph('Basic Snow Land — Mountain')).toBe('land-mountain');
  });
  it('returns undefined for a nonbasic land or non-land', () => {
    expect(basicLandGlyph('Land — Desert')).toBeUndefined();
    expect(basicLandGlyph('Creature — Bear')).toBeUndefined();
  });
});

describe('buildTableScene type-grouped bands (issue #318)', () => {
  const mixed = () =>
    permBoard([
      { id: 'bear', type_line: 'Creature — Bear', power: '2', toughness: '2' },
      { id: 'signet', type_line: 'Artifact' },
      { id: 'forest', type_line: 'Basic Land — Forest' },
    ]);

  it('assigns each type group its tier: creatures→field, support→support, lands→chip', () => {
    const local = buildTableScene(mixed()).bands.at(-1)!;
    const byId = new Map(local.cards.map((c) => [c.entityId, c]));
    expect(byId.get('bear')!.tier).toBe('field');
    expect(byId.get('signet')!.tier).toBe('support');
    expect(byId.get('forest')!.tier).toBe('chip');
  });

  it('orders the local rows toward the center: creatures first, lands at the back', () => {
    const local = buildTableScene(mixed()).bands.at(-1)!;
    expect(local.rows.map((r) => r.kind)).toEqual(['creatures', 'support', 'lands']);
    // Rows stack downward — creatures (nearest center) sit above lands (nearest hand).
    const y = (k: string) => local.rows.find((r) => r.kind === k)!.rect.y;
    expect(y('creatures')).toBeLessThan(y('support'));
    expect(y('support')).toBeLessThan(y('lands'));
  });

  it('mirrors an opponent band so their creatures sit nearest the center line', () => {
    const opp = buildTableScene(
      permBoard([
        {
          id: 'o_bear',
          controller: 'p2',
          type_line: 'Creature — Bear',
          power: '2',
          toughness: '2',
        },
        { id: 'o_forest', controller: 'p2', type_line: 'Basic Land — Forest' },
      ]),
    ).bands.find((b) => b.playerId === 'p2')!;
    const y = (k: string) => opp.rows.find((r) => r.kind === k)!.rect.y;
    // Opponent is at the top; their creatures render below their lands (toward center).
    expect(y('lands')).toBeLessThan(y('creatures'));
  });

  it('labels only the lands row — rows are a sorting convention, not zones', () => {
    const local = buildTableScene(mixed()).bands.at(-1)!;
    expect(local.rows.find((r) => r.kind === 'lands')!.label).toBe('Lands');
    expect(local.rows.find((r) => r.kind === 'creatures')!.label).toBeUndefined();
    expect(local.rows.find((r) => r.kind === 'support')!.label).toBeUndefined();
  });

  it('renders a basic land as a glyph chip and a nonbasic land as a named chip', () => {
    const local = buildTableScene(
      permBoard([
        { id: 'forest', type_line: 'Basic Land — Forest' },
        { id: 'strand', name: 'Windswept Heath', type_line: 'Land' },
      ]),
    ).bands.at(-1)!;
    const byId = new Map(local.cards.map((c) => [c.entityId, c]));
    expect(byId.get('forest')!.data.landGlyph).toBe('land-forest');
    expect(byId.get('strand')!.data.landGlyph).toBeUndefined();
    expect(byId.get('strand')!.name).toBe('Windswept Heath');
  });
});

describe('buildTableScene ×N stacking (issue #318)', () => {
  it('collapses four identical untapped permanents into one ×4 render', () => {
    const local = buildTableScene(
      permBoard(
        Array.from({ length: 4 }, (_, i) => ({
          id: `f${i}`,
          name: 'Forest',
          type_line: 'Basic Land — Forest',
        })),
      ),
    ).bands.at(-1)!;
    expect(local.cards).toHaveLength(1);
    const stack = local.cards[0]!;
    expect(stack.stackCount).toBe(4);
    expect(stack.data.stackCount).toBe(4);
    expect(stack.memberIds).toHaveLength(4);
  });

  it('splits a tapped one out: ×3 untapped beside a tapped single', () => {
    const local = buildTableScene(
      permBoard([
        { id: 'f0', name: 'Forest', type_line: 'Basic Land — Forest' },
        { id: 'f1', name: 'Forest', type_line: 'Basic Land — Forest' },
        { id: 'f2', name: 'Forest', type_line: 'Basic Land — Forest' },
        { id: 'f3', name: 'Forest', type_line: 'Basic Land — Forest', tapped: true },
      ]),
    ).bands.at(-1)!;
    const counts = local.cards.map((c) => c.stackCount).sort();
    expect(counts).toEqual([1, 3]);
    const tapped = local.cards.find((c) => c.data.tapped);
    expect(tapped!.stackCount).toBe(1);
  });

  it('never folds an individually actionable permanent into a stack', () => {
    // Both Forests are identical, but only f0 carries an offered action → f0 stays
    // its own render so it remains clickable; f1 is a singleton too.
    const local = buildTableScene(
      permBoard(
        [
          { id: 'f0', name: 'Forest', type_line: 'Basic Land — Forest' },
          { id: 'f1', name: 'Forest', type_line: 'Basic Land — Forest' },
        ],
        [{ id: 'a0', type: 'activate_ability', label: 'Tap for G', subject: ['f0'] }],
      ),
    ).bands.at(-1)!;
    expect(local.cards).toHaveLength(2);
    expect(local.cards.every((c) => c.stackCount === 1)).toBe(true);
  });
});

describe('buildTableScene tapped footprint (issue #318)', () => {
  it('reserves the rotated footprint of a tapped field card so it cannot overlap', () => {
    const local = buildTableScene(
      permBoard([
        { id: 'a', type_line: 'Creature — Bear', power: '2', toughness: '2', tapped: true },
        { id: 'b', type_line: 'Creature — Ox', power: '2', toughness: '4' },
      ]),
    ).bands.at(-1)!;
    const a = local.cards.find((c) => c.entityId === 'a')!;
    const b = local.cards.find((c) => c.entityId === 'b')!;
    // Tapped card reserves h×w (rotated), so its footprint width is the card height.
    expect(a.rect.w).toBe(TIER.field.h);
    expect(a.rect.h).toBe(TIER.field.w);
    // The neighbor begins past the reserved footprint — no overlap.
    expect(b.rect.x).toBeGreaterThanOrEqual(a.rect.x + a.rect.w);
  });

  it('keeps a tapped chip un-rotated (dim + corner glyph handle tap state)', () => {
    const local = buildTableScene(
      permBoard([{ id: 'forest', type_line: 'Basic Land — Forest', tapped: true }]),
    ).bands.at(-1)!;
    const chip = local.cards[0]!;
    expect(chip.rect.w).toBe(TIER.chip.w);
    expect(chip.rect.h).toBe(TIER.chip.h);
    expect(chip.data.tapped).toBe(true);
  });
});

describe('buildTableScene stacked targeting addressing (issue #318)', () => {
  it('expands identical candidates so each stays individually targetable', () => {
    const ids = ['c0', 'c1', 'c2', 'c3'];
    const scene = buildTableScene(
      permBoard(
        ids.map((id) => ({
          id,
          name: 'Bear',
          type_line: 'Creature — Bear',
          power: '2',
          toughness: '2',
        })),
      ),
      undefined,
      DEFAULT_VIEWPORT_WIDTH,
      { candidates: ids },
    );
    const local = scene.bands.at(-1)!;
    // No collapsing while targeting: four individually pickable candidates.
    expect(local.cards).toHaveLength(4);
    expect(local.cards.every((c) => c.targetable && c.stackCount === 1)).toBe(true);
    expect(local.cards.map((c) => c.entityId).sort()).toEqual(ids);
  });
});

describe('buildTableScene aura clustering (issue #333)', () => {
  it('clusters an aura adjacent to its host, host first, in the host’s row', () => {
    const local = buildTableScene(
      permBoard([
        {
          id: 'bear',
          name: 'Grizzly Bears',
          type_line: 'Creature — Bear',
          power: '2',
          toughness: '2',
        },
        {
          id: 'aura',
          name: 'Ironbark Aegis',
          type_line: 'Enchantment — Aura',
          attached_to: 'bear',
        },
      ]),
    ).bands.at(-1)!;
    // The aura leaves the support row and rides in the host's creatures row, right
    // after the host, so the two read as one cluster.
    const creatures = local.rows.find((r) => r.kind === 'creatures')!;
    const inCreatures = local.cards.filter((c) => c.rect.y === creatures.rect.y);
    expect(inCreatures.map((c) => c.entityId)).toEqual(['bear', 'aura']);
    // No standalone support row is created for the clustered aura.
    expect(local.rows.some((r) => r.kind === 'support')).toBe(false);
    expect(local.cards.find((c) => c.entityId === 'aura')!.attachedTo).toBe('bear');
    expect(local.cards.find((c) => c.entityId === 'bear')!.attachments).toEqual(['aura']);
  });

  it('never folds an attachment or its host into an ×N stack', () => {
    // Two identical bears; only one is enchanted. Without clustering they would fold
    // into a ×2 — the enchanted host and its aura must stay their own renders.
    const local = buildTableScene(
      permBoard([
        {
          id: 'bear_a',
          name: 'Grizzly Bears',
          type_line: 'Creature — Bear',
          power: '2',
          toughness: '2',
        },
        {
          id: 'bear_b',
          name: 'Grizzly Bears',
          type_line: 'Creature — Bear',
          power: '2',
          toughness: '2',
          attached_to: undefined,
        },
        {
          id: 'aura',
          name: 'Ironbark Aegis',
          type_line: 'Enchantment — Aura',
          attached_to: 'bear_a',
        },
      ]),
    ).bands.at(-1)!;
    const host = local.cards.find((c) => c.entityId === 'bear_a')!;
    expect(host.stackCount).toBe(1);
    const aura = local.cards.find((c) => c.entityId === 'aura')!;
    expect(aura.stackCount).toBe(1);
    // The un-enchanted bear is still individually present (it has nothing to fold with).
    expect(local.cards.some((c) => c.entityId === 'bear_b')).toBe(true);
  });

  it('keeps a clustered attachment individually addressable in targeting mode', () => {
    const scene = buildTableScene(
      permBoard([
        {
          id: 'bear',
          name: 'Grizzly Bears',
          type_line: 'Creature — Bear',
          power: '2',
          toughness: '2',
        },
        {
          id: 'aura',
          name: 'Ironbark Aegis',
          type_line: 'Enchantment — Aura',
          attached_to: 'bear',
        },
      ]),
      undefined,
      DEFAULT_VIEWPORT_WIDTH,
      { candidates: ['aura'] },
    );
    const aura = scene.bands.at(-1)!.cards.find((c) => c.entityId === 'aura')!;
    expect(aura.targetable).toBe(true);
    expect(aura.stackCount).toBe(1);
  });

  it('degrades gracefully when the referenced host is not in the visible battlefield', () => {
    // The host is not on the board (e.g. an aura on an object the viewer cannot see):
    // the aura renders in its own support row exactly as an unattached permanent would.
    const local = buildTableScene(
      permBoard([
        { id: 'aura', name: 'Pacifism', type_line: 'Enchantment — Aura', attached_to: 'ghost' },
      ]),
    ).bands.at(-1)!;
    expect(local.rows.map((r) => r.kind)).toEqual(['support']);
    const aura = local.cards.find((c) => c.entityId === 'aura')!;
    expect(aura.attachedTo).toBeUndefined();
  });

  it('reconstructs identical clustering from one GameView (fresh mount)', () => {
    const view = permBoard([
      {
        id: 'bear',
        name: 'Grizzly Bears',
        type_line: 'Creature — Bear',
        power: '2',
        toughness: '2',
      },
      { id: 'aura', name: 'Ironbark Aegis', type_line: 'Enchantment — Aura', attached_to: 'bear' },
    ]);
    expect(buildTableScene(view)).toEqual(buildTableScene(view));
  });
});
