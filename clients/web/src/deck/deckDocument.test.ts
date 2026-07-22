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
    expect(parsed.version).toBe(DECK_SCHEMA_VERSION);
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
    name: 'Jedit',
    cards: [
      { functional_id: 'jedit_ojanen', count: 1 },
      { functional_id: 'forest', count: 99 },
    ],
    commander: 'jedit_ojanen',
  };

  it('emits schema v2 and carries the designation when one is set', () => {
    const doc = toDeckDocument(COMMANDER_DECK);
    expect(doc.version).toBe(DECK_SCHEMA_VERSION);
    expect(DECK_SCHEMA_VERSION).toBe(2);
    expect(doc.commander).toBe('jedit_ojanen');
  });

  it('omits the commander field for a non-commander deck', () => {
    const doc = toDeckDocument({ name: 'Burn', cards: [{ functional_id: 'shock', count: 4 }] });
    expect('commander' in doc).toBe(false);
  });

  it('round-trips a commander deck back to equivalent contents', () => {
    expect(parseDeck(serializeDeck(COMMANDER_DECK))).toEqual(COMMANDER_DECK);
  });

  it('loads a legacy v1 document (no commander) without stranding it', () => {
    // A file exported before the bump: version 1, no commander field. It still imports,
    // yielding contents with no designation.
    const v1 = JSON.stringify({
      schema: DECK_SCHEMA,
      version: 1,
      name: 'Old Deck',
      cards: [{ functional_id: 'shock', count: 4 }],
    });
    const parsed = parseDeck(v1);
    expect(parsed.name).toBe('Old Deck');
    expect(parsed.commander).toBeUndefined();
    expect('commander' in parsed).toBe(false);
  });

  it('loads a v2 document that designates no commander', () => {
    const v2 = JSON.stringify({
      schema: DECK_SCHEMA,
      version: 2,
      name: 'No General',
      cards: [{ functional_id: 'shock', count: 4 }],
    });
    expect(parseDeck(v2).commander).toBeUndefined();
  });

  it('rejects an invalid commander field', () => {
    const bad = JSON.stringify({
      schema: DECK_SCHEMA,
      version: 2,
      name: 'x',
      cards: [{ functional_id: 'shock', count: 1 }],
      commander: '',
    });
    expect(() => parseDeck(bad)).toThrow(/invalid commander/);
  });

  it('still rejects an unsupported (future) version', () => {
    expect(() =>
      parseDeck(JSON.stringify({ schema: DECK_SCHEMA, version: 3, name: 'x', cards: [] })),
    ).toThrow(/Unsupported/);
  });
});
