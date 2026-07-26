import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { Texture } from 'pixi.js';
import { ArtSettings } from './ArtSettings';
import {
  configureArtStore,
  getArtSource,
  getArtStyle,
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

/** Offline store deps: instant art, no network, no persistence side effects. */
function offlineDeps(): Partial<ArtStoreDeps> {
  return {
    fetchLike: () =>
      Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ image_uris: { art_crop: 'https://img/a.jpg' } }),
        blob: () => Promise.resolve(new Blob(['img'])),
      }),
    cache: new MemoryArtCache(),
    loadArt: () => Promise.resolve({ texture: Texture.WHITE, url: 'blob:stub' }),
    delay: () => Promise.resolve(),
    now: () => 1,
  };
}

describe('ArtSettings (ADR 0024)', () => {
  it('offers the three sources with the device preference checked', () => {
    configureArtStore(offlineDeps());
    render(<ArtSettings onClose={vi.fn()} />);
    expect(screen.getByTestId('art-source-procedural').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('art-source-bundled').getAttribute('aria-checked')).toBe('false');
    expect(screen.getByTestId('art-source-scryfall').getAttribute('aria-checked')).toBe('false');
  });

  it('gates Scryfall behind an explicit consent step', () => {
    configureArtStore(offlineDeps());
    render(<ArtSettings onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('art-source-scryfall'));
    // Choosing the option arms consent; nothing downloads yet.
    expect(screen.getByTestId('art-consent')).toBeDefined();
    expect(getArtSource()).toBe('procedural');
    fireEvent.click(screen.getByTestId('art-consent-accept'));
    expect(getArtSource()).toBe('scryfall');
    expect(screen.queryByTestId('art-consent')).toBeNull();
  });

  it('keeps the procedural source when consent is declined', () => {
    configureArtStore(offlineDeps());
    render(<ArtSettings onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('art-source-scryfall'));
    fireEvent.click(screen.getByTestId('art-consent-cancel'));
    expect(getArtSource()).toBe('procedural');
    expect(screen.queryByTestId('art-consent')).toBeNull();
  });

  it('switches back to procedural without any consent step', () => {
    configureArtStore(offlineDeps());
    setArtSource('bundled');
    render(<ArtSettings onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('art-source-procedural'));
    expect(getArtSource()).toBe('procedural');
  });

  it('reports live progress over the cards the current game wants', async () => {
    configureArtStore(offlineDeps());
    setArtSource('scryfall');
    noteCards([{ functionalId: 'shock', name: 'Shock' }]);
    for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
    render(<ArtSettings onClose={vi.fn()} />);
    expect(screen.getByTestId('art-status').textContent).toContain('1 of 1');
    // The clear control is available for the downloaded set.
    expect(screen.getByTestId('art-clear')).toBeDefined();
  });

  it('offers the presentation styles only under the Scryfall source', () => {
    configureArtStore(offlineDeps());
    render(<ArtSettings onClose={vi.fn()} />);
    // Procedural: no presentation choice to make.
    expect(screen.queryByTestId('art-style')).toBeNull();
    fireEvent.click(screen.getByTestId('art-source-scryfall'));
    fireEvent.click(screen.getByTestId('art-consent-accept'));
    // Scryfall active: the illustration-in-frame default is checked.
    expect(screen.getByTestId('art-style-window').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('art-style-full').getAttribute('aria-checked')).toBe('false');
  });

  it('switches to the entire-card presentation (ADR 0024 full-card mode)', async () => {
    configureArtStore(offlineDeps());
    setArtSource('scryfall');
    render(<ArtSettings onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('art-style-full'));
    expect(getArtStyle()).toBe('full');
    expect(screen.getByTestId('art-style-full').getAttribute('aria-checked')).toBe('true');
    await Promise.resolve();
  });

  it('closes on the backdrop', () => {
    configureArtStore(offlineDeps());
    const onClose = vi.fn();
    render(<ArtSettings onClose={onClose} />);
    fireEvent.click(screen.getByTestId('art-settings-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe('ArtSettings workflow (issue #569)', () => {
  const CARD: CardView = {
    id: 'c1',
    name: 'Shock',
    type_line: 'Instant',
    mana_cost: '{R}',
    functional_id: 'shock',
  };

  it('states what a choice applies to, per group and overall', () => {
    configureArtStore(offlineDeps());
    render(<ArtSettings onClose={vi.fn()} />);
    // Global to the device, never per card, never sent anywhere.
    const scope = screen.getByTestId('art-scope').textContent ?? '';
    expect(scope).toContain('every card');
    expect(scope).toContain('this browser');
    expect(scope).toContain('never sent to the server');
    // …and the source group repeats it, because a player may read only one.
    expect(screen.getByTestId('art-source-scope').textContent).toContain('every card');
    // The picker is described by that line rather than leaving it to be guessed.
    const group = screen.getByRole('radiogroup', { name: /where art comes from/i });
    expect(group.getAttribute('aria-describedby')).toBe('art-source-scope');
  });

  it('names the style group as a sub-choice of the Scryfall source', () => {
    configureArtStore(offlineDeps());
    setArtSource('scryfall');
    render(<ArtSettings onClose={vi.fn()} />);
    const group = screen.getByTestId('art-style');
    expect(group.getAttribute('aria-label')).toContain('How a downloaded image is drawn');
    expect(group.getAttribute('aria-label')).toContain('every card, on this device only');
  });

  it('previews the setting with the same face every other surface draws', () => {
    configureArtStore(offlineDeps());
    render(<ArtSettings onClose={vi.fn()} previewCard={CARD} />);
    const preview = screen.getByTestId('art-preview');
    const face = within(preview).getByRole('img', { name: 'Shock' });
    // The one card renderer — not a mock-up, not a second art path (ADR 0024).
    expect(face.getAttribute('data-tier')).toBe('stack');
    expect(face.getAttribute('data-kind')).toBe('card');
  });

  it('omits the preview where there is no card to show', () => {
    configureArtStore(offlineDeps());
    render(<ArtSettings onClose={vi.fn()} />);
    expect(screen.queryByTestId('art-preview')).toBeNull();
  });

  it('offers an explicit way out beside the backdrop dismissal', () => {
    configureArtStore(offlineDeps());
    const onClose = vi.fn();
    render(<ArtSettings onClose={onClose} />);
    fireEvent.click(screen.getByTestId('art-settings-done'));
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId('art-settings-close'));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
