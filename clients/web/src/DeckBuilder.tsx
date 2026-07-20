/**
 * The deck builder (issue #368): construct and submit a legal deck from the full
 * wire-carried card pool (the #367 {@link CatalogView}), not just the two bundled
 * starters. A pre-game surface reachable from the room seat panel — a modal over the
 * lobby — that browses every supported card, adds/removes copies, shows running
 * counts and the room format's advertised deck rules, and submits the built list
 * through the existing `submit_deck` gate.
 *
 * Commander designation (issue #396): when the open format advertises
 * {@link CatalogFormat.requires_commander} (issue #394), the running deck list gains a
 * per-row designate control and names the chosen commander distinctly; the designation
 * is changeable/clearable before submit, round-trips through saved/exported decks, and
 * is carried into `submit_deck`. Eligibility (legendary creature, color identity) stays
 * SERVER-validated — the builder only HINTS likely candidates from the catalog type
 * line and never enforces legality (zero client legality — AGENTS.md).
 *
 * Saved decks (issue #369, ADR 0027): a built deck can be named and saved to the
 * player's device (IndexedDB, `deck/savedDeckStore.ts`), listed on return, loaded
 * back for editing, deleted, and exported/imported as a portable schema-versioned
 * JSON document. Saving never implies legality — a saved deck is validated only at
 * submission time by the room format through the UNCHANGED `submit_deck` gate, so a
 * deck saved under one format may be rejected by another without corrupting the
 * saved copy. Storage is device-local and never leaves the device until submitted;
 * when storage is unavailable the panel simply hides and the bundled-starters flow
 * still works (graceful degradation). Overwriting and deleting always require
 * explicit intent (a confirm affordance) — no silent data loss.
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
import { useEffect, useMemo, useState } from 'react';
import { STARTER_DECKLISTS, decklistCounts } from './decklists';
import { CardInspect } from './table/CardInspect';
import type { CardView, CardIdentity, CatalogCard, CatalogFormat, CatalogView } from './protocol';
import {
  cardsToCounts,
  countsToCards,
  deleteSavedDeck,
  listSavedDecks,
  normalizeDeckName,
  saveDeck,
  savedDeckExists,
  type SavedDeck,
} from './deck/savedDeckStore';
import { DeckDocumentError, parseDeck, serializeDeck } from './deck/deckDocument';
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
   * The commander to seed the designation with on open (issue #396) — e.g. a
   * commander-format starter's own designation, so the builder doubles as an editing
   * base. Ignored unless {@link CatalogFormat.requires_commander} is advertised for the
   * open format. Ephemeral; the player may change or clear it before submit.
   */
  initialCommander?: CardIdentity;
  /**
   * Submit the built list (functional ids, duplicates repeated) through `submit_deck`.
   * The `commander` designation (issue #396) is carried only when the open format
   * advertises {@link CatalogFormat.requires_commander}; the server validates it (zero
   * client legality — AGENTS.md).
   */
  onSubmit: (cards: CardIdentity[], commander?: CardIdentity) => void;
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

/** Total copies across saved card rows (display only). */
function savedDeckSize(deck: SavedDeck): number {
  return deck.cards.reduce((sum, card) => sum + card.count, 0);
}

/**
 * Best-effort file download of an exported deck document. Device-local only — this
 * writes to the player's own machine, never to the project or another player. Where
 * the DOM download path is unavailable or blocked (jsdom, locked-down browsers) it
 * fails silently: the on-screen export text still lets the player copy the deck.
 */
function downloadDeck(name: string, text: string): void {
  try {
    const blob = new Blob([text], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `${name.trim() || 'deck'}.rune-deck.json`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  } catch {
    // No usable download path — the visible export text is the fallback.
  }
}

/**
 * The saved-decks panel (issue #369, ADR 0027): name and save the working deck to
 * the device, list/load/delete saved decks, and export/import the portable JSON
 * document. Device-local only; this never sends anything to the server or another
 * player. Overwriting and deleting demand explicit confirmation (no silent data
 * loss). When device storage is unavailable the whole panel hides so the builder
 * still works from bundled starters (graceful degradation).
 */
function SavedDecksPanel({
  counts,
  commander,
  onLoad,
}: {
  counts: Record<CardIdentity, number>;
  /** The working deck's commander designation to save alongside its cards (issue #396). */
  commander: CardIdentity | undefined;
  /** Load a saved/imported deck back into the builder — its counts and its designation. */
  onLoad: (counts: Record<CardIdentity, number>, commander: CardIdentity | undefined) => void;
}) {
  // `null` until the storage probe resolves; `false` means device storage is
  // unavailable and the panel hides. Ephemeral UI state — never load-bearing.
  const [storageOk, setStorageOk] = useState<boolean | null>(null);
  const [decks, setDecks] = useState<SavedDeck[]>([]);
  const [name, setName] = useState('');
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Explicit-intent gates: a save over an existing name, and a pending delete.
  const [confirmOverwrite, setConfirmOverwrite] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  // The last export document text, shown for copy alongside the file download.
  const [exportText, setExportText] = useState<string | null>(null);
  // The import textarea contents (paste an exported deck document here).
  const [importText, setImportText] = useState('');

  // Probe device storage once on open by reading the saved list. A rejection means
  // storage is unavailable (private mode, disabled) — hide the panel, never crash.
  useEffect(() => {
    let cancelled = false;
    listSavedDecks().then(
      (list) => {
        if (!cancelled) {
          setDecks(list);
          setStorageOk(true);
        }
      },
      () => {
        if (!cancelled) setStorageOk(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);

  const refresh = async (): Promise<void> => {
    try {
      setDecks(await listSavedDecks());
    } catch {
      // A refresh failure leaves the last-known list; the op's own catch reports it.
    }
  };

  const persist = async (deckName: string): Promise<void> => {
    // Carry the commander designation into the saved copy so a commander deck
    // round-trips (issue #396); `undefined` saves the pre-commander shape.
    await saveDeck({ name: deckName, cards: countsToCards(counts), commander });
    setConfirmOverwrite(false);
    setStatus(`Saved “${deckName}”.`);
    setError(null);
    await refresh();
  };

  const save = async (allowOverwrite: boolean): Promise<void> => {
    const trimmed = normalizeDeckName(name);
    if (trimmed === '') {
      setError('Name your deck before saving.');
      setStatus(null);
      return;
    }
    if (countsToCards(counts).length === 0) {
      setError('Add cards to the deck before saving.');
      setStatus(null);
      return;
    }
    try {
      // Explicit intent before overwriting an existing deck (no silent data loss).
      if (!allowOverwrite && (await savedDeckExists(trimmed))) {
        setConfirmOverwrite(true);
        setStatus(null);
        setError(null);
        return;
      }
      await persist(trimmed);
    } catch {
      setError('Couldn’t save on this device.');
    }
  };

  const load = (deck: SavedDeck): void => {
    onLoad(cardsToCounts(deck.cards), deck.commander);
    setName(deck.name);
    setConfirmOverwrite(false);
    setStatus(`Loaded “${deck.name}” — edit it, then save.`);
    setError(null);
  };

  const remove = async (deckName: string): Promise<void> => {
    try {
      await deleteSavedDeck(deckName);
      setConfirmDelete(null);
      setStatus(`Deleted “${deckName}”.`);
      setError(null);
      await refresh();
    } catch {
      setError('Couldn’t delete that deck.');
    }
  };

  const exportDeck = (deck: SavedDeck): void => {
    const text = serializeDeck(deck);
    setExportText(text);
    setStatus(`Exported “${deck.name}”.`);
    setError(null);
    downloadDeck(deck.name, text);
  };

  // Import loads the document into the builder as the working deck; it is NOT
  // auto-saved, so importing over an existing name can never silently overwrite it —
  // the player reviews and saves explicitly (which re-runs the overwrite gate).
  const importDeck = (): void => {
    let contents;
    try {
      contents = parseDeck(importText);
    } catch (err) {
      setError(err instanceof DeckDocumentError ? err.message : 'Couldn’t read that deck.');
      setStatus(null);
      return;
    }
    onLoad(cardsToCounts(contents.cards), contents.commander);
    setName(contents.name);
    setImportText('');
    setConfirmOverwrite(false);
    setStatus(`Imported “${contents.name}” — review it, then save.`);
    setError(null);
  };

  // Storage unavailable (or still probing after a failure): render nothing so the
  // builder degrades cleanly to the bundled-starters experience.
  if (storageOk === false) return null;

  return (
    <section className={l.builderSaved} aria-label="Saved decks" data-testid="deck-builder-saved">
      <div className={l.builderSavedSave} role="group" aria-label="Save this deck">
        <span className={s.fieldLabel}>Save deck as</span>
        <input
          className={cx(s.input, l.builderSavedName)}
          type="text"
          autoComplete="off"
          spellCheck={false}
          maxLength={60}
          placeholder="Deck name"
          value={name}
          onChange={(event) => {
            setName(event.target.value);
            setConfirmOverwrite(false);
          }}
          data-testid="deck-builder-deck-name"
          aria-label="Saved deck name"
        />
        {confirmOverwrite ? (
          <>
            <span className={s.muted} data-testid="deck-builder-overwrite-prompt">
              Overwrite existing deck?
            </span>
            <button
              type="button"
              className={cx(s.button, s.buttonPrimary)}
              onClick={() => void save(true)}
              data-testid="deck-builder-overwrite-confirm"
            >
              Overwrite
            </button>
            <button
              type="button"
              className={s.button}
              onClick={() => setConfirmOverwrite(false)}
              data-testid="deck-builder-overwrite-cancel"
            >
              Cancel
            </button>
          </>
        ) : (
          <button
            type="button"
            className={s.button}
            onClick={() => void save(false)}
            data-testid="deck-builder-save"
          >
            Save
          </button>
        )}
      </div>

      {decks.length > 0 && (
        <ul
          className={l.builderSavedList}
          data-testid="deck-builder-saved-list"
          aria-label="Your saved decks"
        >
          {decks.map((deck) => (
            <li
              key={deck.name}
              className={l.builderSavedRow}
              data-testid={`deck-builder-saved-row-${deck.name}`}
            >
              <span className={l.builderSavedRowName}>{deck.name}</span>
              <span className={s.muted}>{savedDeckSize(deck)} cards</span>
              <span className={l.builderSavedRowActions}>
                <button
                  type="button"
                  className={s.button}
                  onClick={() => load(deck)}
                  data-testid={`deck-builder-load-${deck.name}`}
                  aria-label={`Load ${deck.name}`}
                >
                  Load
                </button>
                <button
                  type="button"
                  className={s.button}
                  onClick={() => exportDeck(deck)}
                  data-testid={`deck-builder-export-${deck.name}`}
                  aria-label={`Export ${deck.name}`}
                >
                  Export
                </button>
                {confirmDelete === deck.name ? (
                  <>
                    <button
                      type="button"
                      className={s.button}
                      onClick={() => void remove(deck.name)}
                      data-testid={`deck-builder-delete-confirm-${deck.name}`}
                      aria-label={`Confirm delete ${deck.name}`}
                    >
                      Delete?
                    </button>
                    <button
                      type="button"
                      className={s.button}
                      onClick={() => setConfirmDelete(null)}
                      data-testid={`deck-builder-delete-cancel-${deck.name}`}
                      aria-label={`Keep ${deck.name}`}
                    >
                      Keep
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    className={s.button}
                    onClick={() => setConfirmDelete(deck.name)}
                    data-testid={`deck-builder-delete-${deck.name}`}
                    aria-label={`Delete ${deck.name}`}
                  >
                    Delete
                  </button>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}

      <details className={l.builderImport}>
        <summary className={s.fieldLabel}>Import / export a deck file</summary>
        <textarea
          className={cx(s.input, l.builderImportText)}
          rows={3}
          placeholder="Paste an exported deck document here"
          value={importText}
          onChange={(event) => setImportText(event.target.value)}
          data-testid="deck-builder-import-text"
          aria-label="Deck document to import"
        />
        <button
          type="button"
          className={s.button}
          onClick={importDeck}
          data-testid="deck-builder-import"
        >
          Import deck
        </button>
        {exportText !== null && (
          <textarea
            className={cx(s.input, l.builderImportText)}
            rows={4}
            readOnly
            value={exportText}
            data-testid="deck-builder-export-output"
            aria-label="Exported deck document"
          />
        )}
      </details>

      {status !== null && (
        <span className={s.muted} role="status" data-testid="deck-builder-saved-status">
          {status}
        </span>
      )}
      {error !== null && (
        <span className={s.errorText} role="alert" data-testid="deck-builder-saved-error">
          {error}
        </span>
      )}
    </section>
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
  // Whether the open format advertises a commander requirement (issue #394/#396): the
  // sole gate for the designation affordance below. Nothing here computes legality — a
  // format that does not advertise it shows no designation surface, and the server
  // remains the only authority on whether a designation is legal.
  const requiresCommander = format?.requires_commander === true;

  // The in-progress deck: identity → copies. Ephemeral local UI state seeded once on
  // open; never load-bearing across messages (the pool it references is server truth).
  const [counts, setCounts] = useState<Record<CardIdentity, number>>(() => ({ ...initialCounts }));
  // The designated commander (issue #396), or `null` for none. Ephemeral like `counts`;
  // seeded from the open deck's designation and changeable/clearable before submit.
  const [commander, setCommander] = useState<CardIdentity | null>(initialCommander ?? null);
  // The card being pin-inspected, if any — ephemeral selection, discarded on close.
  const [inspecting, setInspecting] = useState<CatalogCard | null>(null);

  const cards = useMemo(() => catalog?.cards ?? [], [catalog]);
  const total = totalCount(counts);

  // Resolve chosen counts into display rows in the catalog's stable order, then any
  // seeded identity the catalog does not carry (fallback to its raw id as the name),
  // so a starter-seeded card never silently vanishes from the summary. Each row carries
  // its catalog `type_line` so the designation affordance can HINT likely commander
  // candidates (a legendary type line) — a display hint only, never an enforced gate.
  const deckRows = useMemo(() => {
    const rows: { id: CardIdentity; name: string; count: number; typeLine: string }[] = [];
    const seen = new Set<CardIdentity>();
    for (const card of cards) {
      const count = counts[card.functional_id] ?? 0;
      if (count > 0) {
        rows.push({ id: card.functional_id, name: card.name, count, typeLine: card.type_line });
        seen.add(card.functional_id);
      }
    }
    for (const [id, count] of Object.entries(counts)) {
      if (count > 0 && !seen.has(id)) rows.push({ id, name: id, count, typeLine: '' });
    }
    return rows;
  }, [cards, counts]);

  // Keep the designation honest: if the designated card is no longer in the deck (its
  // last copy was removed, the deck was cleared, or a different list was loaded that
  // drops it), clear the designation so a stale commander can never be submitted. The
  // server still validates; this is only UI hygiene, not a legality check.
  useEffect(() => {
    if (commander !== null && !deckRows.some((row) => row.id === commander)) {
      setCommander(null);
    }
  }, [commander, deckRows]);

  // The designated commander's display row, if it is still in the deck.
  const commanderRow =
    commander !== null ? deckRows.find((row) => row.id === commander) : undefined;

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
    // Carry the designation only for a commander format (issue #396); otherwise the
    // frame stays the pre-commander shape. Legality (eligibility, identity) is the
    // server's — the client never checks it.
    onSubmit(list, requiresCommander ? (commander ?? undefined) : undefined);
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

          {/* Device-local saved decks (#369, ADR 0027): save/load/delete + portable
              export/import. Carries the commander designation (#396) so a commander deck
              round-trips. Hides itself when device storage is unavailable. */}
          <SavedDecksPanel
            counts={counts}
            commander={commander ?? undefined}
            onLoad={(nextCounts, nextCommander) => {
              setCounts(nextCounts);
              setCommander(nextCommander ?? null);
            }}
          />

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

                {/* The commander designation (issue #396), shown only when the format
                    advertises the requirement. The designated card is named here
                    distinctly; below, each deck row offers a one-press designate control
                    and the designated row is marked. Keyboard + touch: all native
                    ≥44px buttons, nothing hover- or drag-only. */}
                {requiresCommander && (
                  <div
                    className={l.builderCommander}
                    data-testid="deck-builder-commander"
                    aria-label="Commander"
                  >
                    <span className={s.fieldLabel}>Commander</span>
                    {commanderRow !== undefined ? (
                      <span className={l.builderCommanderChosen}>
                        <span
                          className={l.builderCommanderName}
                          data-testid="deck-builder-commander-name"
                        >
                          {commanderRow.name}
                        </span>
                        <button
                          type="button"
                          className={s.button}
                          onClick={() => setCommander(null)}
                          data-testid="deck-builder-commander-clear"
                          aria-label="Clear commander designation"
                        >
                          Clear
                        </button>
                      </span>
                    ) : (
                      <span className={s.muted} data-testid="deck-builder-commander-empty">
                        Designate a card below as your commander.
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
                    {deckRows.map((row) => {
                      const isCommander = row.id === commander;
                      // A display-only hint: a legendary type line is the usual commander
                      // (CR 903.3). This never gates designation — any row can be picked;
                      // the server rejects an ineligible one.
                      const candidate = /legendary/i.test(row.typeLine);
                      return (
                        <li
                          key={row.id}
                          className={cx(l.builderDeckRow, isCommander && l.builderDeckRowCommander)}
                          data-testid={`deck-builder-deck-row-${row.id}`}
                        >
                          <span className={l.builderCardCount}>{row.count}×</span>
                          <span className={l.builderDeckRowName}>{row.name}</span>
                          {requiresCommander &&
                            (isCommander ? (
                              <span
                                className={l.builderCommanderBadge}
                                data-testid={`deck-builder-commander-badge-${row.id}`}
                              >
                                Commander
                              </span>
                            ) : (
                              <button
                                type="button"
                                className={cx(s.button, candidate && l.builderCommanderCandidate)}
                                onClick={() => setCommander(row.id)}
                                data-testid={`deck-builder-designate-${row.id}`}
                                aria-label={`Designate ${row.name} as commander`}
                              >
                                {candidate ? 'Commander?' : 'Set commander'}
                              </button>
                            ))}
                        </li>
                      );
                    })}
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
