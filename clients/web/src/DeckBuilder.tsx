/**
 * The deck builder (issue #368; restyled onto the 2.5D visual system for #508):
 * construct and submit a legal deck from the full wire-carried card pool (the #367
 * {@link CatalogView}), not just the two bundled starters. A pre-game surface
 * reachable from the room seat panel — a modal over the lobby — that browses every
 * supported card, adds/removes copies, shows running counts and the room format's
 * advertised deck rules, and submits the built list through the existing
 * `submit_deck` gate.
 *
 * Visual system (issue #508, `docs/design/visual-system.md`): the builder speaks the
 * same language as the match. EVERY card surface — the browsable pool and the running
 * deck list — renders through the one DOM card renderer ({@link CardFace}) at an
 * appropriate tier (field for the pool, chip for the running list), and inspection
 * uses the shared {@link CardInspect} surface; no bespoke card markup remains. Panels
 * ride the scene elevation ladder, and add/remove/designate transitions animate in the
 * motion grammar's zone-travel/micro classes, snapping under reduced motion. Colors,
 * shadows, and durations flow from the scene tokens through `deckScene.ts`.
 *
 * Saved decks (issue #369, ADR 0027): a built deck can be named and saved to the
 * player's device (the {@link SavedDecksPanel}), listed on return, loaded back for
 * editing, deleted, and exported/imported as a portable schema-versioned JSON
 * document. Saving never implies legality — a saved deck is validated only at
 * submission time by the room format through the UNCHANGED `submit_deck` gate.
 *
 * Hard rules (AGENTS.md, ADR 0012):
 * - **Zero game logic.** Deck counts and the format-rule display are INFORMATIONAL
 *   only. The client never computes legality, cost, or effect: Submit is always
 *   offered, the only authority is the server's accept/reject of `submit_deck`, and a
 *   rejection surfaces through the lobby's existing non-blaming feedback path (the
 *   `lobbyError` toast), preserved over the modal for correction.
 * - **Reconstructable from server data + ephemeral UI state.** The browsable pool and
 *   the format rules come straight off the store's {@link CatalogView}; the in-progress
 *   card counts and the open inspect target are ephemeral local UI state only.
 * - **No card logic read here.** Every card characteristic is server-computed and
 *   rendered verbatim through {@link CardFace}/{@link CardInspect}; the builder derives
 *   nothing but the same frame/land-glyph display glue the table uses.
 *
 * Touch + keyboard: every control is a native ≥44px button, no action is drag- or
 * hover-only, and Escape closes the modal. Starter decks are offered as one-tap seeds.
 */
import { useEffect, useMemo, useState } from 'react';
import { STARTER_DECKLISTS, decklistCounts } from './decklists';
import { CardInspect } from './table/CardInspect';
import { CardFace } from './card/dom';
import type { CardIdentity, CatalogCard, CatalogFormat, CatalogView } from './protocol';
import {
  catalogCardToDisplayData,
  catalogCardToView,
  fallbackDisplayData,
} from './deck/catalogCard';
import { deckCardArt, deckSceneVars } from './deck/deckScene';
import { SavedDecksPanel } from './deck/builder/SavedDecksPanel';
import { useReducedMotion } from './table/hooks/useReducedMotion';
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
  /**
   * The commander to seed the designation with on open (issue #396), or absent for
   * none. Only meaningful when the format requires a commander; ignored otherwise.
   */
  initialCommander?: CardIdentity;
  /**
   * Submit the built list (functional ids, duplicates repeated) through `submit_deck`,
   * carrying the designated `commander` (issue #396) when the format requires one and a
   * card is designated. The client never computes legality — the server validates both.
   */
  onSubmit: (cards: CardIdentity[], commander?: CardIdentity) => void;
  /** Close the builder without submitting (backdrop, Cancel, or Escape). */
  onClose: () => void;
  /**
   * The lobby's non-fatal rejection message to surface over the open builder, or
   * `null`. Reuses the lobby's existing non-blaming feedback path; shown here so the
   * player sees it without the modal hiding it, and the builder state is preserved for
   * correction. Never load-bearing — the builder rebuilds without it.
   */
  error?: string | null;
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

/**
 * One browsable pool entry: the card rendered through {@link CardFace} at the field
 * tier (its whole face is the inspect affordance, so the card reads as a card), plus
 * the add/remove copy controls and the running per-card count. A card already in the
 * deck lifts to the held elevation so "in your deck" reads at a glance.
 */
function PoolCard({
  card,
  count,
  onInspect,
  onAdd,
  onRemove,
}: {
  card: CatalogCard;
  count: number;
  onInspect: () => void;
  onAdd: () => void;
  onRemove: () => void;
}) {
  const data = useMemo(() => catalogCardToDisplayData(card), [card]);
  const art = deckCardArt(data.artKey);
  return (
    <li className={l.builderCard} data-testid={`deck-builder-card-${card.functional_id}`}>
      <button
        type="button"
        className={l.builderCardFace}
        onClick={onInspect}
        data-testid={`deck-builder-inspect-${card.functional_id}`}
        aria-label={`Inspect ${card.name}`}
      >
        <CardFace data={data} tier="field" elevation={count > 0 ? 'lifted' : 'rest'} art={art} />
      </button>
      <span className={l.builderCardControls}>
        <button
          type="button"
          className={s.button}
          onClick={onRemove}
          disabled={count === 0}
          data-testid={`deck-builder-remove-${card.functional_id}`}
          aria-label={`Remove a copy of ${card.name}`}
        >
          &minus;
        </button>
        <span
          className={l.builderCardCount}
          data-testid={`deck-builder-count-${card.functional_id}`}
          data-in-deck={count > 0 ? true : undefined}
          aria-label={`${count} copies of ${card.name}`}
        >
          {count}
        </span>
        <button
          type="button"
          className={s.button}
          onClick={onAdd}
          data-testid={`deck-builder-add-${card.functional_id}`}
          aria-label={`Add a copy of ${card.name}`}
        >
          +
        </button>
      </span>
    </li>
  );
}

/** One row of the running deck: the card as a chip-tier {@link CardFace} (a pile of
 * that card), its copy count, and — in a commander format — the designation control
 * and candidate hint. The chip face keeps the list reading as handled cards, not a
 * spreadsheet. */
function DeckRow({
  id,
  name,
  count,
  card,
  requiresCommander,
  isCommander,
  isCandidate,
  onDesignate,
}: {
  id: CardIdentity;
  name: string;
  count: number;
  card: CatalogCard | undefined;
  requiresCommander: boolean;
  isCommander: boolean;
  isCandidate: boolean;
  onDesignate: () => void;
}) {
  const data = useMemo(
    () => (card ? catalogCardToDisplayData(card) : fallbackDisplayData(id, name)),
    [card, id, name],
  );
  return (
    <li
      className={cx(l.builderDeckRow, isCommander && l.builderDeckRowCommander)}
      data-testid={`deck-builder-deck-row-${id}`}
    >
      <span className={l.builderDeckRowFace} aria-hidden="true">
        <CardFace data={data} tier="chip" elevation={isCommander ? 'held' : 'rest'} />
      </span>
      <span className={l.builderCardCount}>{count}×</span>
      <span className={l.builderDeckRowName}>{name}</span>
      {isCommander && (
        <span
          className={l.builderCommanderBadge}
          data-testid={`deck-builder-commander-badge-${id}`}
        >
          Commander
        </span>
      )}
      {requiresCommander && !isCommander && isCandidate && (
        <span className={s.muted} data-testid={`deck-builder-commander-hint-${id}`}>
          Legendary
        </span>
      )}
      {requiresCommander && (
        <button
          type="button"
          className={cx(s.button, l.builderDesignate)}
          aria-pressed={isCommander}
          onClick={onDesignate}
          data-testid={`deck-builder-designate-${id}`}
          aria-label={
            isCommander
              ? `${name} is the commander — clear designation`
              : `Designate ${name} as commander`
          }
        >
          {isCommander ? 'Commander' : 'Make commander'}
        </button>
      )}
    </li>
  );
}

export function DeckBuilder({
  catalog,
  format,
  initialCounts,
  initialCommander,
  onSubmit,
  onClose,
  error,
}: DeckBuilderProps) {
  // The in-progress deck: identity → copies. Ephemeral local UI state seeded once on
  // open; never load-bearing across messages (the pool it references is server truth).
  const [counts, setCounts] = useState<Record<CardIdentity, number>>(() => ({ ...initialCounts }));
  // The designated commander (issue #396, CR 903.3), or `null` for none. Ephemeral UI
  // state seeded once on open. The client relays it verbatim and computes NO legality.
  const [commander, setCommander] = useState<CardIdentity | null>(() => initialCommander ?? null);
  // The card being pin-inspected, if any — ephemeral selection, discarded on close.
  const [inspecting, setInspecting] = useState<CatalogCard | null>(null);

  const reducedMotion = useReducedMotion();
  const cards = useMemo(() => catalog?.cards ?? [], [catalog]);
  const total = totalCount(counts);

  // Whether this room's format requires a commander is learned from the advertised
  // format metadata (issue #394), never a hardcoded format name.
  const requiresCommander = format?.requires_commander === true;

  // A display-only lookup of catalog cards by identity, used to render deck-list rows
  // through CardFace and to hint likely commander candidates — never legality.
  const cardById = useMemo(() => {
    const map: Record<CardIdentity, CatalogCard> = {};
    for (const card of cards) map[card.functional_id] = card;
    return map;
  }, [cards]);

  // Resolve chosen counts into display rows in the catalog's stable order, then any
  // seeded identity the catalog does not carry, so a starter-seeded card never
  // silently vanishes from the summary.
  const deckRows = useMemo(() => {
    // A card is *hinted* as a commander candidate when its catalog type line reads as a
    // legendary creature (CR 903.3) — a display hint only; the affordance is offered on
    // every row, and the server alone decides legality.
    const isCandidate = (id: CardIdentity): boolean => {
      const line = cardById[id]?.type_line.toLowerCase() ?? '';
      return line.includes('legendary') && line.includes('creature');
    };
    const rows: {
      id: CardIdentity;
      name: string;
      count: number;
      isCommanderCandidate: boolean;
    }[] = [];
    const seen = new Set<CardIdentity>();
    for (const card of cards) {
      const count = counts[card.functional_id] ?? 0;
      if (count > 0) {
        rows.push({
          id: card.functional_id,
          name: card.name,
          count,
          isCommanderCandidate: isCandidate(card.functional_id),
        });
        seen.add(card.functional_id);
      }
    }
    for (const [id, count] of Object.entries(counts)) {
      if (count > 0 && !seen.has(id)) {
        rows.push({ id, name: id, count, isCommanderCandidate: isCandidate(id) });
      }
    }
    // Order commander candidates first so a designation is quick to find, without
    // changing the underlying counts (presentation only).
    return requiresCommander
      ? [...rows].sort((a, b) => Number(b.isCommanderCandidate) - Number(a.isCommanderCandidate))
      : rows;
  }, [cards, counts, cardById, requiresCommander]);

  // Clear a designation whose card has left the deck (its count dropped to 0), so the
  // builder never carries a commander the list no longer holds.
  useEffect(() => {
    if (commander !== null && (counts[commander] ?? 0) === 0) setCommander(null);
  }, [commander, counts]);

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
    // Carry the designation only in a commander format and only when one is set; the
    // server validates it (and rejects a missing/illegal one) — no legality here.
    onSubmit(list, requiresCommander && commander !== null ? commander : undefined);
  };

  // The designated commander's display name for the header line, resolved from the deck
  // rows (or falling back to the raw identity). Presentation only.
  const commanderName =
    commander !== null ? (deckRows.find((row) => row.id === commander)?.name ?? commander) : null;

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
          style={deckSceneVars(reducedMotion)}
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
                <li key={line} className={l.builderRule}>
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

          {/* Device-local saved decks (#369, ADR 0027): save/load/delete + portable
              export/import. Hides itself when device storage is unavailable. */}
          <SavedDecksPanel
            counts={counts}
            commander={commander}
            onLoad={(loadedCounts, loadedCommander) => {
              setCounts(loadedCounts);
              setCommander(loadedCommander);
            }}
          />

          {catalog === null ? (
            <p className={s.muted} data-testid="deck-builder-loading">
              Loading the card pool…
            </p>
          ) : (
            <div className={l.builderBody}>
              {/* The browsable card pool: every supported card as a CardFace, add/remove. */}
              <ul className={l.builderPool} data-testid="deck-builder-pool" aria-label="Card pool">
                {cards.map((card) => (
                  <PoolCard
                    key={card.functional_id}
                    card={card}
                    count={counts[card.functional_id] ?? 0}
                    onInspect={() => setInspecting(card)}
                    onAdd={() => add(card.functional_id)}
                    onRemove={() => remove(card.functional_id)}
                  />
                ))}
              </ul>

              {/* The running deck: chosen cards as chip faces and their copy counts. */}
              <div className={l.builderDeck} data-testid="deck-builder-deck" aria-label="Your deck">
                <span className={s.fieldLabel}>Your deck · {total} cards</span>

                {/* The commander designation status (issue #396), shown only when the
                    room's format advertises the requirement (#394). Informational. */}
                {requiresCommander && (
                  <div
                    className={l.builderCommander}
                    data-testid="deck-builder-commander-status"
                    role="status"
                  >
                    {commanderName !== null ? (
                      <>
                        <span className={s.muted}>Commander</span>
                        <span
                          className={l.builderCommanderName}
                          data-testid="deck-builder-commander-name"
                        >
                          {commanderName}
                        </span>
                        <button
                          type="button"
                          className={s.button}
                          onClick={() => setCommander(null)}
                          data-testid="deck-builder-commander-clear"
                          aria-label="Clear the designated commander"
                        >
                          Clear
                        </button>
                      </>
                    ) : (
                      <span className={s.muted} data-testid="deck-builder-commander-none">
                        Designate a commander from your deck below.
                      </span>
                    )}
                  </div>
                )}

                {deckRows.length === 0 ? (
                  <span className={s.muted} data-testid="deck-builder-deck-empty">
                    No cards yet — add from the pool or start from a starter.
                  </span>
                ) : (
                  <ul className={l.builderDeckList}>
                    {deckRows.map((row) => (
                      <DeckRow
                        key={row.id}
                        id={row.id}
                        name={row.name}
                        count={row.count}
                        card={cardById[row.id]}
                        requiresCommander={requiresCommander}
                        isCommander={requiresCommander && commander === row.id}
                        isCandidate={row.isCommanderCandidate}
                        onDesignate={() => setCommander(commander === row.id ? null : row.id)}
                      />
                    ))}
                  </ul>
                )}
              </div>
            </div>
          )}

          {/* A server deck rejection surfaces as a clear, non-blaming state per the
              visual system's rejection treatment (§7): the specific reason verbatim,
              never a client-side legality gate, with the built list preserved. */}
          {error !== undefined && error !== null && error !== '' && (
            <div className={l.builderReject} role="alert" data-testid="deck-builder-error">
              <span className={l.builderRejectMark} aria-hidden="true">
                !
              </span>
              <span className={l.builderRejectText}>{error}</span>
            </div>
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
          target={{ kind: 'card', card: catalogCardToView(inspecting) }}
          onClose={() => setInspecting(null)}
        />
      )}
    </>
  );
}
