/**
 * The card-art settings overlay (ADR 0024, React DOM per ADR 0003): where the
 * player chooses which art pipeline this device uses and manages what it has
 * downloaded.
 *
 * Three sources, one choice, stored as a device preference (never game state):
 * - **Procedural frames** — the default vector faces; no images, works offline.
 * - **Bundled RUNE art** — project-owned illustrations shipped with the client,
 *   when present under `/card-art/`.
 * - **Real card images (Scryfall)** — an explicit opt-in: the player's OWN
 *   browser downloads illustrations from Scryfall and caches them only on this
 *   device. The project never ships, proxies, or redistributes them; selecting
 *   the option first shows the consent note that says exactly that.
 *
 * Everything here is presentation preference and cache management. The overlay
 * derives no rules and holds no load-bearing state: closing it (or clearing the
 * cache) leaves a UI still fully reconstructable from the next GameView.
 */
import { useEffect, useState, useSyncExternalStore } from 'react';
import {
  artStatus,
  clearDownloadedArt,
  getArtSource,
  getArtStyle,
  getArtVersion,
  setArtSource,
  setArtStyle,
  storageEstimate,
  subscribeArt,
} from '../card/art/artStore';
import type { ArtSource, ArtStyle } from '../card/art/artSettings';
import { cx } from '../chrome/cx';
import s from './chrome.module.css';

interface Props {
  /** Close the overlay (backdrop click, Escape via Table, or the close control). */
  onClose: () => void;
}

/** The selectable sources, in display order, with their user-facing copy. */
const SOURCE_OPTIONS: { source: ArtSource; label: string; description: string }[] = [
  {
    source: 'procedural',
    label: 'Procedural frames',
    description: 'The built-in vector card faces. No images, nothing to download.',
  },
  {
    source: 'bundled',
    label: 'RUNE artwork',
    description:
      'Original illustrations bundled with RUNE, drawn into the card frame when available.',
  },
  {
    source: 'scryfall',
    label: 'Real card images (Scryfall)',
    description:
      'Your browser downloads real card illustrations from Scryfall and keeps them on this device only.',
  },
];

/**
 * The two presentations for downloaded real-card images (ADR 0024): the bare
 * illustration inside RUNE's frame, or the entire official card image.
 */
const STYLE_OPTIONS: { style: ArtStyle; label: string; description: string }[] = [
  {
    style: 'window',
    label: 'Illustration in the RUNE frame',
    description: 'RUNE draws the frame and text; only the illustration is downloaded.',
  },
  {
    style: 'full',
    label: 'Entire card image',
    description:
      'The official card, frame and all. Current values (power, counters) still overlay on top.',
  },
];

/** Format a byte count for the storage line. */
function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ArtSettings({ onClose }: Props) {
  // Re-render as illustrations finish loading so the progress line stays live.
  useSyncExternalStore(subscribeArt, getArtVersion);
  const source = getArtSource();
  const style = getArtStyle();
  const status = artStatus();

  // The Scryfall consent step: choosing the option arms it; downloads only start
  // after the explicit confirmation. Ephemeral UI state.
  const [consenting, setConsenting] = useState(false);

  // Device storage estimate, refreshed whenever art availability changes.
  const [storage, setStorage] = useState<{ usage: number; quota: number } | null>(null);
  const version = getArtVersion();
  useEffect(() => {
    let cancelled = false;
    void storageEstimate().then((estimate) => {
      if (!cancelled) setStorage(estimate);
    });
    return () => {
      cancelled = true;
    };
  }, [version]);

  const pick = (next: ArtSource): void => {
    if (next === 'scryfall' && source !== 'scryfall') {
      setConsenting(true);
      return;
    }
    setConsenting(false);
    setArtSource(next);
  };

  return (
    <div
      data-testid="art-settings-backdrop"
      className={s.shortcutBackdrop}
      onClick={onClose}
      role="presentation"
    >
      <div
        data-testid="art-settings"
        className={s.shortcutPanel}
        role="dialog"
        aria-modal="true"
        aria-label="Card art settings"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 className={s.shortcutTitle}>Card art</h2>
        <div role="radiogroup" aria-label="Art source" className={s.artOptions}>
          {SOURCE_OPTIONS.map((option) => (
            <button
              key={option.source}
              type="button"
              role="radio"
              aria-checked={source === option.source}
              data-testid={`art-source-${option.source}`}
              className={cx(s.artOption, source === option.source && s.artOptionActive)}
              onClick={() => pick(option.source)}
            >
              <span className={s.artOptionLabel}>{option.label}</span>
              <span className={s.artOptionDescription}>{option.description}</span>
            </button>
          ))}
        </div>

        {consenting && source !== 'scryfall' && (
          <div className={s.artConsent} data-testid="art-consent">
            <p className={s.artNote}>
              Real card images are fetched by your browser directly from Scryfall and cached only on
              this device. RUNE never ships, uploads, or redistributes them. By default only the
              bare illustration is drawn inside RUNE&apos;s own frame; choosing “Entire card image”
              displays the full official card on your device instead. Portions of the materials are
              unofficial Fan Content permitted under the Wizards of the Coast Fan Content Policy;
              RUNE is not affiliated with or endorsed by Wizards of the Coast or Scryfall.
            </p>
            <div className={s.artActionsRow}>
              <button
                type="button"
                className={s.button}
                data-testid="art-consent-accept"
                onClick={() => {
                  setConsenting(false);
                  setArtSource('scryfall');
                }}
              >
                Enable downloads
              </button>
              <button
                type="button"
                className={s.button}
                data-testid="art-consent-cancel"
                onClick={() => setConsenting(false)}
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {source === 'scryfall' && (
          <div
            role="radiogroup"
            aria-label="Card presentation"
            className={s.artOptions}
            data-testid="art-style"
          >
            {STYLE_OPTIONS.map((option) => (
              <button
                key={option.style}
                type="button"
                role="radio"
                aria-checked={style === option.style}
                data-testid={`art-style-${option.style}`}
                className={cx(s.artOption, style === option.style && s.artOptionActive)}
                onClick={() => setArtStyle(option.style)}
              >
                <span className={s.artOptionLabel}>{option.label}</span>
                <span className={s.artOptionDescription}>{option.description}</span>
              </button>
            ))}
          </div>
        )}

        {source !== 'procedural' && (
          <p className={s.artNote} data-testid="art-status">
            {status.loaded} of {status.total} cards in this game have art
            {status.pending > 0 && `, ${status.pending} loading`}
            {status.failed > 0 && `, ${status.failed} unavailable`}.
          </p>
        )}

        {source === 'scryfall' && (
          <div className={s.artActionsRow}>
            <button
              type="button"
              className={s.button}
              data-testid="art-clear"
              onClick={() => void clearDownloadedArt()}
            >
              Clear downloaded art
            </button>
            {storage && (
              <span className={s.artStorage} data-testid="art-storage">
                {formatBytes(storage.usage)} used of {formatBytes(storage.quota)}
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
