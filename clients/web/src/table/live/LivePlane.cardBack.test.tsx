/**
 * The card back where it actually lands: the library pile on #531's zone rack
 * (`docs/design/card-representation.md` §13, `zone-geography.md` §3).
 *
 * The library is the client's one hidden-card surface today. Face-down
 * *permanents* are not one: no `GameView` field distinguishes a face-down
 * permanent (issue #551) and face-down-ness is never inferred, so nothing here
 * looks for one.
 *
 * **jsdom's limits, stated plainly.** No CSS-module stylesheet is applied and no
 * image is decoded, so the pile's *appearance* is unobservable here — that the
 * back is drawn, that it stays lower-contrast than a card, that a fallback is
 * pixel-identical in box. What is observable is the contract: which custom
 * property the plane publishes, that it is one value for every hidden pile, that
 * no card identity reaches a library node, and that the failure path re-resolves.
 */
import { cleanup, fireEvent, render, act } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import {
  CARD_BACK_SKINS,
  DEFAULT_CARD_BACK_ID,
  cardBackSkin,
  resetCardBackStore,
  setCardBackId,
} from '../../card/back';
import { LivePlane } from './LivePlane';

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

function mount() {
  const view = render(
    <LivePlane view={SAMPLE_GAME_VIEW} quality="standard" density="reduced" reducedMotion />,
  );
  const host = view.getByTestId('live-2-5d-plane');
  return { view, host };
}

/** Every drawn library pile in the reconciled plane. */
function libraryPiles(host: HTMLElement): HTMLElement[] {
  return [...host.querySelectorAll<HTMLElement>("[data-slot='zone'][data-zone='library']")];
}

beforeEach(() => {
  vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
});

afterEach(() => {
  cleanup();
  resetCardBackStore();
  localStorage.clear();
  vi.restoreAllMocks();
});

describe('the card back on the zone rack', () => {
  it('publishes the default back once, for every hidden pile on the plane', () => {
    const { host } = mount();
    const expected = `url("${cardBackSkin(DEFAULT_CARD_BACK_ID)!.src}")`;
    expect(host.style.getPropertyValue('--card-back-image')).toBe(expected);
    expect(host.dataset.cardBack).toBe(DEFAULT_CARD_BACK_ID);
    // The property lives on the plane root, not on a pile, which is exactly why
    // no pile can have its own: there is one declaration for all of them.
    const piles = libraryPiles(host);
    expect(piles.length).toBeGreaterThan(0);
    for (const pile of piles) {
      expect(pile.style.getPropertyValue('--card-back-image')).toBe('');
    }
  });

  it('never lets a library pile carry the identity of a card it hides', () => {
    // `zone-geography.md` §I2 — hidden stays hidden. The graveyard publishes its
    // top card because the view does; the library has no such field and must
    // never acquire one, in the DOM or anywhere else.
    const { host } = mount();
    for (const pile of libraryPiles(host)) {
      expect(pile.dataset.top).toBeUndefined();
      expect(pile.dataset.topColor).toBeUndefined();
      expect(pile.dataset.name).toBeUndefined();
      // Its whole payload is the reconciler key, the zone, the seat, the count,
      // and the rack variant — a count is public information the view already
      // states. Anything else appearing here would be a new channel.
      expect(Object.keys(pile.dataset).sort()).toEqual([
        'count',
        'eliminated',
        'key',
        'seat',
        'slot',
        'variant',
        'zone',
      ]);
    }
    // The graveyard, by contrast, DOES publish its public top card — which is
    // what makes the library's silence a deliberate difference and not an
    // accident of the fixture.
    const graveyards = [
      ...host.querySelectorAll<HTMLElement>("[data-slot='zone'][data-zone='graveyard']"),
    ];
    expect(graveyards.length).toBeGreaterThan(0);
  });

  it('gives every seat’s library the same back — one device, one back', () => {
    // A per-seat back would be a channel. Every pile resolves through the same
    // single property, so they cannot differ even in principle.
    const { host } = mount();
    const seats = new Set(libraryPiles(host).map((pile) => pile.dataset.seat));
    expect(seats.size).toBeGreaterThan(0);
    // Exactly one node in the whole plane declares the property: the root.
    const declaring = [...host.querySelectorAll<HTMLElement>('[style]'), host].filter(
      (node) => node.style.getPropertyValue('--card-back-image') !== '',
    );
    expect(declaring).toEqual([host]);
  });

  it('applies a chosen skin to the same property, with no layout consequence', () => {
    const alternate = CARD_BACK_SKINS.find((skin) => skin.id !== DEFAULT_CARD_BACK_ID)!;
    const { host, view } = mount();
    const before = libraryPiles(host).length;
    act(() => setCardBackId(alternate.id));
    view.rerender(
      <LivePlane view={SAMPLE_GAME_VIEW} quality="standard" density="reduced" reducedMotion />,
    );
    expect(host.style.getPropertyValue('--card-back-image')).toBe(`url("${alternate.src}")`);
    // Same piles, same count, same slots: a skin changes a URL and nothing else.
    expect(libraryPiles(host)).toHaveLength(before);
  });

  it('falls a failed skin back to the default without touching the rack', () => {
    const alternate = CARD_BACK_SKINS.find((skin) => skin.id !== DEFAULT_CARD_BACK_ID)!;
    setCardBackId(alternate.id);
    const { host } = mount();
    const before = libraryPiles(host).map((pile) => pile.getAttribute('style'));
    expect(host.style.getPropertyValue('--card-back-image')).toBe(`url("${alternate.src}")`);
    // The probe exists because a CSS background cannot report a failure.
    act(() => {
      fireEvent.error(host.querySelector("[data-testid='card-back-probe']")!);
    });
    expect(host.style.getPropertyValue('--card-back-image')).toBe(
      `url("${cardBackSkin(DEFAULT_CARD_BACK_ID)!.src}")`,
    );
    expect(libraryPiles(host).map((pile) => pile.getAttribute('style'))).toEqual(before);
  });

  it('keeps the rack complete when every skin fails — the pile is never a hole', () => {
    const { host } = mount();
    const before = libraryPiles(host).length;
    for (let attempt = 0; attempt < CARD_BACK_SKINS.length + 1; attempt += 1) {
      const probe = host.querySelector("[data-testid='card-back-probe']");
      if (probe === null) break;
      act(() => {
        fireEvent.error(probe);
      });
    }
    expect(host.dataset.cardBack).toBe('procedural');
    expect(host.style.getPropertyValue('--card-back-image')).toBe('none');
    expect(libraryPiles(host)).toHaveLength(before);
  });

  it('loads the back exactly once, and never as an announced or focusable node', () => {
    const { host } = mount();
    const probes = [...host.querySelectorAll<HTMLImageElement>("[data-testid='card-back-probe']")];
    expect(probes).toHaveLength(1);
    expect(probes[0]!.getAttribute('aria-hidden')).toBe('true');
    expect(probes[0]!.getAttribute('alt')).toBe('');
    expect(probes[0]!.getAttribute('tabindex')).toBeNull();
    expect(probes[0]!.getAttribute('src')).toBe(cardBackSkin(DEFAULT_CARD_BACK_ID)!.src);
  });
});
