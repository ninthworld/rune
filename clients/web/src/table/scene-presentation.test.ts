import { describe, expect, it } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../game-view.fixture';
import { build, GEO, boardView, permBoard } from './scene.fixture';

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

  it("mirrors each band's carved frame: panel rect, header strip, piles column", () => {
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
          {
            id: 'g2',
            name: 'Cinder Shock',
            type_line: 'Instant',
            mana_cost: '{R}',
          },
        ],
      },
    ];
    const scene = build(view);
    const local = scene.bands.at(-1)!;
    expect(local.zones.graveyard).toBe(2);
    // The LAST card is the top of the ordered pile.
    expect(local.zones.graveyardTop).toEqual({
      name: 'Cinder Shock',
      colorIdentity: 'R',
    });
    // An empty pile reports no top card.
    expect(scene.bands[0]!.zones.graveyardTop).toBeUndefined();
  });
});
