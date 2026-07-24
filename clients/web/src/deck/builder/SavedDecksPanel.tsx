/**
 * The saved-decks panel (issue #369, ADR 0027; restyled onto the 2.5D visual system
 * for #508): name and save the working deck to the device, list/load/delete saved
 * decks, and export/import the portable JSON document. Device-local only; this never
 * sends anything to the server or another player. Overwriting and deleting demand
 * explicit confirmation (no silent data loss). When device storage is unavailable the
 * whole panel hides so the builder still works from bundled starters (graceful
 * degradation).
 *
 * Split out of `DeckBuilder.tsx` (AGENTS.md file-size seam): the persistence surface
 * is a cohesive submodule with its own testids, re-exported from the builder root.
 */
import { useEffect, useState } from 'react';
import type { CardIdentity } from '../../protocol';
import {
  cardsToCounts,
  countsToCards,
  deleteSavedDeck,
  listSavedDecks,
  normalizeDeckName,
  saveDeck,
  savedDeckExists,
  type SavedDeck,
} from '../savedDeckStore';
import { DeckDocumentError, parseDeck, serializeDeck } from '../deckDocument';
import { cx } from '../../chrome/cx';
import s from '../../table/chrome.module.css';
import l from '../../screens.module.css';

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

export function SavedDecksPanel({
  counts,
  commander,
  onLoad,
}: {
  counts: Record<CardIdentity, number>;
  /** The working deck's designated commander (issue #396), saved with it; `null` for none. */
  commander: CardIdentity | null;
  /** Load a saved/imported deck's counts and its designation into the builder. */
  onLoad: (counts: Record<CardIdentity, number>, commander: CardIdentity | null) => void;
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
    await saveDeck({
      name: deckName,
      cards: countsToCards(counts),
      ...(commander !== null ? { commander } : {}),
    });
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
    onLoad(cardsToCounts(deck.cards), deck.commander ?? null);
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
    onLoad(cardsToCounts(contents.cards), contents.commander ?? null);
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
