/**
 * Decks: the destination the builder lives in.
 *
 * `docs/client-design.md` §9.7 settles two things and defers the rest. Settled: the builder is a
 * destination in the shell, reachable **without being at a table**, and the deck it holds is an
 * *input* to `submit_deck` rather than a substitute for it — the verdict stays the server's
 * `LobbyRejection`, and nothing here computes legality. Deferred: the builder itself, which is
 * the densest surface in the product and has its own design pass coming.
 *
 * So this is the route and nothing more. It knows the builder needs a catalog and a draft, and
 * it knows what to do when there is no catalog yet; it knows nothing about how a deck is built,
 * which is what lets that surface be replaced without touching the shell around it.
 */
import type { Catalog, DeckDraft } from './../../deck'
import type { CatalogFormat } from './../../protocol'
import { DeckBuilder } from './DeckBuilder'

export function Decks({
  catalog,
  format,
  draft,
  onAdd,
  onRemove,
  onCommander,
  onInspect,
  onDone,
}: {
  catalog: Catalog
  format?: CatalogFormat
  draft: DeckDraft
  onAdd(identity: string): void
  onRemove(identity: string): void
  onCommander(identity: string | undefined): void
  onInspect(identity: string): void
  onDone(): void
}) {
  if (catalog.cards.length === 0) {
    return (
      <div className="page">
        <header className="page__head">
          <h1>Decks</h1>
        </header>
        <p className="page__pending">
          Waiting for the server’s card list. A deck is built from the cards it publishes.
        </p>
      </div>
    )
  }

  return (
    <DeckBuilder
      catalog={catalog}
      format={format}
      draft={draft}
      onAdd={onAdd}
      onRemove={onRemove}
      onCommander={onCommander}
      onInspect={onInspect}
      onClose={onDone}
    />
  )
}
