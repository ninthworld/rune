/**
 * Portable deck export/import document (issue #369, ADR 0027).
 *
 * The one cross-device / sharing mechanism for saved decks: a small,
 * schema-versioned JSON document a player can export to a file and import on
 * another device. It carries only a name and a list of `functional_id` + count —
 * no card logic, no Oracle text, no legality (the server's `submit_deck` gate is
 * the sole authority on whether a deck is legal). Importing is the inverse of
 * exporting: a round-trip yields an equivalent deck.
 *
 * The `schema` tag and integer `version` let a future format evolve without
 * silently mis-reading an old file. Parsing is strict and total: any malformed or
 * unrecognised input throws {@link DeckDocumentError} so the import UI can report a
 * clear, non-corrupting failure rather than persisting garbage.
 */
import type { DeckCard, DeckContents } from './savedDeckStore';

/** The document's schema identifier — distinguishes a RUNE deck from other JSON. */
export const DECK_SCHEMA = 'rune.deck';

/** The current document schema version. Bump on any incompatible shape change. */
export const DECK_SCHEMA_VERSION = 1;

/** The on-disk shape of an exported deck (what `serializeDeck` emits). */
export interface DeckDocument {
  /** Always {@link DECK_SCHEMA}; guards against importing unrelated JSON. */
  readonly schema: typeof DECK_SCHEMA;
  /** The document schema version ({@link DECK_SCHEMA_VERSION}). */
  readonly version: number;
  /** The deck's player-chosen name. */
  readonly name: string;
  /** The deck's card rows (functional_id + count). */
  readonly cards: readonly DeckCard[];
}

/** A strict-parse failure: malformed JSON, wrong schema, or an invalid shape. */
export class DeckDocumentError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'DeckDocumentError';
  }
}

/** Build the portable document from a deck's contents (drops storage metadata). */
export function toDeckDocument(contents: DeckContents): DeckDocument {
  return {
    schema: DECK_SCHEMA,
    version: DECK_SCHEMA_VERSION,
    name: contents.name,
    cards: contents.cards.map((card) => ({
      functional_id: card.functional_id,
      count: card.count,
    })),
  };
}

/** Serialize a deck's contents to a pretty-printed export document string. */
export function serializeDeck(contents: DeckContents): string {
  return JSON.stringify(toDeckDocument(contents), null, 2);
}

/** Whether a value is a plain, non-null object (not an array). */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Validate and normalize one card row, throwing on any malformed field. */
function parseCard(value: unknown, index: number): DeckCard {
  if (!isRecord(value)) {
    throw new DeckDocumentError(`Card ${index} is not an object.`);
  }
  const functionalId = value.functional_id;
  const count = value.count;
  if (typeof functionalId !== 'string' || functionalId.length === 0) {
    throw new DeckDocumentError(`Card ${index} is missing a functional_id.`);
  }
  if (typeof count !== 'number' || !Number.isInteger(count) || count <= 0) {
    throw new DeckDocumentError(`Card ${index} has an invalid count.`);
  }
  return { functional_id: functionalId, count };
}

/**
 * Parse an export document string back into deck contents, strictly. Throws
 * {@link DeckDocumentError} on malformed JSON, a wrong/absent schema tag, an
 * unsupported version, or an invalid card list — never returns a partial deck.
 * The result is plain (name, cards); the caller saves it (which stamps the time)
 * so an import round-trips to an equivalent saved deck.
 */
export function parseDeck(text: string): DeckContents {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    throw new DeckDocumentError('That file is not valid JSON.');
  }
  if (!isRecord(raw)) {
    throw new DeckDocumentError('That file is not a deck document.');
  }
  if (raw.schema !== DECK_SCHEMA) {
    throw new DeckDocumentError('That file is not a RUNE deck document.');
  }
  if (raw.version !== DECK_SCHEMA_VERSION) {
    throw new DeckDocumentError(`Unsupported deck document version: ${String(raw.version)}.`);
  }
  if (typeof raw.name !== 'string' || raw.name.trim().length === 0) {
    throw new DeckDocumentError('The deck document has no name.');
  }
  if (!Array.isArray(raw.cards)) {
    throw new DeckDocumentError('The deck document has no card list.');
  }
  const cards = raw.cards.map((card, index) => parseCard(card, index));
  return { name: raw.name.trim(), cards };
}
