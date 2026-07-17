/**
 * The play-affordance half of issue #277: the canvas draws the visible "playable"
 * edge bar, and the overlay's select hotspot must carry an accessible-tree
 * equivalent (a hint naming the offered action) so the affordance survives for a
 * screen-reader or no-color-vision user. Nothing outside `valid_actions[]` may be
 * interactive, so an inert card gets no hotspot at all.
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { EntityOverlay } from './EntityOverlay';
import { buildTableScene } from './scene';
import { SAMPLE_GAME_VIEW } from '../game-view.fixture';

const noop = () => {};

function renderOverlay() {
  const scene = buildTableScene(SAMPLE_GAME_VIEW);
  render(
    <EntityOverlay
      scene={scene}
      selectedId={null}
      targeting={false}
      onSelect={noop}
      onChoose={noop}
      onPickTarget={noop}
    />,
  );
}

describe('EntityOverlay play affordance (issue #277)', () => {
  it('names the offered action on an actionable card so the hint is accessible', () => {
    renderOverlay();
    // perm_xyz carries the "Tap for mana" activate-ability action.
    const hotspot = screen.getByTestId('entity-perm_xyz');
    expect(hotspot.getAttribute('data-actionable')).toBe('true');
    expect(hotspot.getAttribute('aria-label')).toContain('Tap for mana');
  });

  it('gives no select hotspot to a card with no offered action', () => {
    renderOverlay();
    // The inert hand card (Llanowar Elves, c1) carries no subject-action, so it is
    // neither actionable nor selectable — only its inspect handle exists.
    expect(screen.queryByTestId('entity-c1')).toBeNull();
  });
});
