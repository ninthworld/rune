import { describe, expect, it } from 'vitest';
import { normalizeGameView } from '../wire';
import { TIER } from '../tokens';
import { basicLandGlyph, buildTableScene, rowKindForType, tappedFootprint } from './scene';
import { build, GEO, allCards, boardView, panelGeometry, permBoard } from './scene.fixture';

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

  it('orders every panel\'s rows creatures → support → lands, top to bottom', () => {
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
