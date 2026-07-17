import { describe, expect, it } from 'vitest';
import { Container, Graphics, Text } from 'pixi.js';
import {
  buildCardDisplay,
  buildChipDisplay,
  cardVisualSignature,
  parseManaCost,
  type CardDisplayData,
} from './cardFactory';
import { BADGE, PALETTE, PIP, PT_TEXT, SURFACES } from '../tokens';

/** Collect every `Text` node in a display object, depth first. */
function collectText(node: Container): Text[] {
  const found: Text[] = [];
  const walk = (n: Container): void => {
    for (const child of n.children) {
      if (child instanceof Text) found.push(child);
      if (child instanceof Container) walk(child);
    }
  };
  walk(node);
  return found;
}

/** Count every `Graphics` node in a display object, depth first. */
function countGraphics(node: Container): number {
  let count = 0;
  const walk = (n: Container): void => {
    for (const child of n.children) {
      if (child instanceof Graphics) count += 1;
      if (child instanceof Container) walk(child);
    }
  };
  walk(node);
  return count;
}

const texts = (node: Container): string[] => collectText(node).map((t) => t.text);

/** Names/type lines may be truncated to fit small cards, so match on a prefix. */
function findByPrefix(node: Container, prefix: string): Text | undefined {
  const head = prefix.slice(0, 6);
  return collectText(node).find((t) => t.text.startsWith(head));
}

/** Pixi normalizes style colors to lowercase; compare tokens the same way. */
const fill = (t: Text | undefined): string | undefined =>
  typeof t?.style.fill === 'string' ? t.style.fill.toLowerCase() : undefined;
const token = (hex: string): string => hex.toLowerCase();

/** A representative vanilla creature. */
const grizzlyBears: CardDisplayData = {
  name: 'Grizzly Bears',
  typeLine: 'Creature — Bear',
  colorIdentity: 'G',
  manaCost: '{1}{G}',
  power: '2',
  toughness: '2',
};

/** A representative land (no cost, no P/T). */
const forest: CardDisplayData = {
  name: 'Forest',
  typeLine: 'Basic Land — Forest',
  colorIdentity: 'L',
};

/** A creature carrying counters, tapped and summoning sick. */
const pridemate: CardDisplayData = {
  name: 'Pridemate',
  typeLine: 'Creature — Cat Soldier',
  colorIdentity: 'W',
  manaCost: '{1}{W}',
  power: '4',
  toughness: '4',
  counters: [{ kind: '+1/+1', count: 2 }],
  tapped: true,
  summoningSick: true,
  selected: true,
};

describe('parseManaCost', () => {
  it('splits a cost string into pips with token swatches', () => {
    expect(parseManaCost('{1}{G}')).toEqual([
      { symbol: '1', bg: PIP.N.bg, fg: PIP.N.fg },
      { symbol: 'G', bg: PIP.G.bg, fg: PIP.G.fg },
    ]);
  });

  it('falls back to the neutral swatch for colorless/generic symbols', () => {
    expect(parseManaCost('{C}')).toEqual([{ symbol: 'C', bg: PIP.N.bg, fg: PIP.N.fg }]);
  });

  it('returns nothing for an empty cost', () => {
    expect(parseManaCost('')).toEqual([]);
  });
});

describe('buildCardDisplay', () => {
  it('builds a vanilla creature with name, type line and P/T from tokens', () => {
    const card = buildCardDisplay(grizzlyBears, 'hand');
    expect(card).toBeInstanceOf(Container);

    const nameNode = findByPrefix(card, 'Grizzly Bears');
    expect(nameNode?.text).toBe('Grizzly Bears');
    expect(fill(nameNode)).toBe(token(SURFACES.nameText));

    expect(findByPrefix(card, 'Creature — Bear')?.text).toBe('Creature — Bear');

    const ptNode = collectText(card).find((t) => t.text === '2/2');
    expect(fill(ptNode)).toBe(token(PT_TEXT.G));
  });

  it('renders P/T exactly as provided — never summing counters into it', () => {
    // Base P/T 4/4 with two +1/+1 counters. The server sends effective values;
    // the factory must NOT compute 6/6.
    const labels = texts(buildCardDisplay(pridemate));
    expect(labels).toContain('4/4');
    expect(labels).not.toContain('6/6');
    // The counter is surfaced as its own chip, verbatim.
    expect(labels).toContain('+1/+1 ×2');
  });

  it('renders a land with no cost pips and no P/T', () => {
    const card = buildCardDisplay(forest);
    expect(findByPrefix(card, 'Forest')?.text).toBe('Forest');
    expect(findByPrefix(card, 'Basic Land')).toBeDefined();
    // No P/T pill and no hybrid pips means no slash anywhere on a land.
    expect(texts(card).some((l) => l.includes('/'))).toBe(false);
  });

  it('draws mana pips using the token swatch color per symbol', () => {
    const greenPip = collectText(buildCardDisplay(grizzlyBears)).find((t) => t.text === 'G');
    expect(fill(greenPip)).toBe(token(PIP.G.fg));
  });

  it('surfaces a counter chip in the counter token color', () => {
    const chip = collectText(buildCardDisplay(pridemate)).find((t) => t.text === '+1/+1 ×2');
    expect(fill(chip)).toBe(token(BADGE.counterText));
  });

  it('rotates a tapped card by a quarter turn and dims it', () => {
    const inner = buildCardDisplay(pridemate).children[0] as Container;
    expect(inner.rotation).toBeCloseTo(Math.PI / 2);
    expect(inner.alpha).toBeLessThan(1);
  });

  it('dims an ineligible target during targeting mode', () => {
    // A plain (untapped, unsick) card dims well below full opacity when it is not
    // a legal target — the ADR 0009 "everything else dimmed" state.
    const inner = buildCardDisplay({ ...grizzlyBears, dimmed: true }).children[0] as Container;
    expect(inner.alpha).toBeLessThan(0.5);
    // A highlighted candidate is NOT dimmed: it stays fully opaque.
    const lit = buildCardDisplay({ ...grizzlyBears, targeting: true }).children[0] as Container;
    expect(lit.alpha).toBe(1);
  });

  it('draws an always-on playable edge only when the card is actionable (issue #277)', () => {
    const inert = countGraphics(buildCardDisplay(grizzlyBears).children[0] as Container);
    const playable = countGraphics(
      buildCardDisplay({ ...grizzlyBears, actionable: true }).children[0] as Container,
    );
    // The playable card carries exactly one extra Graphics — the bottom edge bar —
    // over its otherwise-identical inert twin.
    expect(playable).toBe(inert + 1);
  });

  it('keeps the actionable state in the visual signature so the reconciler rebuilds', () => {
    expect(cardVisualSignature({ ...grizzlyBears, actionable: true })).not.toBe(
      cardVisualSignature(grizzlyBears),
    );
  });

  it('accepts the color identity from the caller (no derivation)', () => {
    // Same card data framed by a different identity yields that identity's P/T
    // token color — proving the factory reads what it is handed.
    const red = buildCardDisplay({ ...grizzlyBears, colorIdentity: 'R' });
    const ptNode = collectText(red).find((t) => t.text === '2/2');
    expect(fill(ptNode)).toBe(token(PT_TEXT.R));
  });

  it('builds a card for every color identity and tier', () => {
    const tiers = ['support', 'field', 'hand'] as const;
    for (const id of Object.keys(PALETTE) as Array<keyof typeof PALETTE>) {
      for (const tier of tiers) {
        expect(buildCardDisplay({ ...forest, colorIdentity: id }, tier)).toBeInstanceOf(Container);
      }
    }
  });

  it('draws an ×N stack badge only when the render stands for more than one (issue #318)', () => {
    expect(texts(buildCardDisplay(grizzlyBears))).not.toContain('×3');
    expect(texts(buildCardDisplay({ ...grizzlyBears, stackCount: 3 }))).toContain('×3');
  });

  it('keeps stack count and land glyph in the visual signature (issue #318)', () => {
    expect(cardVisualSignature(forest)).not.toBe(cardVisualSignature({ ...forest, stackCount: 4 }));
    expect(cardVisualSignature(forest, 'chip')).not.toBe(
      cardVisualSignature({ ...forest, landGlyph: 'land-forest' }, 'chip'),
    );
  });
});

describe('buildChipDisplay (issue #318)', () => {
  it('draws a basic land as a glyph chip — no name text', () => {
    const chip = buildChipDisplay({ ...forest, landGlyph: 'land-forest' });
    expect(chip).toBeInstanceOf(Container);
    // A glyph chip renders no card name; identity is carried by the glyph geometry.
    expect(texts(chip)).not.toContain('Forest');
    expect(countGraphics(chip)).toBeGreaterThan(1);
  });

  it('draws a nonbasic land as a named chip', () => {
    const chip = buildChipDisplay({
      name: 'Windswept Heath',
      typeLine: 'Land',
      colorIdentity: 'L',
    });
    expect(texts(chip).some((t) => t.startsWith('Wind'))).toBe(true);
  });

  it('dims a tapped chip without rotating it (chip tier has no room to rotate)', () => {
    const inner = buildChipDisplay({ ...forest, landGlyph: 'land-forest', tapped: true })
      .children[0] as Container;
    expect(inner.rotation).toBe(0);
    expect(inner.alpha).toBeLessThan(1);
  });

  it('shows the ×N badge on a stacked chip', () => {
    const chip = buildChipDisplay({ ...forest, landGlyph: 'land-forest', stackCount: 3 });
    expect(texts(chip)).toContain('×3');
  });

  it('never draws a keyword strip on a chip (chip tier stays minimal, issue #320)', () => {
    const plain = countGraphics(buildChipDisplay({ ...forest, landGlyph: 'land-forest' }));
    const withKw = countGraphics(
      buildChipDisplay({ ...forest, landGlyph: 'land-forest', keywords: ['flying', 'trample'] }),
    );
    expect(withKw).toBe(plain);
  });
});

describe('card-face information budget (issue #320)', () => {
  const flyer: CardDisplayData = {
    ...grizzlyBears,
    keywords: ['flying', 'deathtouch'],
  };

  it('draws one keyword glyph per server-supplied keyword at field tier', () => {
    // Each glyph is one extra Graphics over the same card with no keywords.
    const plain = countGraphics(buildCardDisplay(grizzlyBears, 'field'));
    const withKw = countGraphics(buildCardDisplay(flyer, 'field'));
    expect(withKw).toBe(plain + 2);
  });

  it('renders keyword glyphs at support, field, and hand tiers alike', () => {
    for (const tier of ['support', 'field', 'hand'] as const) {
      const plain = countGraphics(buildCardDisplay(grizzlyBears, tier));
      const withKw = countGraphics(buildCardDisplay(flyer, tier));
      expect(withKw).toBeGreaterThan(plain);
    }
  });

  it('caps the strip and overflows to +N rather than shrinking below legibility', () => {
    const many: CardDisplayData = {
      ...grizzlyBears,
      keywords: ['flying', 'first_strike', 'deathtouch', 'trample', 'vigilance', 'lifelink'],
    };
    // Field tier fits four glyphs; six keywords overflow to a "+N" tag.
    expect(texts(buildCardDisplay(many, 'field')).some((t) => /^\+\d+$/.test(t))).toBe(true);
  });

  it('drops a keyword with no glyph rather than leaving a gap', () => {
    const plain = countGraphics(buildCardDisplay(grizzlyBears, 'field'));
    const unknown = countGraphics(
      buildCardDisplay({ ...grizzlyBears, keywords: ['not_a_real_keyword'] }, 'field'),
    );
    expect(unknown).toBe(plain);
  });

  it('draws a latent activated-ability marker distinct from the gold playable bar', () => {
    const plain = countGraphics(buildCardDisplay(grizzlyBears, 'field').children[0] as Container);
    const marked = countGraphics(
      buildCardDisplay({ ...grizzlyBears, hasActivatedAbility: true }, 'field')
        .children[0] as Container,
    );
    // Exactly one extra Graphics — the marker dot.
    expect(marked).toBe(plain + 1);
  });

  it('shows the ability marker and the gold bar together (latent + live)', () => {
    const plain = countGraphics(buildCardDisplay(grizzlyBears, 'field').children[0] as Container);
    const both = countGraphics(
      buildCardDisplay({ ...grizzlyBears, hasActivatedAbility: true, actionable: true }, 'field')
        .children[0] as Container,
    );
    // The dot and the edge bar are two separate extra Graphics.
    expect(both).toBe(plain + 2);
  });

  it('renders a marked-damage badge from view data (issue #320)', () => {
    expect(texts(buildCardDisplay(grizzlyBears, 'field'))).not.toContain('3 dmg');
    expect(texts(buildCardDisplay({ ...grizzlyBears, markedDamage: 3 }, 'field'))).toContain(
      '3 dmg',
    );
  });

  it('keeps keywords, the ability marker, and damage in the visual signature', () => {
    expect(cardVisualSignature(grizzlyBears)).not.toBe(cardVisualSignature(flyer));
    expect(cardVisualSignature(grizzlyBears)).not.toBe(
      cardVisualSignature({ ...grizzlyBears, hasActivatedAbility: true }),
    );
    expect(cardVisualSignature(grizzlyBears)).not.toBe(
      cardVisualSignature({ ...grizzlyBears, markedDamage: 2 }),
    );
  });
});
