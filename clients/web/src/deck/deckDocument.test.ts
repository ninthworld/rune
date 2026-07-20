import { describe, expect, it } from 'vitest';
import {
  DECK_SCHEMA,
  DECK_SCHEMA_VERSION,
  DeckDocumentError,
  parseDeck,
  serializeDeck,
  toDeckDocument,
} from './deckDocument';
import type { DeckContents } from './savedDeckStore';

const DECK: DeckContents = {
  name: 'Mono-Red Burn',
  cards: [
    { functional_id: 'shock', count: 4 },
    { functional_id: 'mountain', count: 20 },
  ],
};

describe('deckDocument (ADR 0027 export/import)', () => {
  it('tags the exported document with the schema and version', () => {
    const doc = toDeckDocument(DECK);
    expect(doc.schema).toBe(DECK_SCHEMA);
    expect(doc.version).toBe(DECK_SCHEMA_VERSION);
    expect(doc.name).toBe('Mono-Red Burn');
    expect(doc.cards).toEqual(DECK.cards);
  });

  it('serializes to a versioned JSON string', () => {
    const parsed = JSON.parse(serializeDeck(DECK));
    expect(parsed.schema).toBe('rune.deck');
    expect(parsed.version).toBe(2);
    expect(parsed.name).toBe('Mono-Red Burn');
  });

  it('round-trips an exported deck back to equivalent contents', () => {
    expect(parseDeck(serializeDeck(DECK))).toEqual(DECK);
  });

  it('drops storage metadata from the portable document', () => {
    // A SavedDeck carries updatedAt; the document must not, so it stays portable.
    const withMeta = { ...DECK, updatedAt: 999 };
    expect('updatedAt' in toDeckDocument(withMeta)).toBe(false);
  });

  it('rejects malformed JSON', () => {
    expect(() => parseDeck('{not json')).toThrow(DeckDocumentError);
  });

  it('rejects a document with the wrong schema tag', () => {
    expect(() =>
      parseDeck(JSON.stringify({ schema: 'other', version: 1, name: 'x', cards: [] })),
    ).toThrow(/not a RUNE deck/);
  });

  it('rejects an unsupported version', () => {
    expect(() =>
      parseDeck(JSON.stringify({ schema: DECK_SCHEMA, version: 999, name: 'x', cards: [] })),
    ).toThrow(/Unsupported/);
  });

  it('rejects a missing name', () => {
    expect(() =>
      parseDeck(JSON.stringify({ schema: DECK_SCHEMA, version: 1, name: '  ', cards: [] })),
    ).toThrow(/no name/);
  });

  it('rejects invalid card rows', () => {
    const doc = {
      schema: DECK_SCHEMA,
      version: 1,
      name: 'x',
      cards: [{ functional_id: 'shock', count: 0 }],
    };
    expect(() => parseDeck(JSON.stringify(doc))).toThrow(/invalid count/);
  });

  it('trims the imported name', () => {
    const doc = { schema: DECK_SCHEMA, version: 1, name: '  Trimmed  ', cards: [] };
    expect(parseDeck(JSON.stringify(doc)).name).toBe('Trimmed');
  });
});

describe('deckDocument commander designation (issue #396, schema v2)', () => {
  const COMMANDER_DECK: DeckContents = {
    name: 'Arcades Walls',
    cards: [
      { functional_id: 'arcades_the_strategist', count: 1 },
      { functional_id: 'forest', count: 40 },
    ],
    commander: 'arcades_the_strategist',
  };

  it('writes the current version and carries the commander on export', () => {
    const doc = toDeckDocument(COMMANDER_DECK);
    expect(doc.version).toBe(DECK_SCHEMA_VERSION);
    expect(DECK_SCHEMA_VERSION).toBe(2);
    expect(doc.commander).toBe('arcades_the_strategist');
  });

  it('round-trips a commander deck back to equivalent contents', () => {
    expect(parseDeck(serializeDeck(COMMANDER_DECK))).toEqual(COMMANDER_DECK);
  });

  it('omits the commander field entirely for a deck that designates none', () => {
    const noCommander: DeckContents = {
      name: 'Burn',
      cards: [{ functional_id: 'shock', count: 4 }],
    };
    const doc = toDeckDocument(noCommander);
    expect('commander' in doc).toBe(false);
    // And it round-trips without acquiring a commander key.
    expect(parseDeck(serializeDeck(noCommander))).toEqual(noCommander);
  });

  it('loads a legacy v1 document (no commander) unchanged — ADR 0027 durability', () => {
    // A pre-existing export written before #396 has version 1 and no commander field.
    const legacy = JSON.stringify({
      schema: DECK_SCHEMA,
      version: 1,
      name: 'Old Deck',
      cards: [{ functional_id: 'shock', count: 4 }],
    });
    const parsed = parseDeck(legacy);
    expect(parsed).toEqual({ name: 'Old Deck', cards: [{ functional_id: 'shock', count: 4 }] });
    expect('commander' in parsed).toBe(false);
  });

  it('accepts a v2 document without a commander (a non-commander deck saved after #396)', () => {
    const doc = JSON.stringify({
      schema: DECK_SCHEMA,
      version: 2,
      name: 'Plain',
      cards: [{ functional_id: 'shock', count: 4 }],
    });
    expect('commander' in parseDeck(doc)).toBe(false);
  });

  it('rejects a malformed commander (present but not a non-empty id)', () => {
    const doc = {
      schema: DECK_SCHEMA,
      version: 2,
      name: 'x',
      cards: [{ functional_id: 'shock', count: 1 }],
      commander: '',
    };
    expect(() => parseDeck(JSON.stringify(doc))).toThrow(/invalid commander/);
  });

  it('still rejects a version beyond the supported range', () => {
    expect(() =>
      parseDeck(JSON.stringify({ schema: DECK_SCHEMA, version: 3, name: 'x', cards: [] })),
    ).toThrow(/Unsupported/);
  });
});
