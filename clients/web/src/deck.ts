/**
 * A deck being built, and the catalog it is built out of.
 *
 * The wire wants a flat list of card identities with duplicates repeated (`submit_deck`), which
 * is the one thing a person never wants to look at. A draft is held here as counted entries in
 * the order the cards were first added, and expanded back to the flat list at the moment of
 * submission — so the shape a player edits and the shape the server validates are the same deck,
 * derived one from the other rather than kept in step by hand.
 *
 * **A draft holds two lists and one designation, and only part of that reaches the server.** The
 * deck is submitted; the cards *beside* it are not, because the wire has no sideboard at all
 * (ADR 0018) — so `expand` leaves them out and every surface that shows them says they are not
 * sent. The commander is neither list but a pointer into the first: designating a card does not
 * move it, and the one state this model must never reach is a designation naming a card the deck
 * no longer holds, which is why the last copy leaving takes the designation with it. That is
 * bookkeeping over this module's own model, not a claim about whether a designation is *legal*.
 *
 * **This module computes no legality.** It counts, it searches, and it repeats the deck rules the
 * catalog advertised — nothing more. A format's rules arrive as numbers the server published
 * (`CatalogFormat`), so restating them is quoting, not deciding; and the arithmetic of "this
 * deck holds 37 cards and the format said 40" is a count next to a published number, which is
 * why it is offered as advice and never as a verdict. The verdict is the server's, and it
 * arrives as a `LobbyRejection` after a submission the client always allows.
 *
 * That boundary is why nothing here inspects a `type_line`. Whether a card is a basic land, a
 * legendary creature, or a legal commander is a rules question the server answers; a client that
 * read the words in a type line to decide one would be a second, worse rules engine — and it
 * would be wrong first about exactly the cards that matter. So the copy limit is displayed and
 * never enforced (the exemption is not the client's to apply), and the commander picker offers
 * the deck's own cards and lets the server say which of them is legal.
 */
import type { AiOption, CatalogCard, CatalogFormat, CatalogView, LobbyRejection } from './protocol'
import { list } from './normalize'
import { manaSymbols, type ManaColor } from './mana'

// ---------------------------------------------------------------------------
// The draft
// ---------------------------------------------------------------------------

/** One card in a draft, with how many copies of it the deck holds. */
export interface DeckEntry {
  identity: string
  count: number
}

/**
 * A deck under construction: counted entries, plus the commander designated over them.
 *
 * Immutable — every edit returns a new draft — so a builder can hold one in React state and a
 * caller can never mutate the deck a pending submission was composed from.
 */
export interface DeckDraft {
  entries: readonly DeckEntry[]
  /**
   * The cards kept beside the deck rather than in it.
   *
   * **Nothing on the wire carries these.** `submit_deck` is one flat list plus a commander, so a
   * sideboard is this device's own note about a deck it is building, and `expand` leaves it out.
   * It is held here rather than beside the draft because a deck file has one, and a deck that
   * lost its sideboard on the way through the builder would be the file being read wrong.
   */
  side?: readonly DeckEntry[]
  /** The `CardIdentity` designated as commander (CR 903.3), if the player has chosen one. */
  commander?: string
}

export const EMPTY_DECK: DeckDraft = { entries: [] }

/** Which of a draft's two lists an edit is about. */
export type DeckList = 'main' | 'side'

/** The cards beside the deck, as a list that is always there to read. */
export const sideEntries = (draft: DeckDraft): readonly DeckEntry[] => draft.side ?? []

/** Collapse a flat decklist into counted entries, first appearance ordering. */
export function collect(cards: readonly string[], commander?: string): DeckDraft {
  const entries: DeckEntry[] = []
  const at = new Map<string, number>()
  for (const identity of cards) {
    const index = at.get(identity)
    if (index === undefined) {
      at.set(identity, entries.length)
      entries.push({ identity, count: 1 })
    } else {
      entries[index] = { identity, count: entries[index]!.count + 1 }
    }
  }
  return commander === undefined ? { entries } : { entries, commander }
}

/** Expand a draft back to the flat list `submit_deck` carries, duplicates repeated. */
export const expand = (draft: DeckDraft): readonly string[] =>
  draft.entries.flatMap((entry) => Array.from({ length: entry.count }, () => entry.identity))

/** How many cards the deck holds in total. */
export const deckSize = (draft: DeckDraft): number =>
  draft.entries.reduce((total, entry) => total + entry.count, 0)

/** How many copies of one card the deck holds. */
export const copiesOf = (draft: DeckDraft, identity: string): number =>
  draft.entries.find((entry) => entry.identity === identity)?.count ?? 0

/** One more copy, appended if the card is not in the deck yet. */
export function withCard(draft: DeckDraft, identity: string): DeckDraft {
  const found = draft.entries.some((entry) => entry.identity === identity)
  return {
    ...draft,
    entries: found
      ? draft.entries.map((entry) =>
          entry.identity === identity ? { ...entry, count: entry.count + 1 } : entry,
        )
      : [...draft.entries, { identity, count: 1 }],
  }
}

/**
 * One fewer copy; the entry leaves when its last copy does.
 *
 * A designation is a pointer into the deck list, so removing the designated card's last copy
 * drops the designation with it. That is bookkeeping over this module's own model — the client
 * still never decides whether a designation is *legal*, only that a card the deck no longer
 * holds cannot go on being pointed at from a list it has left.
 */
export function withoutCard(draft: DeckDraft, identity: string): DeckDraft {
  const entries = draft.entries
    .map((entry) => (entry.identity === identity ? { ...entry, count: entry.count - 1 } : entry))
    .filter((entry) => entry.count > 0)
  const gone = !entries.some((entry) => entry.identity === identity)
  return gone && draft.commander === identity
    ? withCommander({ ...draft, entries }, undefined)
    : { ...draft, entries }
}

/** The same edits, against whichever of the two lists is named. */
export function withCardIn(draft: DeckDraft, identity: string, where: DeckList): DeckDraft {
  if (where === 'main') return withCard(draft, identity)
  const side = sideEntries(draft)
  const found = side.some((entry) => entry.identity === identity)
  return {
    ...draft,
    side: found
      ? side.map((entry) =>
          entry.identity === identity ? { ...entry, count: entry.count + 1 } : entry,
        )
      : [...side, { identity, count: 1 }],
  }
}

export function withoutCardIn(draft: DeckDraft, identity: string, where: DeckList): DeckDraft {
  if (where === 'main') return withoutCard(draft, identity)
  return {
    ...draft,
    side: sideEntries(draft)
      .map((entry) => (entry.identity === identity ? { ...entry, count: entry.count - 1 } : entry))
      .filter((entry) => entry.count > 0),
  }
}

/** One copy across to the other list, which is the pair of edits a player means by "move". */
export const moved = (draft: DeckDraft, identity: string, from: DeckList): DeckDraft =>
  withCardIn(withoutCardIn(draft, identity, from), identity, from === 'main' ? 'side' : 'main')

/** Designate a commander, or clear the designation with `undefined`. */
export function withCommander(draft: DeckDraft, identity: string | undefined): DeckDraft {
  if (identity !== undefined) return { ...draft, commander: identity }
  // Dropped rather than set to nothing, so a cleared designation does not travel as a key.
  const cleared = { ...draft }
  delete cleared.commander
  return cleared
}

// ---------------------------------------------------------------------------
// The catalog
// ---------------------------------------------------------------------------

/** The catalog as a builder reads it: the same lists, plus the lookups every surface wants. */
export interface Catalog {
  cards: readonly CatalogCard[]
  byId: ReadonlyMap<string, CatalogCard>
  /** Every keyword any listed card carries, sorted — a filter built from stated data only. */
  keywords: readonly string[]
  /** Every advertised format, sorted by id — see `readCatalog`. */
  formats: readonly CatalogFormat[]
  ai: readonly AiOption[]
  /** AI kind id → the display name the server advertised for it. */
  aiNames: Readonly<Record<string, string>>
}

export const EMPTY_CATALOG: Catalog = {
  cards: [],
  byId: new Map(),
  keywords: [],
  formats: [],
  ai: [],
  aiNames: {},
}

/**
 * Index one `CatalogView`. A client that has not fetched one reads an empty catalog.
 *
 * The cards keep the stable order the catalog promised. The **formats do not**: the protocol
 * promises no order for them, and a server that builds the list from a map hands out a different
 * one per process — which would mean the format a create-table form opens on changed between
 * restarts for no reason a player could see. Sorting them by id is a presentation choice, and
 * the only field there is to sort on.
 */
export function readCatalog(view: CatalogView | undefined): Catalog {
  if (!view) return EMPTY_CATALOG
  const cards = list(view.cards)
  const ai = list(view.ai_opponents)
  return {
    cards,
    byId: new Map(cards.map((card) => [card.functional_id, card])),
    keywords: [...new Set(cards.flatMap((card) => list(card.keywords)))].sort(),
    formats: [...list(view.formats)].sort((a, b) => a.game_setup.localeCompare(b.game_setup)),
    ai,
    aiNames: Object.fromEntries(ai.map((option) => [option.id, option.name])),
  }
}

/** The advertised rules for one `game_setup`, or `undefined` if the catalog lists no such id. */
export const formatOf = (catalog: Catalog, gameSetup: string): CatalogFormat | undefined =>
  catalog.formats.find((format) => format.game_setup === gameSetup)

/** A card's display name, falling back to its identity when the catalog has not arrived. */
export const cardName = (catalog: Catalog, identity: string): string =>
  catalog.byId.get(identity)?.name ?? identity

/**
 * The colours a deck is drawn as, at a table (`docs/client-design.md` §9.5).
 *
 * The shallowest possible reading, exactly as `mana.ts`'s tint is: the colours of the pips
 * printed on the cards in it, in a fixed order so two decks with the same colours draw the same
 * row. It is **not** colour identity (CR 903.4) and it decides nothing — a seat's deck is
 * checked by the server, and this only says what the row of pips shows.
 */
const COLOR_ORDER: readonly ManaColor[] = ['w', 'u', 'b', 'r', 'g']

export function deckColors(draft: DeckDraft, catalog: Catalog): readonly ManaColor[] {
  const found = new Set<ManaColor>()
  for (const entry of draft.entries) {
    for (const symbol of manaSymbols(catalog.byId.get(entry.identity)?.mana_cost)) {
      for (const color of symbol.colors) found.add(color)
    }
  }
  return COLOR_ORDER.filter((color) => found.has(color))
}

/** What a browser is filtering the card pool down to. */
export interface CardQuery {
  /** Free text, matched case-insensitively against the fields the catalog states. */
  text?: string
  /** One keyword from `Catalog.keywords`, matched exactly. */
  keyword?: string
}

/**
 * The catalog narrowed to a query, in the catalog's own order.
 *
 * Text matches name, type line, rules text, and mana cost — the strings the server sent, read as
 * strings. Matching the words in a type line is not the same as *interpreting* them: this finds
 * "Forest", it does not conclude that Forest is a land.
 */
export function findCards(catalog: Catalog, query: CardQuery): readonly CatalogCard[] {
  const text = query.text?.trim().toLowerCase()
  return catalog.cards.filter((card) => {
    if (query.keyword !== undefined && !list(card.keywords).includes(query.keyword)) return false
    if (!text) return true
    const haystack = [card.name, card.type_line, card.rules_text ?? '', card.mana_cost ?? '']
      .join(' ')
      .toLowerCase()
    return haystack.includes(text)
  })
}

// ---------------------------------------------------------------------------
// What the format published
// ---------------------------------------------------------------------------

/**
 * The format's deck rules as sentences, built from the numbers it published.
 *
 * Every line quotes a stated field. A format that published no bound says nothing about that
 * bound, rather than a sentence implying an unlimited deck has some limit — the protocol is
 * deliberate about advertising permissiveness as absence, and it stays absent here.
 */
export function deckRules(format: CatalogFormat | undefined): readonly string[] {
  if (!format) return []
  const rules: string[] = []
  if (format.min_deck_size > 0) rules.push(`At least ${format.min_deck_size} cards`)
  if (format.max_deck_size !== undefined) rules.push(`At most ${format.max_deck_size} cards`)
  if (format.max_copies !== undefined) {
    const copies = `At most ${format.max_copies} cop${format.max_copies === 1 ? 'y' : 'ies'} of a card`
    rules.push(format.basic_land_exempt ? `${copies}, basic lands exempt` : copies)
  }
  if (format.requires_commander) rules.push('A commander must be designated')
  if (format.enforce_color_identity)
    rules.push("Every card's color identity must fit the commander's")
  return rules
}

/**
 * The seats a format allows, as a phrase.
 *
 * Kept out of [`deckRules`] because a seat range is a fact about the *table*, not about the
 * deck — it belongs where a table is being made, and reading it under a decklist would suggest
 * the deck had to answer for it.
 */
export const seatRange = (format: CatalogFormat | undefined): string | undefined =>
  format === undefined
    ? undefined
    : format.min_seats === format.max_seats
      ? `${format.min_seats} seats`
      : `${format.min_seats}–${format.max_seats} seats`

/**
 * How this deck's size stands against the size the format published, or `undefined` when it is
 * between the two — advice, never a verdict.
 *
 * Deck size is the one deck rule that needs nothing but counting, which is why it is the only one
 * offered back. Copies and color identity depend on what a card *is*, and that is the server's.
 * Nothing acts on this: the submit control is offered whenever the server advertises
 * `submit_deck`, and a deck this says nothing about can still be rejected.
 */
export function sizeAdvice(
  draft: DeckDraft,
  format: CatalogFormat | undefined,
): string | undefined {
  if (!format) return undefined
  const size = deckSize(draft)
  if (size < format.min_deck_size)
    return `${format.min_deck_size - size} short of the ${format.min_deck_size}-card minimum`
  if (format.max_deck_size !== undefined && size > format.max_deck_size)
    return `${size - format.max_deck_size} above the ${format.max_deck_size}-card maximum`
  return undefined
}

/**
 * A rejection as one sentence.
 *
 * `reason` is the server's own explanation and is shown verbatim — the client composes no prose
 * about legality. The only thing added is the named card's display name, because `card` rides the
 * wire as an identity and a player reads names.
 */
export function rejectionText(rejection: LobbyRejection, catalog: Catalog): string {
  const named =
    rejection.card !== undefined && catalog.byId.has(rejection.card)
      ? ` (${cardName(catalog, rejection.card)})`
      : rejection.card !== undefined
        ? ` (${rejection.card})`
        : ''
  return `${rejection.reason}${named}`
}
