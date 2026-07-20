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
    expect(parsed.version).toBe(1);
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
