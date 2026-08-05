/**
 * The deck builder, as its own screen — under live design (#705).
 *
 * The topbar is its navigation, as every screen's is: the way back to the lobby, who this device
 * is, and the same gear. The right column is the card you are looking at and the deck as a thing
 * that is kept. The main region is split: the pool above, the options bar between, and the deck
 * below (`DeckArea`).
 *
 * **The bar changes only the reading.** How much of each card is shown, and what the columns are
 * cut by, are this device's view of a draft the server has not been told about yet.
 *
 * **A deck comes from this device or from a file, and goes back the same two ways** (`DeckFiles`,
 * `deck-store.ts`, `dck.ts`). What a file named and this catalog does not hold is said plainly
 * rather than dropped, because a deck that came back short without saying so is the worse bug.
 */
import { useMemo, useState } from 'react'

import {
  CARD_KINDS,
  deckStats,
  entryColumns,
  poolCards,
  type CardKind,
  type DeckSort,
  type DeckView,
} from './../../builder'
import { catalogFace } from './../../card-face'
import {
  collect,
  copiesOf,
  deckSize,
  moved,
  sideEntries,
  withCard,
  withCardIn,
  withCommander,
  withoutCardIn,
  type Catalog,
  type DeckDraft,
} from './../../deck'
import { draftOf, deleteDeck, saveDeck, savedDecks, type SavedDeck } from './../../deck-store'
import type { StarterDeck } from './../../decks'
import { formatDck, parseDck, resolveDeck } from './../../dck'
import { Card } from './../card/Card'
import { CardSearch } from './CardSearch'
import { DeckArea, type DeckPile, type Pick } from './DeckArea'
import { DeckStats } from './DeckStats'
import { LoadDeck, SaveDeck } from './DeckFiles'

const VIEWS: readonly { id: DeckView; label: string }[] = [
  { id: 'full', label: 'Full cards' },
  { id: 'stacked', label: 'Stacked' },
  { id: 'titles', label: 'Titles' },
]

const SORTS: readonly { id: DeckSort; label: string }[] = [
  { id: 'cost', label: 'Mana cost' },
  { id: 'color', label: 'Color' },
  { id: 'type', label: 'Card type' },
]

/** The two piles that are not the deck. Neither chosen is the deck on its own. */
const PILES: readonly { id: Exclude<DeckPile, undefined>; label: string }[] = [
  { id: 'commander', label: 'Commander' },
  { id: 'side', label: 'Sideboard' },
]

/** Absent in a browser with storage disabled, which is a normal way to run. */
const deviceStorage = (): Storage | undefined => {
  try {
    return globalThis.localStorage
  } catch {
    return undefined
  }
}

/** One segmented control: the options in a set, and which of them is on. */
function Options<T extends string>({
  label,
  options,
  current,
  onPick,
}: {
  label: string
  options: readonly { id: T; label: string }[]
  current: T
  onPick(id: T): void
}) {
  return (
    <span className="builder-opt">
      <span className="builder-opt-label">{label}</span>
      <span className="seg" role="radiogroup" aria-label={label}>
        {options.map((option) => (
          <button
            key={option.id}
            role="radio"
            aria-checked={current === option.id}
            className={`seg-btn${current === option.id ? ' seg-on' : ''}`}
            onClick={() => onPick(option.id)}
          >
            {option.label}
          </button>
        ))}
      </span>
    </span>
  )
}

export function DeckBuilder({
  draft,
  catalog,
  name,
  server,
  back,
  onDraft,
  onBack,
  onSettings,
}: {
  draft: DeckDraft
  catalog: Catalog
  onDraft(draft: DeckDraft): void
  /** What this device connected as, and where — the same chip the lobby carries. */
  name: string
  server: string
  /** What the way out says, because it goes back to wherever this was opened from. */
  back?: string
  onBack(): void
  onSettings(): void
}) {
  const [deckName, setDeckName] = useState('')
  const [view, setView] = useState<DeckView>('stacked')
  const [sort, setSort] = useState<DeckSort>('cost')
  const [text, setText] = useState('')
  const [sideOpen, setSideOpen] = useState(true)
  // What the pointer is over, drawn whole in the sidebar — the board's gesture, unchanged (§6.6).
  const [looking, setLooking] = useState<string | undefined>(undefined)
  // What was clicked: one copy, in one place. A deck holding four of a land holds four cards,
  // and clicking the third lights the third rather than all four.
  const [picked, setPicked] = useState<Pick | undefined>(undefined)
  const [pile, setPile] = useState<DeckPile>(undefined)
  const [loadOpen, setLoadOpen] = useState(false)
  const [saveOpen, setSaveOpen] = useState(false)
  const [saved, setSaved] = useState<readonly SavedDeck[]>(() => savedDecks(deviceStorage()))
  // The names a file asked for that this catalog does not hold.
  const [missing, setMissing] = useState<readonly string[]>([])

  const [kinds, setKinds] = useState<readonly CardKind[]>(CARD_KINDS)

  // The commander is drawn in its own pile, so it is not also a card in the deck's columns —
  // it is one card in two places or it is neither.
  const columns = useMemo(
    () =>
      entryColumns(
        draft.entries.filter((entry) => entry.identity !== draft.commander),
        catalog,
        sort,
      ),
    [draft, catalog, sort],
  )
  const side = useMemo(
    () => entryColumns(sideEntries(draft), catalog, sort),
    [draft, catalog, sort],
  )
  const pool = useMemo(() => poolCards(catalog, { text, kinds }), [catalog, text, kinds])
  // The deck as it would be submitted — the commander is one of its cards, wherever it is drawn.
  const stats = useMemo(() => deckStats(draft.entries, catalog), [draft.entries, catalog])

  // The pointer decides what is drawn whole; with the pointer off the cards, the pick does — a
  // card you chose is the one you are working on, and an empty pane while you reach for a button
  // would be the screen forgetting it.
  const preview = catalog.byId.get(looking ?? picked?.identity ?? '')
  const held = catalog.byId.get(picked?.identity ?? '')
  const inSide =
    held !== undefined && sideEntries(draft).some((e) => e.identity === held.functional_id)

  const pick = (next: Pick) => setPicked((current) => (current?.at === next.at ? undefined : next))

  /**
   * A card into one of the three piles. The commander is a designation over the deck list rather
   * than a list of its own, so moving a card there puts it in the deck as well — a designation
   * pointing at a card the deck does not hold is the one state this model must not reach.
   */
  const move = (identity: string, to: 'commander' | 'side' | 'main') => {
    const beside = sideEntries(draft).some((entry) => entry.identity === identity)
    const inDeck = copiesOf(draft, identity) > 0
    // The pick is one copy in one place, and that copy is about to not be there.
    setPicked(undefined)

    if (to === 'side') {
      if (beside) return
      onDraft(inDeck ? moved(draft, identity, 'main') : withCardIn(draft, identity, 'side'))
      return
    }

    const held = beside
      ? moved(draft, identity, 'side')
      : inDeck
        ? draft
        : withCardIn(draft, identity, 'main')
    onDraft(to === 'commander' ? withCommander(held, identity) : held)
  }

  const toggle = (kind: CardKind) =>
    setKinds((current) =>
      current.includes(kind) ? current.filter((entry) => entry !== kind) : [...current, kind],
    )

  const took = (next: DeckDraft, from: string, gone: readonly string[]) => {
    onDraft(next)
    setDeckName(from)
    setMissing(gone)
    setLoadOpen(false)
  }

  const fromFile = (file: string, contents: string) => {
    const { draft: loaded, missing: gone } = resolveDeck(parseDck(contents), catalog)
    took(loaded, file, gone)
  }

  const fromStarter = (deck: StarterDeck) =>
    took(collect(deck.cards, deck.commander), deck.name, [])

  const download = () => {
    const blob = new Blob([formatDck(draft, catalog)], { type: 'text/plain' })
    const url = URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = url
    link.download = `${deckName.trim()}.dck`
    link.click()
    URL.revokeObjectURL(url)
    setSaveOpen(false)
  }

  return (
    <div className={`builder${sideOpen ? '' : ' side-hidden'}`}>
      <div className="topbar builder-topbar">
        <button className="view-btn" onClick={onBack}>
          {back ?? '← Lobby'}
        </button>
        <span className="builder-title">Deck Editor</span>
        <span className="topbar-fill" />
        <span className="lobby-who">
          <b>{name}</b>
          <span className="lobby-server">{server}</span>
        </span>
        <button className="settings-btn" title="Settings" onClick={onSettings}>
          ⚙
        </button>
        <button
          className="menu-btn"
          title="The card and the deck file"
          aria-expanded={sideOpen}
          onClick={() => setSideOpen(!sideOpen)}
        >
          ☰
        </button>
      </div>

      <main className="builder-main" aria-label="Deck builder">
        <section className="builder-top">
          <CardSearch
            text={text}
            kinds={kinds}
            onText={setText}
            onToggle={toggle}
            onMore={() => undefined}
          />
          <div className="builder-pool" role="region" aria-label="Cards">
            {pool.map((card) => (
              <div
                className={`pool-card${picked?.at === `pool|${card.functional_id}` ? ' is-picked' : ''}`}
                key={card.functional_id}
                title="Double-click to put a copy in the deck"
                onMouseEnter={() => setLooking(card.functional_id)}
                onMouseLeave={() => setLooking(undefined)}
                onClick={() =>
                  pick({ identity: card.functional_id, at: `pool|${card.functional_id}` })
                }
                onDoubleClick={() => onDraft(withCard(draft, card.functional_id))}
              >
                <Card face={catalogFace(card)} />
              </div>
            ))}
            {pool.length === 0 && <div className="zone-empty">No card matches that.</div>}
          </div>
        </section>

        <div className="builder-bar">
          <Options label="View" options={VIEWS} current={view} onPick={(id) => setView(id)} />
          <Options label="Columns" options={SORTS} current={sort} onPick={(id) => setSort(id)} />
          <span className="builder-opt">
            <span className="builder-opt-label">Beside</span>
            <span className="seg" role="radiogroup" aria-label="Beside">
              {PILES.map((option) => (
                <button
                  key={option.label}
                  role="radio"
                  aria-checked={pile === option.id}
                  className={`seg-btn${pile === option.id ? ' seg-on' : ''}`}
                  onClick={() => setPile(pile === option.id ? undefined : option.id)}
                >
                  {option.label}
                </button>
              ))}
            </span>
          </span>
          <span className="topbar-fill" />
          <span className="builder-tally">
            {deckSize(draft)} in the deck
            {sideEntries(draft).length > 0 &&
              ` · ${sideEntries(draft).reduce((n, e) => n + e.count, 0)} beside it`}
          </span>
        </div>

        <DeckArea
          columns={columns}
          side={side}
          {...(draft.commander === undefined
            ? {}
            : { commander: catalog.byId.get(draft.commander) })}
          view={view}
          {...(picked === undefined ? {} : { picked })}
          pile={pile}
          onPick={pick}
          onLook={setLooking}
          onRemove={(identity, from) => onDraft(withoutCardIn(draft, identity, from))}
          onMove={move}
        />
      </main>

      <aside className="builder-side" aria-label="Deck">
        <div className="preview-section">{preview && <Card face={catalogFace(preview)} />}</div>

        {held && (
          <div className="builder-held">
            <span className="files-name">{held.name}</span>
            <span className="files-note">
              {copiesOf(draft, held.functional_id)} in the deck
              {inSide && ' · beside it'}
            </span>
            <div className="builder-acts">
              <button
                className="view-btn"
                onClick={() => onDraft(withCardIn(draft, held.functional_id, 'main'))}
              >
                + Deck
              </button>
              <button
                className="view-btn"
                onClick={() => onDraft(withoutCardIn(draft, held.functional_id, 'main'))}
              >
                − Deck
              </button>
            </div>
            <div className="builder-acts">
              <button
                className="view-btn"
                onClick={() => onDraft(moved(draft, held.functional_id, inSide ? 'side' : 'main'))}
              >
                {inSide ? 'To deck' : 'To sideboard'}
              </button>
              <button
                className={`view-btn${draft.commander === held.functional_id ? ' view-on' : ''}`}
                onClick={() =>
                  onDraft(
                    withCommander(
                      draft,
                      draft.commander === held.functional_id ? undefined : held.functional_id,
                    ),
                  )
                }
              >
                Commander
              </button>
            </div>
          </div>
        )}

        <div className="builder-file">
          <input
            className="connect-input builder-name"
            aria-label="Deck name"
            value={deckName}
            onChange={(event) => setDeckName(event.target.value)}
            placeholder="Untitled deck"
          />
          <div className="builder-acts">
            <button className="view-btn" onClick={() => setLoadOpen(true)}>
              Load…
            </button>
            <button className="view-btn" onClick={() => setSaveOpen(true)}>
              Save…
            </button>
          </div>
        </div>

        <DeckStats stats={stats} />

        {missing.length > 0 && (
          <div className="builder-missing" role="status">
            <span className="files-label">Not in this catalog</span>
            <p>{missing.join(', ')}</p>
            <button className="view-btn" onClick={() => setMissing([])}>
              Dismiss
            </button>
          </div>
        )}
      </aside>

      {loadOpen && (
        <LoadDeck
          saved={saved}
          onSaved={(deck) => took(draftOf(deck), deck.name, [])}
          onStarter={fromStarter}
          onFile={fromFile}
          onDelete={(gone) => setSaved(deleteDeck(deviceStorage(), gone))}
          onClose={() => setLoadOpen(false)}
        />
      )}

      {saveOpen && (
        <SaveDeck
          name={deckName}
          onName={setDeckName}
          onDevice={() => {
            setSaved(saveDeck(deviceStorage(), deckName.trim(), draft))
            setSaveOpen(false)
          }}
          onFile={download}
          onClose={() => setSaveOpen(false)}
        />
      )}
    </div>
  )
}
