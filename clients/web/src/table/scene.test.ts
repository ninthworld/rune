import { describe, expect, it } from 'vitest';
import { normalizeGameView, parseGameView } from '../wire';
import {
  COMBAT_GAME_VIEW_JSON,
  FOUR_PLAYER_GAME_VIEW_JSON,
  SAMPLE_GAME_VIEW,
} from '../game-view.fixture';
import type { GameView } from '../protocol';
import { TIER } from '../tokens';
import { deriveColorIdentity } from './colorIdentity';
import { rectsOverlap } from './layout';
import {
  basicLandGlyph,
  buildTableScene,
  defaultSceneGeometry,
  rowKindForType,
  tappedFootprint,
  type PanelFrame,
  type SceneGeometry,
  type TableScene,
  type TargetingScene,
} from './scene';

/** The default carved geometry for a duel (the same carve the live shell makes). */
const GEO = defaultSceneGeometry();
/** The carved geometry for a four-seat table (three opponent panels). */
const GEO4 = defaultSceneGeometry(4);

/** Build a scene against the default duel geometry (most tests' shell). */
function build(view: GameView, selectedId?: string, targeting?: TargetingScene): TableScene {
  return buildTableScene(view, selectedId, GEO, targeting);
}

/**
 * A synthetic single-panel geometry whose receiver content area is exactly
 * `contentW` wide — for the wrap tests, where the interesting variable is the
 * row budget, not the whole shell.
 */
function panelGeometry(contentW: number): SceneGeometry {
  const frame = (y: number): PanelFrame => ({
    rect: { x: 0, y, w: contentW + 32, h: 900 },
    header: { x: 0, y, w: contentW + 32, h: 0 },
    content: { x: 16, y, w: contentW, h: 900 },
    piles: { x: contentW + 32, y, w: 0, h: 0 },
  });
  return {
    width: contentW + 32,
    height: 2000,
    opponents: [frame(0)],
    you: frame(950),
    hand: { x: 16, y: 1900, w: contentW, h: 100 },
    tiers: { you: 'field', opp: 'support' },
    handFan: false,
  };
}

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
    const scene = build(SAMPLE_GAME_VIEW);
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
    const scene = build(opening);
    expect(scene.localPlayerId).toBe('p1');
    // A local band is still laid out for the receiver even with no permanents.
    expect(scene.bands.map((b) => b.playerId)).toEqual(['p2', 'p1']);
    expect(scene.bands.at(-1)?.isLocal).toBe(true);
  });

  it('treats an absent view.you (older server) as unknown', () => {
    const legacy = normalizeGameView({ ...JSON.parse(JSON.stringify(SAMPLE_GAME_VIEW)), you: '' });
    const scene = build(legacy);
    expect(scene.localPlayerId).toBeUndefined();
    // No band is flagged local when the receiver is unknown.
    expect(scene.bands.every((b) => !b.isLocal)).toBe(true);
  });
});

describe('buildTableScene', () => {
  it('groups the battlefield into per-controller bands with the local band last', () => {
    const scene = build(SAMPLE_GAME_VIEW);
    expect(scene.bands.map((b) => b.playerId)).toEqual(['p2', 'p1']);
    const local = scene.bands.at(-1);
    expect(local?.isLocal).toBe(true);
    expect(local?.cards.map((c) => c.entityId)).toEqual(['perm_xyz']);
  });

  it('passes P/T, tapped and counters through verbatim (no game logic)', () => {
    const scene = build(SAMPLE_GAME_VIEW);
    const bear = scene.bands.at(-1)?.cards[0];
    expect(bear?.data.power).toBe('2');
    expect(bear?.data.toughness).toBe('2');
    expect(bear?.data.tapped).toBe(true);
    expect(bear?.data.counters).toEqual([{ kind: '+1/+1', count: 2 }]);
    expect(bear?.data.colorIdentity).toBe('G');
  });

  it('routes each subject-action onto its entity, leaving others non-interactive', () => {
    const scene = build(SAMPLE_GAME_VIEW);
    const bear = scene.bands.at(-1)?.cards[0];
    // The activate-ability action names perm_xyz, so it rides on the card.
    expect(bear?.actions.map((a) => a.id)).toEqual(['a2']);
    // The hand card has no subject-action → no on-entity interactivity.
    expect(scene.hand[0]?.entityId).toBe('c1');
    expect(scene.hand[0]?.actions).toEqual([]);
  });

  it('renders the local hand at hand tier', () => {
    const scene = build(SAMPLE_GAME_VIEW);
    expect(scene.hand.map((c) => c.tier)).toEqual(['hand']);
  });

  it('labels each band by its controller and marks the local one (issue #278)', () => {
    const scene = build(SAMPLE_GAME_VIEW);
    const local = scene.bands.at(-1);
    const opponent = scene.bands[0];
    expect(local?.isLocal).toBe(true);
    expect(local?.label).toBe('p1 (you)');
    expect(opponent?.label).toBe('p2');
  });

  it('gives every band a bounded region, including an empty one (issue #278)', () => {
    const scene = build(boardView(['p1'], 0));
    const band = scene.bands[0];
    expect(band?.isEmpty).toBe(true);
    // An empty panel still reserves a carved, non-zero home a newcomer can see.
    expect(band?.rect.w).toBeGreaterThan(0);
    expect(band?.rect.h).toBeGreaterThan(0);
  });

  it('carries each controller’s zone pile counts straight from the view (issue #278)', () => {
    const view = SAMPLE_GAME_VIEW;
    const scene = build(view);
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
    const scene = build(SAMPLE_GAME_VIEW);
    expect(scene.handRegion.label).toBe('Your hand');
    expect(scene.handRegion.rect.h).toBeGreaterThan(0);
    expect(scene.handRegion.rect).toEqual(GEO.hand);
  });

  it('marks the selected entity so its card draws a ring', () => {
    const scene = build(SAMPLE_GAME_VIEW, 'perm_xyz');
    expect(scene.bands.at(-1)?.cards[0]?.data.selected).toBe(true);
    expect(scene.hand[0]?.data.selected).toBe(false);
  });

  it('marks a card with offered actions as actionable and inert cards not (issue #277)', () => {
    const scene = build(SAMPLE_GAME_VIEW);
    // perm_xyz carries the activate-ability action → the playable affordance.
    expect(scene.bands.at(-1)?.cards[0]?.data.actionable).toBe(true);
    // The hand card has no subject-action → no affordance, purely from the view.
    expect(scene.hand[0]?.data.actionable).toBe(false);
  });

  it('is a pure function of its inputs: identical view → identical scene', () => {
    const a = build(SAMPLE_GAME_VIEW, 'perm_xyz');
    const b = build(SAMPLE_GAME_VIEW, 'perm_xyz');
    expect(a).toEqual(b);
  });

  it('leaves nothing targetable outside targeting mode', () => {
    const scene = build(SAMPLE_GAME_VIEW);
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

describe('buildTableScene panel row flow (carved frames, ADR 0023)', () => {
  it('lays a small board as one centered row inside the panel content area', () => {
    const scene = build(boardView(['p1'], 3));
    const cards = scene.bands[0]!.cards;
    const ys = new Set(cards.map((c) => c.rect.y));
    expect(ys.size).toBe(1); // all three share one row
    // Columns advance by card width + gap at the receiver's field tier.
    const [a, b, c] = cards;
    expect(b?.rect.x).toBe(a!.rect.x + TIER.field.w + 10);
    // The line centers within the content span rather than hugging the left edge.
    const content = GEO.you.content;
    const leftSlack = a!.rect.x - content.x;
    const rightSlack = content.x + content.w - (c!.rect.x + c!.rect.w);
    expect(Math.abs(leftSlack - rightSlack)).toBeLessThanOrEqual(1);
  });

  it('wraps a panel into rows bounded by its content width', () => {
    // A 178px content area holds exactly two field cards per row (84×2 + one 10px gap).
    const scene = buildTableScene(boardView(['p1'], 5), undefined, panelGeometry(178));
    const cards = scene.bands[0]!.cards;
    const perRowY = cards[0]!.rect.y;
    // Row 0 holds cards 0 and 1 at the same y; card 2 starts a new, lower row.
    expect(cards[1]?.rect.y).toBe(perRowY);
    expect(cards[2]?.rect.y).toBeGreaterThan(perRowY);
    // Three rows for five cards at two per row.
    expect(new Set(cards.map((c) => c.rect.y)).size).toBe(3);
    // The cards never run past the width the scene reports.
    const maxRight = Math.max(...cards.map((c) => c.rect.x + c.rect.w));
    expect(maxRight).toBeLessThanOrEqual(scene.width);
  });

  it('keeps at least one card per row even in an absurdly narrow panel', () => {
    const scene = buildTableScene(boardView(['p1'], 3), undefined, panelGeometry(10));
    const cards = scene.bands[0]!.cards;
    // One per row → three distinct rows, each card at the content's left edge.
    expect(new Set(cards.map((c) => c.rect.y)).size).toBe(3);
    expect(cards.every((c) => c.rect.x === 16)).toBe(true);
  });

  it('lays out 100 permanents across panels with no horizontal scroll', () => {
    // 50 permanents per player — the 100-permanent envelope (ui-requirements §11).
    // Everything must stay inside the carved canvas width.
    const scene = build(boardView(['p1', 'p2'], 50));
    const cards = allCards(scene);
    expect(cards).toHaveLength(100);

    // Hard requirement: nothing extends past the reported width → the board
    // never scrolls sideways; density degrades inside each panel instead.
    const maxRight = Math.max(...cards.map((c) => c.rect.x + c.rect.w));
    expect(maxRight).toBeLessThanOrEqual(scene.width);
    expect(scene.width).toBe(GEO.width);

    // Each 50-card panel must have wrapped into multiple rows (not one long row).
    for (const band of scene.bands) {
      expect(new Set(band.cards.map((c) => c.rect.y)).size).toBeGreaterThan(1);
    }
  });

  it('is deterministic: identical view + geometry → identical layout', () => {
    const a = build(boardView(['p1', 'p2'], 50));
    const b = build(boardView(['p1', 'p2'], 50));
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
    const scene = build(next);
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
    const local = build(
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
    const card = build(withKeywords).bands.at(-1)!.cards[0]!;
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
      build(view)
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

describe('buildTableScene type-grouped rows (issue #318)', () => {
  const mixed = () =>
    permBoard([
      { id: 'bear', type_line: 'Creature — Bear', power: '2', toughness: '2' },
      { id: 'signet', type_line: 'Artifact' },
      { id: 'forest', type_line: 'Basic Land — Forest' },
    ]);

  it('assigns each type group its tier: creatures→field, support→support, lands→chip', () => {
    // A roomy panel, so the density ladder stays on its full-tier rung.
    const local = buildTableScene(mixed(), undefined, panelGeometry(600)).bands.at(-1)!;
    expect(local.densityRung).toBe(0);
    const byId = new Map(local.cards.map((c) => [c.entityId, c]));
    expect(byId.get('bear')!.tier).toBe('field');
    expect(byId.get('signet')!.tier).toBe('support');
    expect(byId.get('forest')!.tier).toBe('chip');
  });

  it('steps a panel that outgrows its content down one tier rung (density ladder)', () => {
    // Three stacked rows outgrow the default duel carve's receiver panel, so the
    // panel engages rung 1: every row steps down one card tier — per panel, never
    // globally.
    const local = build(mixed()).bands.at(-1)!;
    expect(local.densityRung).toBeGreaterThanOrEqual(1);
    const byId = new Map(local.cards.map((c) => [c.entityId, c]));
    expect(byId.get('bear')!.tier).toBe('support');
    expect(byId.get('signet')!.tier).toBe('mini');
    expect(byId.get('forest')!.tier).toBe('chip');
  });

  it('orders every panel’s rows creatures → support → lands, top to bottom', () => {
    // The fixed shell's panels are self-contained homes: the row order is the
    // shared vocabulary of the blueprint mocks and never flips per seat.
    const local = build(mixed()).bands.at(-1)!;
    expect(local.rows.map((r) => r.kind)).toEqual(['creatures', 'support', 'lands']);
    const y = (k: string) => local.rows.find((r) => r.kind === k)!.rect.y;
    expect(y('creatures')).toBeLessThan(y('support'));
    expect(y('support')).toBeLessThan(y('lands'));

    const opp = build(
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
    const oy = (k: string) => opp.rows.find((r) => r.kind === k)!.rect.y;
    expect(oy('creatures')).toBeLessThan(oy('lands'));
  });

  it('labels only the lands row — rows are a sorting convention, not zones', () => {
    const local = build(mixed()).bands.at(-1)!;
    expect(local.rows.find((r) => r.kind === 'lands')!.label).toBe('Lands');
    expect(local.rows.find((r) => r.kind === 'creatures')!.label).toBeUndefined();
    expect(local.rows.find((r) => r.kind === 'support')!.label).toBeUndefined();
  });

  it('renders a basic land as a glyph chip and a nonbasic land as a named chip', () => {
    const local = build(
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
    const local = build(
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
    const local = build(
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

  it('splits permanents whose offered actions differ (action set is part of the key)', () => {
    // Both Forests look identical, but only f0 carries an offered action → the two
    // are NOT interchangeable, so they stay separate renders.
    const local = build(
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

  it('folds identically actionable permanents into one activatable stack', () => {
    // Four untapped Forests each offering the same tap-for-mana action (per-entity
    // action ids, same shape) are interchangeable: they fold into one ×4 stack that
    // keeps the representative's action, so "four Forests" reads as one chip and
    // tapping the stack floats one mana. This is the fix for boards reading as a
    // row of duplicate lands (every untapped land is always actionable).
    const local = build(
      permBoard(
        Array.from({ length: 4 }, (_, i) => ({
          id: `f${i}`,
          name: 'Forest',
          type_line: 'Basic Land — Forest',
        })),
        Array.from({ length: 4 }, (_, i) => ({
          id: `a${i}`,
          type: 'activate_ability',
          label: 'Tap for G',
          subject: [`f${i}`],
        })),
      ),
    ).bands.at(-1)!;
    expect(local.cards).toHaveLength(1);
    const stack = local.cards[0]!;
    expect(stack.stackCount).toBe(4);
    expect(stack.memberIds).toHaveLength(4);
    // The stack stays activatable via its representative's offered action.
    expect(stack.actions).toHaveLength(1);
    expect(stack.actions[0]!.subject).toContain(stack.entityId);
    expect(stack.data.actionable).toBe(true);
  });
});

describe('buildTableScene tapped footprint (issue #318)', () => {
  it('reserves the rotated footprint of a tapped field card so it cannot overlap', () => {
    const local = build(
      permBoard([
        { id: 'a', type_line: 'Creature — Bear', power: '2', toughness: '2', tapped: true },
        { id: 'b', type_line: 'Creature — Ox', power: '2', toughness: '4' },
      ]),
    ).bands.at(-1)!;
    const a = local.cards.find((c) => c.entityId === 'a')!;
    const b = local.cards.find((c) => c.entityId === 'b')!;
    // A tapped card reserves the bounding box the ~25° rotation sweeps.
    const swept = tappedFootprint(TIER.field.w, TIER.field.h);
    expect(a.rect.w).toBe(swept.w);
    expect(a.rect.h).toBe(swept.h);
    expect(a.rect.w).toBeGreaterThan(TIER.field.w);
    // The neighbor begins past the reserved footprint — no overlap.
    expect(b.rect.x).toBeGreaterThanOrEqual(a.rect.x + a.rect.w);
  });

  it('applies the one tap treatment to a chip too (same sweep, smaller card)', () => {
    const local = build(
      permBoard([{ id: 'forest', type_line: 'Basic Land — Forest', tapped: true }]),
    ).bands.at(-1)!;
    const chip = local.cards[0]!;
    const swept = tappedFootprint(TIER.chip.w, TIER.chip.h);
    expect(chip.rect.w).toBe(swept.w);
    expect(chip.rect.h).toBe(swept.h);
    expect(chip.data.tapped).toBe(true);
  });
});

describe('buildTableScene stacked targeting addressing (issue #318)', () => {
  it('expands identical candidates so each stays individually targetable', () => {
    const ids = ['c0', 'c1', 'c2', 'c3'];
    const scene = build(
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
    const local = build(
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
    const local = build(
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
    const scene = build(
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
      { candidates: ['aura'] },
    );
    const aura = scene.bands.at(-1)!.cards.find((c) => c.entityId === 'aura')!;
    expect(aura.targetable).toBe(true);
    expect(aura.stackCount).toBe(1);
  });

  it('degrades gracefully when the referenced host is not in the visible battlefield', () => {
    // The host is not on the board (e.g. an aura on an object the viewer cannot see):
    // the aura renders in its own support row exactly as an unattached permanent would.
    const local = build(
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
    expect(build(view)).toEqual(build(view));
  });
});

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
        { player_id: 'p2', hand_size: 0, life: 20, library_size: 40, graveyard_size: 0 },
        { player_id: 'p3', hand_size: 0, life: 20, library_size: 40, graveyard_size: 0 },
        { player_id: 'p4', hand_size: 0, life: 20, library_size: 40, graveyard_size: 0 },
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

  it('carries each seat’s zone-pile counts, including an eliminated seat’s', () => {
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
    expect(scene.combatLinks).toContainEqual({ blocker: 'p2_blk', attacker: 'p1_atk_a' });
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
          { player_id: 'p2', hand_size: 1, life: 20, library_size: 40, graveyard_size: 0 },
          { player_id: 'p3', hand_size: 1, life: 20, library_size: 40, graveyard_size: 0 },
          { player_id: 'p4', hand_size: 1, life: 20, library_size: 40, graveyard_size: 0 },
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

describe('buildTableScene shell-derived presentation (frames, piles, names)', () => {
  it('labels bands by display name, never by seat id, when names are supplied', () => {
    const view = permBoard([{ id: 'b1', type_line: 'Creature — Bear' }]);
    view.player_names = { p1: 'Rowan', p2: 'Kellan' };
    const scene = build(view);
    expect(scene.bands.at(-1)?.label).toBe('Rowan (you)');
    expect(scene.bands[0]?.label).toBe('Kellan');
  });

  it('falls back to the raw id when the server sent no name', () => {
    const scene = build(permBoard([{ id: 'b1', type_line: 'Creature — Bear' }]));
    expect(scene.bands.at(-1)?.label).toBe('p1 (you)');
    expect(scene.bands[0]?.label).toBe('p2');
  });

  it('lays out at scale 1 — tiers, not scaling, spend the screen (ADR 0023)', () => {
    const scene = build(permBoard([{ id: 'b1', type_line: 'Creature — Bear' }]));
    // The fixed shell never scales the scene; the legacy field stays unset.
    expect(scene.scale).toBeUndefined();
    // The scene spans exactly the carved canvas.
    expect(scene.width).toBe(GEO.width);
    expect(scene.height).toBe(GEO.height);
  });

  it('mirrors each band’s carved frame: panel rect, header strip, piles column', () => {
    const scene = build(SAMPLE_GAME_VIEW);
    const opponent = scene.bands[0]!;
    const local = scene.bands.at(-1)!;
    expect(opponent.rect).toEqual(GEO.opponents[0]!.rect);
    expect(opponent.headerRect).toEqual(GEO.opponents[0]!.header);
    expect(opponent.pileRect).toEqual(GEO.opponents[0]!.piles);
    expect(local.rect).toEqual(GEO.you.rect);
    expect(local.headerRect).toEqual(GEO.you.header);
  });

  it('reserves an opponent pile column clear of the card rows; the local panel has none', () => {
    const scene = build(boardView(['p1', 'p2'], 12));
    const opponent = scene.bands.find((b) => !b.isLocal)!;
    // The column parks at the panel's right edge, inside the panel.
    expect(opponent.pileRect.w).toBeGreaterThan(0);
    expect(opponent.pileRect.x + opponent.pileRect.w).toBe(opponent.rect.x + opponent.rect.w);
    // Cards never intrude into the reserved column.
    for (const card of opponent.cards) {
      expect(card.rect.x + card.rect.w).toBeLessThanOrEqual(opponent.pileRect.x);
    }
    // The receiver's piles live in the bottom shell's identity panel instead
    // (full composition), so the local panel reserves no column.
    const local = scene.bands.find((b) => b.isLocal)!;
    expect(local.pileRect.w).toBe(0);
  });

  it('carries the public graveyard top card on the band zones (face-up in place)', () => {
    const view = permBoard([{ id: 'b1', type_line: 'Creature — Bear' }]);
    view.graveyards = [
      {
        player_id: 'p1',
        cards: [
          { id: 'g1', name: 'Early Bear', type_line: 'Creature — Bear' },
          { id: 'g2', name: 'Cinder Shock', type_line: 'Instant', mana_cost: '{R}' },
        ],
      },
    ];
    const scene = build(view);
    const local = scene.bands.at(-1)!;
    expect(local.zones.graveyard).toBe(2);
    // The LAST card is the top of the ordered pile.
    expect(local.zones.graveyardTop).toEqual({ name: 'Cinder Shock', colorIdentity: 'R' });
    // An empty pile reports no top card.
    expect(scene.bands[0]!.zones.graveyardTop).toBeUndefined();
  });
});
