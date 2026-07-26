import { describe, expect, it } from 'vitest';
import { cardVisualSignature, parseManaCost, type CardDisplayData } from './cardFactory';
import { PIP } from '../tokens';

/** The pure card display-data model (ADR 0030): the Pixi draw path was retired
 * with the legacy scene stack (#504); only the signature/parse helpers remain. */

const base = (over: Partial<CardDisplayData> = {}): CardDisplayData => ({
  name: 'Grizzly Bears',
  typeLine: 'Creature — Bear',
  colorIdentity: 'G',
  power: '2',
  toughness: '2',
  ...over,
});

describe('parseManaCost', () => {
  it('splits a braced cost into one pip per symbol, in order', () => {
    const pips = parseManaCost('{1}{G}{G}');
    expect(pips.map((p) => p.symbol)).toEqual(['1', 'G', 'G']);
    expect(pips[1]).toEqual({ symbol: 'G', bg: PIP.G.bg, fg: PIP.G.fg });
  });

  it('falls back to the neutral swatch for an unknown symbol', () => {
    const [pip] = parseManaCost('{X}');
    expect(pip).toEqual({ symbol: 'X', bg: PIP.N.bg, fg: PIP.N.fg });
  });

  it('still draws a pip for a code the vocabulary does not know (issue #462)', () => {
    // The shared tokenizer classifies it as unknown; a cost may still never
    // lose a symbol the server sent, so it draws neutral and verbatim.
    expect(parseManaCost('{WEIRD}')).toEqual([{ symbol: 'WEIRD', bg: PIP.N.bg, fg: PIP.N.fg }]);
  });

  it('returns nothing for an empty or symbol-free string', () => {
    expect(parseManaCost('')).toEqual([]);
    expect(parseManaCost('no braces')).toEqual([]);
  });
});

describe('cardVisualSignature', () => {
  it('is stable for identical display data and tier', () => {
    expect(cardVisualSignature(base(), 'field')).toBe(cardVisualSignature(base(), 'field'));
  });

  it('changes when a visible field changes', () => {
    expect(cardVisualSignature(base({ tapped: false }))).not.toBe(
      cardVisualSignature(base({ tapped: true })),
    );
    expect(cardVisualSignature(base({ stackCount: 1 }))).not.toBe(
      cardVisualSignature(base({ stackCount: 4 })),
    );
    expect(cardVisualSignature(base(), 'field')).not.toBe(cardVisualSignature(base(), 'chip'));
  });

  it('folds equal-looking cards: the art key and counters ride the signature', () => {
    expect(cardVisualSignature(base({ artKey: 'a' }))).not.toBe(
      cardVisualSignature(base({ artKey: 'b' })),
    );
    expect(cardVisualSignature(base({ counters: [{ kind: '+1/+1', count: 1 }] }))).not.toBe(
      cardVisualSignature(base({ counters: [{ kind: '+1/+1', count: 2 }] })),
    );
  });
});
