/**
 * The DOM half of issue #278: labeled, bounded player lanes + zone piles anchored
 * to the scene's band/hand rects. The scene-side data (labels, rects, counts) is
 * covered in scene.test.ts; this covers the rendered chrome — labels, the empty
 * lane's invite-to-play placeholder, the library pile count, and the
 * graveyard/exile pile affordances opening the existing browsers (issue #262).
 */
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { TableGeography } from './TableGeography';
import { buildTableScene } from './scene';
import { SAMPLE_GAME_VIEW } from '../game-view.fixture';
import { normalizeGameView } from '../wire';

describe('TableGeography (issue #278)', () => {
  afterEach(cleanup);

  it('labels each player lane and the hand row', () => {
    render(<TableGeography scene={buildTableScene(SAMPLE_GAME_VIEW)} />);
    expect(screen.getByTestId('band-label-p1').textContent).toBe('p1 (you)');
    expect(screen.getByTestId('band-label-p2').textContent).toBe('p2');
    expect(screen.getByTestId('hand-label').textContent).toBe('Your hand');
  });

  it('shows a live library count in each player lane', () => {
    const scene = buildTableScene(SAMPLE_GAME_VIEW);
    render(<TableGeography scene={scene} />);
    const local = scene.bands.at(-1)!;
    const pile = screen.getByTestId(`library-pile-${local.playerId}`);
    // The count is the pile's single home; the zone name rides its accessible label.
    expect(pile.textContent).toContain(`${local.zones.library}`);
    expect(pile.getAttribute('aria-label')).toContain(`library (${local.zones.library})`);
  });

  it('opens the graveyard and exile browsers from the table piles', () => {
    const onOpenZone = vi.fn();
    render(<TableGeography scene={buildTableScene(SAMPLE_GAME_VIEW)} onOpenZone={onOpenZone} />);
    fireEvent.click(screen.getByTestId('table-graveyard-p2'));
    fireEvent.click(screen.getByTestId('table-exile-p1'));
    expect(onOpenZone).toHaveBeenCalledWith('p2', 'graveyard');
    expect(onOpenZone).toHaveBeenCalledWith('p1', 'exile');
  });

  it('omits the pile buttons but keeps the library on a read-only board', () => {
    render(<TableGeography scene={buildTableScene(SAMPLE_GAME_VIEW)} />);
    // No onOpenZone → graveyard/exile are not interactive, but the library (hidden
    // info, no browser) still shows.
    expect(screen.queryByTestId('table-graveyard-p1')).toBeNull();
    expect(screen.getByTestId('library-pile-p1')).toBeDefined();
  });

  it('invites play in an empty local battlefield', () => {
    // A fresh game: p1 alone, no permanents → an empty, labeled local lane.
    const view = normalizeGameView({
      you: 'p1',
      my_hand: [],
      opponents: [],
      battlefield: [],
      phase: 'precombat_main',
      valid_actions: [],
    });
    render(<TableGeography scene={buildTableScene(view)} />);
    const hint = screen.getByTestId('empty-band-p1');
    expect(hint.textContent).toContain('Your battlefield');
  });

  it('labels only the lands row — rows are a sorting convention (issue #318)', () => {
    const view = normalizeGameView({
      you: 'p1',
      my_hand: [],
      opponents: [],
      battlefield: [
        {
          id: 'bear',
          controller: 'p1',
          owner: 'p1',
          card: {
            id: 'bear',
            name: 'Bear',
            type_line: 'Creature — Bear',
            power: '2',
            toughness: '2',
          },
        },
        {
          id: 'forest',
          controller: 'p1',
          owner: 'p1',
          card: { id: 'forest', name: 'Forest', type_line: 'Basic Land — Forest' },
        },
      ],
      phase: 'precombat_main',
      valid_actions: [],
    });
    render(<TableGeography scene={buildTableScene(view)} />);
    expect(screen.getByTestId('row-label-p1-lands').textContent).toBe('Lands');
    expect(screen.queryByTestId('row-label-p1-creatures')).toBeNull();
  });
});
