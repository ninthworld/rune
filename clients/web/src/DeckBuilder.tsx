/**
 * The deck builder (issue #368): construct and submit a legal deck from the full
 * wire-carried card pool (the #367 {@link CatalogView}), not just the two bundled
 * starters. A pre-game surface reachable from the room seat panel — a modal over the
 * lobby — that browses every supported card, adds/removes copies, shows running
 * counts and the room format's advertised deck rules, and submits the built list
 * through the existing `submit_deck` gate.
 *
 * Hard rules (AGENTS.md, ADR 0012):
 * - **Zero game logic.** Deck counts and the format-rule display are INFORMATIONAL
 *   only. The client never computes legality, cost, or effect: Submit is always
 *   offered, the only authority is the server's accept/reject of `submit_deck`, and a
 *   rejection surfaces through the lobby's existing non-blaming feedback path (the
 *   `lobbyError` toast). The builder state is preserved across a rejection for
 *   correction — nothing here is cleared by the parent on reject.
 * - **Reconstructable from server data + ephemeral UI state.** The browsable pool and
 *   the format rules come straight off the store's {@link CatalogView}; the in-progress
 *   card counts and the open inspect target are ephemeral local UI state only, never
 *   load-bearing across messages.
 * - **No card logic read here.** Every card characteristic (cost, type line, rules
 *   text, P/T, keywords) is server-computed and rendered verbatim through the shared
 *   {@link CardInspect} treatment; the builder derives nothing.
 *
 * Touch + keyboard: every control is a native ≥44px button, no action is drag- or
 * hover-only, and Escape closes the modal. Starter decks are offered as one-tap
 * seeds so a player can load one and edit it.
 */
import { useMemo, useState } from 'react';
import { STARTER_DECKLISTS, decklistCounts } from './decklists';
import { CardInspect } from './table/CardInspect';
import type { CardView, CardIdentity, CatalogCard, CatalogFormat, CatalogView } from './protocol';
import { cx } from './chrome/cx';
import s from './table/chrome.module.css';
import l from './screens.module.css';

interface DeckBuilderProps {
  /** The wire-carried card pool + format rules (#367), or `null` while it loads. */
  catalog: CatalogView | null;
  /**
   * The room's advertised format rules for the display panel, matched from the
   * catalog by the room's `game_setup`. Absent when the format is unknown to the
   * catalog (an older/newer server); the panel then omits the rules line.
   */
  format?: CatalogFormat;
  /** The counts to seed the builder with on open (a starter deck, or empty). */
  initialCounts: Readonly<Record<CardIdentity, number>>;
  /** Submit the built list (functional ids, duplicates repeated) through `submit_deck`. */
  onSubmit: (cards: CardIdentity[]) => void;
  /** Close the builder without submitting (backdrop, Cancel, or Escape). */
  onClose: () => void;
  /**
   * The lobby's non-fatal rejection message to surface over the open builder (e.g. a
   * rejected deck), or `null`. Reuses the lobby's existing non-blaming feedback path;
   * shown here so the player sees it without the modal hiding it, and the builder state
   * is preserved for correction. Never load-bearing — the builder rebuilds without it.
   */
  error?: string | null;
}

/**
 * Adapt a browse-time {@link CatalogCard} into the in-game {@link CardView} shape the
 * shared {@link CardInspect} renders. Pure field mapping — the catalog entry names a
 * card by identity, so the per-game entity `id` is stood in by the `functional_id`
 * (nothing in inspect treats it as an entity handle). No characteristic is derived.
 */
function toCardView(card: CatalogCard): CardView {
  return {
    id: card.functional_id,
    name: card.name,
    type_line: card.type_line,
    mana_cost: card.mana_cost,
    rules_text: card.rules_text,
    functional_id: card.functional_id,
    power: card.power,
    toughness: card.toughness,
    keywords: card.keywords,
  };
}

/**
 * The format's advertised deck rules as human-readable, display-only lines (issue
 * #368). Purely informational: an absent upper bound reads as "no limit" honestly
 * (the catalog carries `None`, never a sentinel), and none of this gates the client —
 * the server is the sole authority on legality.
 */
function formatRuleLines(format: CatalogFormat): string[] {
  const lines: string[] = [];
  lines.push(
    format.min_deck_size > 0 ? `Minimum ${format.min_deck_size} cards` : 'No minimum deck size',
  );
  if (format.max_deck_size !== undefined) lines.push(`Maximum ${format.max_deck_size} cards`);
  if (format.max_copies !== undefined) {
    const exempt = format.basic_land_exempt ? ' (basic lands exempt)' : '';
    lines.push(`Up to ${format.max_copies} copies of a card${exempt}`);
  } else {
    lines.push('No copy limit');
  }
  lines.push(
    format.min_seats === format.max_seats
      ? `${format.min_seats} players`
      : `${format.min_seats}–${format.max_seats} players`,
  );
  return lines;
}

/** Total copies across all counts (display only — the running deck size). */
function totalCount(counts: Record<CardIdentity, number>): number {
  let total = 0;
  for (const n of Object.values(counts)) total += n;
  return total;
}

export function DeckBuilder({
  catalog,
  format,
  initialCounts,
  onSubmit,
  onClose,
  error,
}: DeckBuilderProps) {
  // The in-progress deck: identity → copies. Ephemeral local UI state seeded once on
  // open; never load-bearing across messages (the pool it references is server truth).
  const [counts, setCounts] = useState<Record<CardIdentity, number>>(() => ({ ...initialCounts }));
  // The card being pin-inspected, if any — ephemeral selection, discarded on close.
  const [inspecting, setInspecting] = useState<CatalogCard | null>(null);

  const cards = useMemo(() => catalog?.cards ?? [], [catalog]);
  const total = totalCount(counts);

  // Resolve chosen counts into display rows in the catalog's stable order, then any
  // seeded identity the catalog does not carry (fallback to its raw id as the name),
  // so a starter-seeded card never silently vanishes from the summary.
  const deckRows = useMemo(() => {
    const rows: { id: CardIdentity; name: string; count: number }[] = [];
    const seen = new Set<CardIdentity>();
    for (const card of cards) {
      const count = counts[card.functional_id] ?? 0;
      if (count > 0) {
        rows.push({ id: card.functional_id, name: card.name, count });
        seen.add(card.functional_id);
      }
    }
    for (const [id, count] of Object.entries(counts)) {
      if (count > 0 && !seen.has(id)) rows.push({ id, name: id, count });
    }
    return rows;
  }, [cards, counts]);

  const setCount = (id: CardIdentity, next: number): void => {
    setCounts((prev) => {
      const updated = { ...prev };
      if (next <= 0) delete updated[id];
      else updated[id] = next;
      return updated;
    });
  };
  const add = (id: CardIdentity): void => setCount(id, (counts[id] ?? 0) + 1);
  const remove = (id: CardIdentity): void => setCount(id, (counts[id] ?? 0) - 1);

  const submit = (): void => {
    // Expand counts into the flat identity list the wire carries (catalog order, then
    // any leftover seeded ids). Pure data assembly — the server validates the result.
    const list: CardIdentity[] = [];
    for (const row of deckRows) {
      for (let i = 0; i < row.count; i += 1) list.push(row.id);
    }
    onSubmit(list);
  };

  return (
    <>
      <div
        data-testid="deck-builder-backdrop"
        className={s.inspectBackdrop}
        onClick={onClose}
        role="presentation"
      >
        <div
          data-testid="deck-builder"
          className={l.builderPanel}
          role="dialog"
          aria-modal="true"
          aria-label="Build a deck"
          onClick={(event) => event.stopPropagation()}
          onKeyDown={(event) => {
            if (event.key === 'Escape') onClose();
          }}
        >
          <header className={l.builderHead}>
            <h2 className={l.cardTitle}>Build a deck</h2>
            <span className={l.builderCount} data-testid="deck-builder-total">
              {total} cards
            </span>
            <button
              type="button"
              className={s.button}
              onClick={onClose}
              data-testid="deck-builder-close"
              aria-label="Close deck builder"
            >
              Close
            </button>
          </header>

          {format !== undefined && (
            <ul
              className={l.builderRules}
              data-testid="deck-builder-format"
              aria-label="Deck rules"
            >
              {formatRuleLines(format).map((line) => (
                <li key={line} className={s.seatBadge}>
                  {line}
                </li>
              ))}
            </ul>
          )}

          {/* Starter decks as one-tap seeds — a player loads one and edits it. */}
          <div className={l.builderStarters} role="group" aria-label="Start from a starter deck">
            <span className={s.fieldLabel}>Start from</span>
            {STARTER_DECKLISTS.map((deck) => (
              <button
                key={deck.id}
                type="button"
                className={s.button}
                onClick={() => setCounts(decklistCounts(deck))}
                data-testid={`deck-builder-starter-${deck.id}`}
              >
                {deck.name}
              </button>
            ))}
            <button
              type="button"
              className={s.button}
              onClick={() => setCounts({})}
              data-testid="deck-builder-clear"
            >
              Empty deck
            </button>
          </div>

          {catalog === null ? (
            <p className={s.muted} data-testid="deck-builder-loading">
              Loading the card pool…
            </p>
          ) : (
            <div className={l.builderBody}>
              {/* The browsable card pool: every supported card, add/remove copies. */}
              <ul className={l.builderPool} data-testid="deck-builder-pool" aria-label="Card pool">
                {cards.map((card) => {
                  const count = counts[card.functional_id] ?? 0;
                  return (
                    <li
                      key={card.functional_id}
                      className={l.builderCard}
                      data-testid={`deck-builder-card-${card.functional_id}`}
                    >
                      <span className={l.builderCardInfo}>
                        <span className={l.builderCardName}>{card.name}</span>
                        <span className={s.muted}>
                          {card.mana_cost !== undefined && (
                            <span data-testid={`deck-builder-cost-${card.functional_id}`}>
                              {card.mana_cost}{' '}
                            </span>
                          )}
                          {card.type_line}
                        </span>
                      </span>
                      <span className={l.builderCardControls}>
                        <button
                          type="button"
                          className={s.button}
                          onClick={() => setInspecting(card)}
                          data-testid={`deck-builder-inspect-${card.functional_id}`}
                          aria-label={`Inspect ${card.name}`}
                        >
                          Inspect
                        </button>
                        <button
                          type="button"
                          className={s.button}
                          onClick={() => remove(card.functional_id)}
                          disabled={count === 0}
                          data-testid={`deck-builder-remove-${card.functional_id}`}
                          aria-label={`Remove a copy of ${card.name}`}
                        >
                          &minus;
                        </button>
                        <span
                          className={l.builderCardCount}
                          data-testid={`deck-builder-count-${card.functional_id}`}
                          aria-label={`${count} copies of ${card.name}`}
                        >
                          {count}
                        </span>
                        <button
                          type="button"
                          className={s.button}
                          onClick={() => add(card.functional_id)}
                          data-testid={`deck-builder-add-${card.functional_id}`}
                          aria-label={`Add a copy of ${card.name}`}
                        >
                          +
                        </button>
                      </span>
                    </li>
                  );
                })}
              </ul>

              {/* The running deck: chosen cards and their copy counts (display only). */}
              <div className={l.builderDeck} data-testid="deck-builder-deck" aria-label="Your deck">
                <span className={s.fieldLabel}>Your deck · {total} cards</span>
                {deckRows.length === 0 ? (
                  <span className={s.muted} data-testid="deck-builder-deck-empty">
                    No cards yet — add from the pool or start from a starter.
                  </span>
                ) : (
                  <ul className={l.builderDeckList}>
                    {deckRows.map((row) => (
                      <li
                        key={row.id}
                        className={l.builderDeckRow}
                        data-testid={`deck-builder-deck-row-${row.id}`}
                      >
                        <span className={l.builderCardCount}>{row.count}×</span>
                        <span>{row.name}</span>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
          )}

          {error !== undefined && error !== null && error !== '' && (
            <span className={s.errorText} role="alert" data-testid="deck-builder-error">
              {error}
            </span>
          )}

          <footer className={l.builderFoot}>
            <button
              type="button"
              className={cx(s.button, s.buttonPrimary)}
              onClick={submit}
              data-testid="deck-builder-submit"
            >
              Submit deck
            </button>
            <button
              type="button"
              className={s.button}
              onClick={onClose}
              data-testid="deck-builder-cancel"
            >
              Cancel
            </button>
          </footer>
        </div>
      </div>

      {/* Rendered as a sibling of the builder backdrop (not nested) so dismissing the
          inspect popover never bubbles a click up to the builder's own backdrop and
          closes the whole builder. It reuses the shared inspect treatment verbatim. */}
      {inspecting !== null && (
        <CardInspect
          target={{ kind: 'card', card: toCardView(inspecting) }}
          onClose={() => setInspecting(null)}
        />
      )}
    </>
  );
}
