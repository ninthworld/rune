/**
 * Decks as files: the `.dck` list a player already has on disk, read and written.
 *
 * ```
 * [Main]
 * 4 Lightning Bolt|LEA
 *
 * [Sideboard]
 * 3 Tormod's Crypt|DRK
 *
 * [Commander]
 * 1 Krenko, Mob Boss|JMP
 * ```
 *
 * **A file names cards; this client addresses identities.** The two are joined here, by the name
 * the catalog stated, and a name the catalog does not hold is reported rather than dropped — a
 * deck that quietly came back four cards short is worse than one that says which four.
 *
 * The set code after the bar is read and kept out of the matching: SAGE's catalog carries no
 * printings (`CatalogCard` has no set), so a file that names one is describing something this
 * client cannot see, and matching on it would fail every line. It is written back out only where
 * the file that came in had one.
 *
 * **This is a reading of a list, not of the rules.** Nothing here checks a count against a
 * format, and the commander section is carried across as the designation the file made.
 */
import { collect, sideEntries, type Catalog, type DeckDraft } from './deck'

/** One line of a deck file: how many, of what, from where. */
export interface DeckFileLine {
  count: number
  name: string
  /** The set code printed after the bar, where the file gave one. */
  set?: string
}

/** A deck file's three lists, in the file's own words. */
export interface DeckFile {
  main: readonly DeckFileLine[]
  side: readonly DeckFileLine[]
  commander: readonly DeckFileLine[]
}

/** What came back from a file, and what could not be found in the catalog. */
export interface LoadedDeck {
  draft: DeckDraft
  /** The names the catalog does not hold, in the order the file listed them. */
  missing: readonly string[]
}

const SECTIONS: Record<string, keyof DeckFile> = {
  main: 'main',
  maindeck: 'main',
  deck: 'main',
  sideboard: 'side',
  side: 'side',
  commander: 'commander',
}

/** `4 Lightning Bolt|LEA` — a count, a name, and optionally where it was printed. */
const LINE = /^(\d+)\s+(.+?)\s*(?:\|\s*([^|]*))?$/

/**
 * Read a deck file. Tolerant on purpose: unknown section headers and unreadable lines are
 * skipped rather than failing the whole file, and lines before any header are the main deck,
 * because a bare list of cards is a deck.
 */
export function parseDck(text: string): DeckFile {
  const out: Record<keyof DeckFile, DeckFileLine[]> = { main: [], side: [], commander: [] }
  let into: keyof DeckFile = 'main'

  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim()
    if (line === '' || line.startsWith('//') || line.startsWith('#')) continue

    const header = /^\[(.+)]$/.exec(line)
    if (header) {
      into = SECTIONS[header[1]!.trim().toLowerCase().replace(/\s+/g, '')] ?? into
      continue
    }

    const match = LINE.exec(line)
    if (!match) continue
    const set = match[3]?.trim()
    out[into].push({
      count: Number(match[1]),
      name: match[2]!,
      ...(set === undefined || set === '' ? {} : { set }),
    })
  }

  return out
}

/** The catalog keyed by the name a file would call a card, lowercased. */
const byName = (catalog: Catalog): Map<string, string> => {
  const index = new Map<string, string>()
  for (const card of catalog.cards) {
    const key = card.name.toLowerCase()
    if (!index.has(key)) index.set(key, card.functional_id)
  }
  return index
}

/**
 * A parsed file against a catalog: the draft it describes, and every name that is not in it.
 *
 * A commander line puts the card in the deck as well as designating it, because a deck file
 * lists the commander in its own section and the deck it is submitted as still holds it.
 */
export function resolveDeck(file: DeckFile, catalog: Catalog): LoadedDeck {
  const index = byName(catalog)
  const missing: string[] = []

  const take = (lines: readonly DeckFileLine[]): string[] => {
    const cards: string[] = []
    for (const line of lines) {
      const identity = index.get(line.name.toLowerCase())
      if (identity === undefined) {
        if (!missing.includes(line.name)) missing.push(line.name)
        continue
      }
      for (let copy = 0; copy < Math.max(1, line.count); copy += 1) cards.push(identity)
    }
    return cards
  }

  const commanders = take(file.commander)
  const draft = collect([...commanders, ...take(file.main)])
  const side = collect(take(file.side))

  return {
    draft: {
      entries: draft.entries,
      ...(side.entries.length > 0 ? { side: side.entries } : {}),
      ...(commanders[0] === undefined ? {} : { commander: commanders[0] }),
    },
    missing,
  }
}

/**
 * Write a draft back out. Names come from the catalog, and no set code is written, because this
 * client was never told one — a bar with nothing after it would be a printing it made up.
 */
export function formatDck(draft: DeckDraft, catalog: Catalog): string {
  const nameOf = (identity: string) => catalog.byId.get(identity)?.name ?? identity
  const lines = (entries: readonly { identity: string; count: number }[]) =>
    entries.map((entry) => `${entry.count} ${nameOf(entry.identity)}`)

  const commander = draft.commander
  // The commander has its own section, so it is not also listed among the main deck.
  const main = draft.entries.filter((entry) => entry.identity !== commander)
  const out = ['[Main]', ...lines(main)]

  const side = sideEntries(draft)
  if (side.length > 0) out.push('', '[Sideboard]', ...lines(side))
  if (commander !== undefined) out.push('', '[Commander]', `1 ${nameOf(commander)}`)

  return `${out.join('\n')}\n`
}
