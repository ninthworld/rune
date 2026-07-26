/**
 * The symbol-notation vocabulary (issue #462): one tokenizer over the server's
 * `{…}` strings, and the plain-text substitution the aria contexts use.
 *
 * The tokenizer is pure display formatting — it decides how a string is drawn
 * and spoken, never what anything costs or may produce.
 */
import { describe, expect, it } from 'vitest';
import { PIP } from '../../tokens';
import { hasSymbolNotation, symbolNotationText, tokenizeNotation } from './notation';

/** The compact shape a test cares about: kind plus its identifying text. */
function shape(text: string): string[] {
  return tokenizeNotation(text).map((token) =>
    token.kind === 'text' ? `text:${token.text}` : `${token.kind}:${token.code}`,
  );
}

describe('tokenizeNotation', () => {
  it('interleaves literal text with symbol runs, in source order', () => {
    expect(shape('{T}: Add {G}.')).toEqual(['symbol:T', 'text:: Add ', 'symbol:G', 'text:.']);
  });

  it('keeps adjacent symbols as separate tokens with no empty text between', () => {
    expect(shape('{1}{G}{G}')).toEqual(['symbol:1', 'symbol:G', 'symbol:G']);
  });

  it('is total — every character survives into exactly one token', () => {
    for (const text of ['{T}: Add {G}.', 'no braces at all', '', '{2/W} hybrids {W/P} too', '{}']) {
      const rebuilt = tokenizeNotation(text)
        .map((token) => (token.kind === 'text' ? token.text : token.raw))
        .join('');
      expect(rebuilt).toBe(text);
    }
  });

  it('recognizes hybrid and phyrexian codes as one symbol each', () => {
    const [hybrid] = tokenizeNotation('{W/U}');
    expect(hybrid).toMatchObject({ kind: 'symbol', caption: 'W/U', name: 'white or blue mana' });
    const [monoHybrid] = tokenizeNotation('{2/W}');
    expect(monoHybrid).toMatchObject({ name: 'two generic or white mana' });
    const [phyrexian] = tokenizeNotation('{G/P}');
    expect(phyrexian).toMatchObject({ name: 'phyrexian green mana' });
  });

  it('swatches a symbol from the shipped PIP card tokens, colored half winning', () => {
    expect(tokenizeNotation('{G}')[0]).toMatchObject({ swatch: PIP.G });
    // Generic, colorless and variables share the neutral swatch.
    for (const code of ['{3}', '{C}', '{X}']) {
      expect(tokenizeNotation(code)[0], code).toMatchObject({ swatch: PIP.N });
    }
    // A hybrid wears its colored half, so `{2/W}` is not an anonymous disc.
    expect(tokenizeNotation('{2/W}')[0]).toMatchObject({ swatch: PIP.W });
  });

  it('draws the tap symbol as the original procedural glyph, never a letter', () => {
    expect(tokenizeNotation('{T}')[0]).toMatchObject({ glyph: 'tap', name: 'tap' });
  });

  it('leaves an unrecognized code as its own literal run — never dropped', () => {
    expect(shape('Cost {WEIRD} here')).toEqual(['text:Cost ', 'unknown:WEIRD', 'text: here']);
    expect(shape('{}')).toEqual(['unknown:']);
    // A malformed run with no closing brace is plain text, not a symbol.
    expect(shape('{G')).toEqual(['text:{G']);
  });

  it('is stateless across calls (the shared regex never carries an index)', () => {
    expect(shape('{G}{G}')).toEqual(shape('{G}{G}'));
    expect(tokenizeNotation('{W}')).toHaveLength(1);
    expect(tokenizeNotation('{W}')).toHaveLength(1);
  });
});

describe('symbolNotationText — the pure-text substitution', () => {
  it('speaks every recognized symbol and leaves everything else alone', () => {
    expect(symbolNotationText('{T}: Add {G}.')).toBe('tap: Add green mana.');
    // Adjacent symbols get a separator the source string does not carry.
    expect(symbolNotationText('{1}{G}')).toBe('one generic mana green mana');
    expect(symbolNotationText('no symbols here')).toBe('no symbols here');
  });

  it('prints an unrecognized code exactly as the server wrote it', () => {
    expect(symbolNotationText('pay {WEIRD}')).toBe('pay {WEIRD}');
  });
});

describe('hasSymbolNotation', () => {
  it('is true only when a drawable symbol is present', () => {
    expect(hasSymbolNotation('{T}: Add {G}.')).toBe(true);
    expect(hasSymbolNotation('Declare attackers')).toBe(false);
    expect(hasSymbolNotation('{NOPE}')).toBe(false);
  });
});
