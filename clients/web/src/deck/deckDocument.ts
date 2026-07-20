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
 *
 * Version history:
 * - `1` — name + card rows.
 * - `2` — adds an optional `commander` designation (issue #396): a commander-format
 *   deck names one of its own cards as its commander (CR 903.3), by the same
 *   `functional_id` a row uses. A v1 document (no commander) still loads unchanged —
 *   the loader accepts both versions so a previously exported deck never fails to
 *   import (ADR 0027 durability contract). The commander is never validated here; the
 *   server's `submit_deck` gate remains the sole authority on legality.
 */
import type { DeckCard, DeckContents } from './savedDeckStore';

/** The document's schema identifier — distinguishes a RUNE deck from other JSON. */
export const DECK_SCHEMA = 'rune.deck';

/** The current document schema version. Bump on any incompatible shape change. */
export const DECK_SCHEMA_VERSION = 2;

/**
 * Every document version this loader can read. The current version ({@link
 * DECK_SCHEMA_VERSION}) is written on export; older listed versions still import so a
 * previously exported deck never fails (ADR 0027). A version 1 document simply carries
 * no commander designation.
 */
const SUPPORTED_VERSIONS: ReadonlySet<number> = new Set([1, 2]);

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
  /**
   * The card this deck designates as its commander (CR 903.3, issue #396), by the same
   * `functional_id` one of its rows uses. Present only for a commander-format deck
   * (written at schema version ≥ 2) and omitted otherwise, so a non-commander export
   * keeps the pre-commander shape. Never validated here — the server's `submit_deck`
   * gate owns legality.
   */
  readonly commander?: string;
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
  const document: DeckDocument = {
    schema: DECK_SCHEMA,
    version: DECK_SCHEMA_VERSION,
    name: contents.name,
    cards: contents.cards.map((card) => ({
      functional_id: card.functional_id,
      count: card.count,
    })),
  };
  // Emit `commander` only when the deck designates one, so a non-commander export
  // stays byte-identical to the pre-commander shape (minus the version bump).
  return contents.commander !== undefined
    ? { ...document, commander: contents.commander }
    : document;
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
 * Accepts every {@link SUPPORTED_VERSIONS} version: a legacy v1 document (no
 * commander) loads unchanged, a v2 document carries an optional commander (issue
 * #396). The result is plain (name, cards, optional commander); the caller saves it
 * (which stamps the time) so an import round-trips to an equivalent saved deck.
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
  if (typeof raw.version !== 'number' || !SUPPORTED_VERSIONS.has(raw.version)) {
    throw new DeckDocumentError(`Unsupported deck document version: ${String(raw.version)}.`);
  }
  if (typeof raw.name !== 'string' || raw.name.trim().length === 0) {
    throw new DeckDocumentError('The deck document has no name.');
  }
  if (!Array.isArray(raw.cards)) {
    throw new DeckDocumentError('The deck document has no card list.');
  }
  const cards = raw.cards.map((card, index) => parseCard(card, index));
  // The commander designation is optional (absent in every v1 document, and in a v2
  // deck that names none). When present it must be a non-empty id; legality is the
  // server's concern, so we validate only the shape here.
  const commander = parseCommander(raw.commander);
  return commander !== undefined
    ? { name: raw.name.trim(), cards, commander }
    : { name: raw.name.trim(), cards };
}

/** Validate an optional commander field: `undefined`/`null` → none, else a non-empty id. */
function parseCommander(value: unknown): string | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== 'string' || value.length === 0) {
    throw new DeckDocumentError('The deck document has an invalid commander.');
  }
  return value;
}
