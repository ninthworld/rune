import { describe, expect, it } from 'vitest';
import { Container, Text } from 'pixi.js';
import { buildCardDisplay, parseManaCost, type CardDisplayData } from './cardFactory';
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
});
