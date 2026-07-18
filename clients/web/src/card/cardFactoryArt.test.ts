/**
 * The card factory's art window (ADR 0024): with a published illustration the
 * face draws a cover-cropped, masked sprite in place of the monogram; without
 * one (or at a dense tier) the procedural face is byte-identical to before.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { Container, Sprite, Text, Texture } from 'pixi.js';
import { buildCardDisplay, cardVisualSignature, type CardDisplayData } from './cardFactory';
import {
  artKeyFor,
  configureArtStore,
  noteCards,
  resetArtStore,
  setArtSource,
  setArtStyle,
  type ArtStoreDeps,
} from './art/artStore';
import { MemoryArtCache } from './art/artCache';
import type { ArtStyle } from './art/artSettings';

afterEach(() => {
  resetArtStore();
  localStorage.clear();
});

/** Publish stub art for one functional id and return its signature key. */
async function publishArt(functionalId: string, style: ArtStyle = 'window'): Promise<string> {
  const deps: Partial<ArtStoreDeps> = {
    fetchLike: () =>
      Promise.resolve({
        ok: true,
        status: 200,
        json: () =>
          Promise.resolve({
            image_uris: { art_crop: 'https://img/crop.jpg', normal: 'https://img/full.jpg' },
          }),
        blob: () => Promise.resolve(new Blob(['img'])),
      }),
    cache: new MemoryArtCache(),
    loadArt: () => Promise.resolve({ texture: Texture.WHITE, url: 'blob:stub' }),
    delay: () => Promise.resolve(),
    now: () => 1,
  };
  configureArtStore(deps);
  setArtSource('scryfall');
  setArtStyle(style);
  noteCards([{ functionalId, name: functionalId }]);
  for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  const key = artKeyFor(functionalId);
  if (!key) throw new Error('art did not publish');
  return key;
}

/** Every plain-`Text` string in a display object, depth first. */
function collectTexts(node: Container): string[] {
  const found: string[] = [];
  const walk = (n: Container): void => {
    for (const child of n.children) {
      if (child instanceof Text) found.push(child.text);
      if (child instanceof Container) walk(child);
    }
  };
  walk(node);
  return found;
}

/** Collect the named art-layer sprites in a display object, depth first. */
function collectSprites(node: Container): Sprite[] {
  const found: Sprite[] = [];
  const walk = (n: Container): void => {
    for (const child of n.children) {
      if (child instanceof Sprite && child.name === 'card-art') found.push(child);
      if (child instanceof Container) walk(child);
    }
  };
  walk(node);
  return found;
}

const CARD: CardDisplayData = {
  name: 'Emberfang Jackal',
  typeLine: 'Creature — Jackal',
  colorIdentity: 'R',
  manaCost: '{1}{R}',
  power: '2',
  toughness: '1',
};

describe('card factory art window (ADR 0024)', () => {
  it('draws a masked art sprite at the field tier when art is published', async () => {
    const artKey = await publishArt('emberfang_jackal');
    const display = buildCardDisplay({ ...CARD, artKey }, 'field');
    const sprites = collectSprites(display);
    expect(sprites.length).toBe(1);
    expect(sprites[0]!.mask).not.toBeNull();
    expect(sprites[0]!.texture).toBe(Texture.WHITE);
  });

  it('keeps the procedural face without an art key', () => {
    const display = buildCardDisplay(CARD, 'field');
    expect(collectSprites(display).length).toBe(0);
  });

  it('keeps dense tiers procedural even when art is published', async () => {
    const artKey = await publishArt('emberfang_jackal');
    for (const tier of ['mini', 'support'] as const) {
      const display = buildCardDisplay({ ...CARD, artKey }, tier);
      expect(collectSprites(display).length).toBe(0);
    }
  });

  it('renders art at the hand tier', async () => {
    const artKey = await publishArt('emberfang_jackal');
    const display = buildCardDisplay({ ...CARD, artKey }, 'hand');
    expect(collectSprites(display).length).toBe(1);
  });

  it('renders the entire card image as the face in full-card mode', async () => {
    const artKey = await publishArt('emberfang_jackal', 'full');
    const display = buildCardDisplay({ ...CARD, keywords: ['haste'] }, 'field');
    // Without the key the procedural face renders as usual…
    expect(collectSprites(display).length).toBe(0);
    const fullFace = buildCardDisplay({ ...CARD, keywords: ['haste'], artKey }, 'field');
    const sprites = collectSprites(fullFace);
    expect(sprites.length).toBe(1);
    expect(sprites[0]!.mask).not.toBeNull();
    // …with the key, RUNE's printed text is suppressed (it's on the image): no
    // name, no type line — while the server-computed P/T pill stays on top.
    const texts = collectTexts(fullFace);
    expect(texts.some((t) => t.startsWith('Emberf'))).toBe(false);
    expect(texts.some((t) => t.startsWith('Creatu'))).toBe(false);
    expect(texts).toContain('2/1');
  });

  it('renders full-card mode at every full-face tier, dense tiers included', async () => {
    const artKey = await publishArt('emberfang_jackal', 'full');
    for (const tier of ['mini', 'support', 'field', 'hand'] as const) {
      const display = buildCardDisplay({ ...CARD, artKey }, tier);
      expect(collectSprites(display).length).toBe(1);
    }
  });

  it('changes the visual signature when art arrives, so the reconciler rebuilds', () => {
    const plain = cardVisualSignature(CARD, 'field');
    const withArt = cardVisualSignature({ ...CARD, artKey: 'scryfall:x#1' }, 'field');
    expect(withArt).not.toBe(plain);
    // A stale key that no longer resolves still keeps the face renderable: the
    // factory simply finds no texture and draws the procedural face.
    const display = buildCardDisplay({ ...CARD, artKey: 'scryfall:gone#99' }, 'field');
    expect(collectSprites(display).length).toBe(0);
  });
});
