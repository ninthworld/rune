/**
 * Graveyard / exile zone browser (React DOM, ADR 0003 — a scrollable list of card
 * text a user reads is DOM, not the Pixi canvas).
 *
 * `GameView` already ships the full contents of these public, ordered zones as
 * {@link ZonePile}s per player; this overlay simply makes one browsable
 * (ui-requirements §1: "any public ordered zone openable as a full overlay with
 * scrolling"). It renders the pile in wire order — top of the pile last, exactly as
 * the server sent it — and each card opens the shared inspect popover (issue #261),
 * so a card in a browser is inspectable like a card anywhere else.
 *
 * Pure render of the pile handed in: nothing here is load-bearing across messages,
 * so a reconnect that replays the same view reproduces the identical list. The
 * client derives nothing — it only formats the {@link CardView}s the server sent.
 */
import type { CardView, EntityId } from '../protocol';
import {
  zoneBrowserBackdrop,
  zoneBrowserCard,
  zoneBrowserCardName,
  zoneBrowserCardType,
  zoneBrowserClose,
  zoneBrowserEmpty,
  zoneBrowserList,
  zoneBrowserPanel,
  zoneBrowserTitle,
} from './styles';

interface Props {
  /** The heading, e.g. `"p2 — Graveyard"` (composed by the caller from the view). */
  title: string;
  /** The pile's cards, in wire order (top last). Empty renders the empty state. */
  cards: CardView[];
  /** Open the inspect popover for a card in the browser (issue #261). */
  onInspect: (id: EntityId) => void;
  /** Close the browser (backdrop click or the explicit close control). */
  onClose: () => void;
}

export function ZoneBrowser({ title, cards, onInspect, onClose }: Props) {
  return (
    <div
      data-testid="zone-browser-backdrop"
      style={zoneBrowserBackdrop}
      onClick={onClose}
      role="presentation"
    >
      <div
        data-testid="zone-browser"
        style={zoneBrowserPanel}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onClick={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          data-testid="zone-browser-close"
          aria-label="Close browser"
          onClick={onClose}
          style={zoneBrowserClose}
        >
          ×
        </button>
        <h2 style={zoneBrowserTitle} data-testid="zone-browser-title">
          {title} ({cards.length})
        </h2>
        {cards.length === 0 ? (
          <p style={zoneBrowserEmpty} data-testid="zone-browser-empty">
            No cards.
          </p>
        ) : (
          <ol style={zoneBrowserList}>
            {cards.map((card, index) => (
              // A zone can legally hold duplicate identities; the entity id is unique,
              // and index guards against any repeated id in a single render.
              <li key={`${card.id}-${index}`}>
                <button
                  type="button"
                  data-testid={`browser-card-${card.id}`}
                  aria-label={`Inspect ${card.name}`}
                  onClick={() => onInspect(card.id)}
                  style={zoneBrowserCard}
                >
                  <span style={zoneBrowserCardName}>{card.name}</span>
                  <span style={zoneBrowserCardType}>{card.type_line}</span>
                </button>
              </li>
            ))}
          </ol>
        )}
      </div>
    </div>
  );
}
