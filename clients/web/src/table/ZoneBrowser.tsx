/**
 * The public zone browser — graveyard and exile (React DOM, ADR 0003: a
 * scrollable pile of cards a player reads is DOM, not the Pixi canvas).
 *
 * ADR 0003 decides the *technology*; it never said the entries had to be prose.
 * They were, until issue #584: the one surface whose whole job is *looking at
 * cards* was the one surface that did not use the client's card renderer. It now
 * draws every entry with {@link CardFace} — the same component the hand fan, the
 * stack rail, and the battlefield draw — at the `stack` browse tier, on the 2.5D
 * control language's framed plate rather than the pre-2.5D overlay rectangle.
 *
 * What the browser may do is bounded on purpose:
 *
 * - **Nothing is derived.** Each entry is the `CardView` the server sent, mapped
 *   through the shared display-data glue every other card surface uses. The
 *   browser filters nothing, sorts nothing, and computes no legality.
 * - **The pile order is the server's.** `ZonePile.cards` is top-of-pile-last on
 *   the wire; the grid presents it top-first because that is the end a player
 *   looks for, states that direction in its heading, and keeps each card's wire
 *   index on the entry (see `zoneBrowserView.ts`). A reconnect replaying the same
 *   view reproduces the identical contents in the identical order.
 * - **Exile is not a second graveyard** (`docs/design/zone-geography.md` §3.3):
 *   the panel carries `data-zone`, and the stylesheet gives exile its cool rune-
 *   iris glass and the graveyard its warm ash — in the browser, not only on the
 *   rack.
 * - **DOM stays bounded** (§8.2.4): at most `ZONE_BROWSER.block` faces are
 *   mounted, so a 400-card exile pages instead of spending the scene's budget.
 *
 * All browser state (which block is shown) is ephemeral presentation: the shell
 * drops the whole browser on every fresh `GameView`.
 */
import {
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type CSSProperties,
  type KeyboardEvent,
} from 'react';
import { getArtVersion, subscribeArt } from '../card/art/artStore';
import { CARD_BOX, CardFace } from '../card/dom';
import { Glyph, type GlyphName } from '../chrome/glyphs';
import type { CardView, EntityId } from '../protocol';
import type { BrowsableZone } from './PanelChrome';
import { domCardArt } from './planeDisplayData';
import { toDisplayData } from './scene/card-helpers';
import { ZONE_BROWSER_ORDER_NOTE, zoneBrowserEntries } from './zoneBrowserView';
import s from './zone-browser.module.css';

/** Display copy per zone: the heading word and the empty state's own glyph. */
const ZONE_COPY: Record<BrowsableZone, { title: string; glyph: GlyphName; empty: string }> = {
  graveyard: {
    title: 'Graveyard',
    glyph: 'zone-graveyard',
    empty: 'Nothing has been put here yet.',
  },
  exile: { title: 'Exile', glyph: 'zone-exile', empty: 'Nothing has been exiled.' },
};

/**
 * The browse tier: the stack rail's 0.715 portrait card. Big enough to read a
 * name, a cost, and a type line at arm's length, small enough that a grid of
 * them is a pile rather than a slideshow — and already one of the three
 * screen-space tiers, so the ≤ 12-node battlefield budget does not bind
 * (`card-representation.md` §8.1).
 */
const BROWSE_TIER = 'stack' as const;

/** The drawn box of {@link BROWSE_TIER}, so the grid's column and the face
 * inside it are the same number rather than two that happen to agree. */
const BROWSE_BOX = CARD_BOX[BROWSE_TIER].card;

/** The grid geometry the stylesheet reads, sourced from the card layer. */
const GRID_VARS = {
  '--zb-card-w': `${BROWSE_BOX.w}px`,
  '--zb-card-h': `${BROWSE_BOX.h}px`,
} as CSSProperties;

interface Props {
  /** Which public zone this is — the browser's whole identity treatment. */
  zone: BrowsableZone;
  /** The owning player's display label, e.g. `"Rowan (you)"`. */
  owner: string;
  /** The pile's cards in wire order (top last), exactly as the server sent them. */
  cards: CardView[];
  /** Open the shared inspect surface on a card in the browser (issue #261). */
  onInspect: (id: EntityId) => void;
  /** Close the browser (backdrop click or the explicit close control). */
  onClose: () => void;
}

/**
 * Move focus between the grid's card buttons. The grid is a plain run of
 * buttons in presented order, so along-axis traversal is "the next button" in
 * either direction — no column arithmetic, and identical under a one-column
 * compact grid and a six-column wide one. `Tab` still works natively; this is
 * the arrow parity the rest of the table's regions have.
 */
function moveGridFocus(grid: HTMLElement, delta: number | 'first' | 'last'): boolean {
  const cards = Array.from(grid.querySelectorAll<HTMLButtonElement>('[data-browser-card]'));
  if (cards.length === 0) return false;
  const at = cards.indexOf(document.activeElement as HTMLButtonElement);
  const next =
    delta === 'first'
      ? 0
      : delta === 'last'
        ? cards.length - 1
        : Math.max(0, Math.min(cards.length - 1, (at < 0 ? 0 : at) + delta));
  cards[next]!.focus();
  return true;
}

export function ZoneBrowser({ zone, owner, cards, onInspect, onClose }: Props) {
  const copy = ZONE_COPY[zone];
  const [block, setBlock] = useState(0);
  const gridRef = useRef<HTMLUListElement>(null);
  // Repaint as illustrations finish downloading (ADR 0024), so a face in the
  // browser gains its art exactly like a face anywhere else. Presentation only.
  useSyncExternalStore(subscribeArt, getArtVersion);
  const { plan, block: current, entries } = zoneBrowserEntries(cards, block);

  // A pile that shrank out from under the shown block (a fresh view is a fresh
  // browser, but a same-mount re-render is not) falls back to a block that exists.
  useEffect(() => {
    if (current !== block) setBlock(current);
  }, [current, block]);

  const heading = `${owner} — ${copy.title}`;
  const count = `${plan.total} ${plan.total === 1 ? 'card' : 'cards'}`;
  const label =
    plan.total === 0
      ? `${owner} ${zone}, empty`
      : `Browse ${owner} ${zone}, ${count}, ${ZONE_BROWSER_ORDER_NOTE}`;

  const onGridKeyDown = (event: KeyboardEvent<HTMLUListElement>): void => {
    const grid = gridRef.current;
    if (!grid) return;
    const move =
      event.key === 'ArrowRight' || event.key === 'ArrowDown'
        ? 1
        : event.key === 'ArrowLeft' || event.key === 'ArrowUp'
          ? -1
          : event.key === 'Home'
            ? ('first' as const)
            : event.key === 'End'
              ? ('last' as const)
              : null;
    if (move === null) return;
    if (moveGridFocus(grid, move)) event.preventDefault();
  };

  return (
    <div
      data-testid="zone-browser-backdrop"
      className={s.backdrop}
      onClick={onClose}
      role="presentation"
    >
      {/* The framed plate of the control family (`control-language.md` §3.1):
          gradient frame box padding a chamfered face, never a CSS border — a
          border cannot carry the gradient and does not follow the chamfer. */}
      <div
        data-testid="zone-browser"
        data-zone={zone}
        className={s.frame}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        style={GRID_VARS}
        onClick={(event) => event.stopPropagation()}
      >
        <div className={s.face}>
          <header className={s.head}>
            <h2 className={s.title} data-testid="zone-browser-title">
              {heading}
            </h2>
            {/* The reading direction is stated, not implied (issue #584). */}
            <p className={s.subtitle} data-testid="zone-browser-order">
              {plan.total === 0 ? 'empty' : `${count} · ${ZONE_BROWSER_ORDER_NOTE}`}
            </p>
            <button
              type="button"
              data-testid="zone-browser-close"
              aria-label="Close browser"
              onClick={onClose}
              className={s.close}
            >
              ×
            </button>
          </header>

          {plan.total === 0 ? (
            // The designed quiet state (`zone-geography.md` §Empty states): an
            // etched card silhouette carrying the zone's own glyph, not a
            // sentence where a pile should be.
            <div className={s.empty} data-testid="zone-browser-empty">
              <span className={s.emptySilhouette} aria-hidden="true">
                <Glyph name={copy.glyph} size={34} />
              </span>
              <p className={s.emptyNote}>{copy.empty}</p>
            </div>
          ) : (
            <ul
              ref={gridRef}
              className={s.grid}
              data-testid="zone-browser-grid"
              onKeyDown={onGridKeyDown}
            >
              {entries.map(({ card, pileIndex, fromTop }) => (
                // A zone can legally hold duplicate identities; the wire index is
                // unique within the pile and is what keys the presented entry.
                <li key={`${card.id}-${pileIndex}`} className={s.cell}>
                  <button
                    type="button"
                    data-browser-card=""
                    data-testid={`browser-card-${card.id}`}
                    data-pile-index={pileIndex}
                    data-entity={card.id}
                    aria-label={
                      fromTop === 1 ? `Inspect ${card.name}, top of pile` : `Inspect ${card.name}`
                    }
                    onClick={() => onInspect(card.id)}
                    className={s.card}
                  >
                    <CardFace
                      data={toDisplayData(card, { selected: false, actionable: false })}
                      tier="stack"
                      art={domCardArt(card)}
                      rulesText={card.rules_text}
                    />
                  </button>
                </li>
              ))}
            </ul>
          )}

          {/* Bounded DOM (§8.2.4): a pile past the block cap pages rather than
              mounting every face. Ordinary ≥ 44 px buttons — never drag-only,
              never hover-only. */}
          {plan.paged && (
            <div className={s.pager} data-testid="zone-browser-pager">
              <button
                type="button"
                className={s.pageButton}
                data-testid="zone-browser-prev"
                aria-label={`Previous card block, block ${current} of ${plan.blocks}`}
                disabled={current === 0}
                onClick={() => setBlock(current - 1)}
              >
                ‹
              </button>
              <p className={s.pageLabel} aria-live="polite" data-testid="zone-browser-block">
                {`Block ${current + 1} of ${plan.blocks}`}
              </p>
              <button
                type="button"
                className={s.pageButton}
                data-testid="zone-browser-next"
                aria-label={`Next card block, block ${current + 2} of ${plan.blocks}`}
                disabled={current >= plan.blocks - 1}
                onClick={() => setBlock(current + 1)}
              >
                ›
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
