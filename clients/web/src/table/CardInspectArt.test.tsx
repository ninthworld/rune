/**
 * The inspector's illustration (ADR 0024): shown only when the player's chosen
 * art source has one loaded for the card's `functional_id`; the panel itself is
 * the baseline (and the whole panel remains pure render of the view).
 *
 * The art REGION, by contrast, is not conditional (issue #527). The popover
 * reserves it permanently and at one size for both art modes, so a download
 * finishing mid-inspect and a change of ADR 0024 art style both paint into a
 * rectangle that is already there. jsdom lays nothing out, so the assertions
 * below pin the declared contract — the slot element, its classes, and the
 * custom properties its stylesheet rule sizes itself from — not a measured
 * height; a measured height is browser verification and stays with the
 * maintainer.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { Texture } from 'pixi.js';
import { CardInspect } from './CardInspect';
import {
  configureArtStore,
  noteCards,
  resetArtStore,
  setArtSource,
  setArtStyle,
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

/** Drain the art store's promise chain (fetch → blob → cache → publish). */
async function flush(): Promise<void> {
  for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
}

/**
 * Install the stub art pipeline. `configureArtStore` swaps the whole store —
 * and with it the listener set — so this must run BEFORE the component mounts
 * or its subscription is orphaned. Both image kinds are stubbed so the ADR 0024
 * style switch has something to resolve.
 */
function stubArtStore(): void {
  const deps: Partial<ArtStoreDeps> = {
    fetchLike: () =>
      Promise.resolve({
        ok: true,
        status: 200,
        json: () =>
          Promise.resolve({
            image_uris: { art_crop: 'https://img/a.jpg', normal: 'https://img/full.jpg' },
          }),
        blob: () => Promise.resolve(new Blob(['img'])),
      }),
    cache: new MemoryArtCache(),
    loadArt: () => Promise.resolve({ texture: Texture.WHITE, url: 'blob:art-url' }),
    delay: () => Promise.resolve(),
    now: () => 1,
  };
  configureArtStore(deps);
}

/** Turn the stubbed source on for the inspected card and let it land. */
async function loadArt(): Promise<void> {
  setArtSource('scryfall');
  noteCards([{ functionalId: 'shock', name: 'Shock' }]);
  await flush();
}

/** Publish stub art for the card under the scryfall source. */
async function publishArt(): Promise<void> {
  stubArtStore();
  await loadArt();
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

describe('CardInspect reserved art slot (#527)', () => {
  /** The slot's declared geometry: the element, the classes that select its
   * stylesheet rule, and the properties that rule reads its box from. */
  function slotShape(): Record<string, string | null> {
    const slot = screen.getByTestId('card-inspect-art-slot');
    return {
      tag: slot.tagName,
      className: slot.className,
      style: slot.getAttribute('style'),
    };
  }

  it('reserves the slot when the card has no art at all', () => {
    configureArtStore({ cache: new MemoryArtCache() });
    render(<CardInspect target={{ kind: 'card', card: CARD }} onClose={vi.fn()} />);
    const slot = screen.getByTestId('card-inspect-art-slot');
    expect(slot.querySelector('img')).toBeNull();
    // Reserved, not empty: the frame's monogram placeholder on the token fill,
    // so a text-only card reads as a card with no illustration.
    expect(slot.getAttribute('data-art-mono')).toBe('S');
  });

  it('does not move the slot when a background download lands mid-inspect', async () => {
    stubArtStore();
    render(<CardInspect target={{ kind: 'card', card: CARD }} onClose={vi.fn()} />);
    const before = slotShape();
    expect(screen.queryByTestId('card-inspect-art')).toBeNull();

    await act(async () => {
      await loadArt();
    });

    // The image landed INSIDE the rectangle that was already reserved…
    const img = screen.getByTestId('card-inspect-art');
    expect(img.parentElement).toBe(screen.getByTestId('card-inspect-art-slot'));
    // …and nothing about that rectangle's declared box changed.
    expect(slotShape()).toEqual(before);
  });

  it('does not move the slot when the art mode changes', async () => {
    await publishArt();
    render(<CardInspect target={{ kind: 'card', card: CARD }} onClose={vi.fn()} />);
    const windowed = slotShape();
    expect(screen.getByTestId('card-inspect-art').getAttribute('data-art-mode')).toBe('panel');

    // Switching style starts a fresh download for the other image kind, so the
    // surface passes through "mode changed, nothing loaded yet" — the state
    // that used to remove the art block entirely. The slot survives it.
    act(() => setArtStyle('full'));
    expect(screen.queryByTestId('card-inspect-art')).toBeNull();
    expect(slotShape()).toEqual(windowed);

    // …and the substantially taller full-card mode then lands in the SAME slot.
    await act(async () => {
      await flush();
    });
    expect(screen.getByTestId('card-inspect-art').getAttribute('data-art-mode')).toBe('panelFull');
    expect(slotShape()).toEqual(windowed);

    // Back again: the cached window image returns instantly, still same slot.
    await act(async () => {
      setArtStyle('window');
      await flush();
    });
    expect(screen.getByTestId('card-inspect-art').getAttribute('data-art-mode')).toBe('panel');
    expect(slotShape()).toEqual(windowed);
  });
});
