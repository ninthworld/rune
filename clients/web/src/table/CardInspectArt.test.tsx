/**
 * The inspected card's illustration (ADR 0024): shown only when the player's
 * chosen art source has one loaded for the card's `functional_id`.
 *
 * Since issue #569 the surface draws the shared `CardFace` at its `inspect`
 * tier, so the art here is the face's own reserved slot rather than a second
 * panel-owned art block — which is exactly the point: there is one art path, and
 * the enlarged view shows the face the art mode already resolved. These tests
 * therefore address the slot through the primitive's own stable hooks
 * (`data-art-slot`, `data-art-mode`) rather than a surface-local test id.
 *
 * The art REGION is not conditional (issue #527). It is reserved permanently and
 * at one size for both art modes, so a download finishing mid-inspect and a
 * change of ADR 0024 art style both paint into a rectangle that is already
 * there. jsdom lays nothing out, so the assertions below pin the declared
 * contract — the slot element, its classes, and the custom properties its
 * stylesheet rule sizes itself from — not a measured height; a measured height
 * is browser verification and stays with the maintainer.
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

/** The card image inside the inspected face, or `null` when none has landed. */
function artImage(): HTMLImageElement | null {
  return screen.getByTestId('card-inspect').querySelector('img[data-art-mode]');
}

/** The face's permanently reserved art slot (issue #527). */
function artSlot(): HTMLElement {
  const slot = screen.getByTestId('card-inspect').querySelector<HTMLElement>('[data-art-slot]');
  if (!slot) throw new Error('the inspected face reserved no art slot');
  return slot;
}

/** The drawn face's own root, addressed by its tier. */
function inspectFace(): HTMLElement | null {
  return screen.getByTestId('card-inspect').querySelector('[data-tier="inspect"]');
}

describe('CardInspect art (ADR 0024)', () => {
  it('shows the loaded illustration for the inspected card', async () => {
    await publishArt();
    render(<CardInspect target={{ kind: 'card', card: CARD }} onClose={vi.fn()} />);
    expect(artImage()?.getAttribute('src')).toBe('blob:art-url');
  });

  it('draws the procedural face when no art is loaded', () => {
    configureArtStore({ cache: new MemoryArtCache() });
    render(<CardInspect target={{ kind: 'card', card: CARD }} onClose={vi.fn()} />);
    expect(artImage()).toBeNull();
    // The card is still there, drawn by the one renderer under its own name.
    expect(inspectFace()?.getAttribute('aria-label')).toBe('Shock');
  });
});

describe('CardInspect reserved art slot (#527)', () => {
  /** The slot's declared geometry: the element, the classes that select its
   * stylesheet rule, and the properties that rule reads its box from. */
  function slotShape(): Record<string, string | null> {
    const slot = artSlot();
    return {
      tag: slot.tagName,
      className: slot.className,
      style: slot.getAttribute('style'),
    };
  }

  it('reserves the slot when the card has no art at all', () => {
    configureArtStore({ cache: new MemoryArtCache() });
    render(<CardInspect target={{ kind: 'card', card: CARD }} onClose={vi.fn()} />);
    const slot = artSlot();
    expect(slot.querySelector('img')).toBeNull();
    // Reserved, not empty: the frame's monogram placeholder on the token fill,
    // so a text-only card reads as a card with no illustration.
    expect(slot.getAttribute('data-art-mono')).toBe('S');
  });

  it('does not move the slot when a background download lands mid-inspect', async () => {
    stubArtStore();
    render(<CardInspect target={{ kind: 'card', card: CARD }} onClose={vi.fn()} />);
    const before = slotShape();
    expect(artImage()).toBeNull();

    await act(async () => {
      await loadArt();
    });

    // The image landed INSIDE the rectangle that was already reserved…
    expect(artImage()?.parentElement).toBe(artSlot());
    // …and nothing about that rectangle's declared box changed.
    expect(slotShape()).toEqual(before);
  });

  it('does not move the slot when the art mode changes', async () => {
    await publishArt();
    render(<CardInspect target={{ kind: 'card', card: CARD }} onClose={vi.fn()} />);
    const windowed = slotShape();
    expect(artImage()?.getAttribute('data-art-mode')).toBe('panel');

    // Switching style starts a fresh download for the other image kind, so the
    // surface passes through "mode changed, nothing loaded yet" — the state
    // that used to remove the art block entirely. The slot survives it.
    act(() => setArtStyle('full'));
    expect(artImage()).toBeNull();
    expect(slotShape()).toEqual(windowed);

    // …and the substantially taller full-card mode then lands in the SAME slot.
    await act(async () => {
      await flush();
    });
    expect(artImage()?.getAttribute('data-art-mode')).toBe('panelFull');
    expect(slotShape()).toEqual(windowed);

    // Back again: the cached window image returns instantly, still same slot.
    await act(async () => {
      setArtStyle('window');
      await flush();
    });
    expect(artImage()?.getAttribute('data-art-mode')).toBe('panel');
    expect(slotShape()).toEqual(windowed);
  });
});
