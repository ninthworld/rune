/**
 * The inspector's illustration (ADR 0024): shown only when the player's chosen
 * art source has one loaded for the card's `functional_id`; the text-only panel
 * stays the baseline (and the whole panel remains pure render of the view).
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { Texture } from 'pixi.js';
import { CardInspect } from './CardInspect';
import {
  configureArtStore,
  noteCards,
  resetArtStore,
  setArtSource,
  type ArtStoreDeps,
} from '../card/art/artStore';
import { MemoryArtCache } from '../card/art/artCache';
import type { CardView } from '../protocol';

afterEach(() => {
  cleanup();
  resetArtStore();
  localStorage.clear();
});

const CARD: CardView = {
  id: 'c1',
  name: 'Shock',
  type_line: 'Instant',
  mana_cost: '{R}',
  functional_id: 'shock',
};

/** Publish stub art for the card under the scryfall source. */
async function publishArt(): Promise<void> {
  const deps: Partial<ArtStoreDeps> = {
    fetchLike: () =>
      Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ image_uris: { art_crop: 'https://img/a.jpg' } }),
        blob: () => Promise.resolve(new Blob(['img'])),
      }),
    cache: new MemoryArtCache(),
    loadArt: () => Promise.resolve({ texture: Texture.WHITE, url: 'blob:art-url' }),
    delay: () => Promise.resolve(),
    now: () => 1,
  };
  configureArtStore(deps);
  setArtSource('scryfall');
  noteCards([{ functionalId: 'shock', name: 'Shock' }]);
  for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
}

describe('CardInspect art (ADR 0024)', () => {
  it('shows the loaded illustration for the inspected card', async () => {
    await publishArt();
    render(<CardInspect target={{ kind: 'card', card: CARD }} onClose={vi.fn()} />);
    const img = screen.getByTestId('card-inspect-art');
    expect(img.getAttribute('src')).toBe('blob:art-url');
  });

  it('renders the text-only panel when no art is loaded', () => {
    configureArtStore({ cache: new MemoryArtCache() });
    render(<CardInspect target={{ kind: 'card', card: CARD }} onClose={vi.fn()} />);
    expect(screen.queryByTestId('card-inspect-art')).toBeNull();
    expect(screen.getByTestId('card-inspect-name').textContent).toBe('Shock');
  });
});
