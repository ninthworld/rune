/**
 * The client shell: a rail of destinations, and the content region beside it.
 *
 * `docs/client-design.md` §9 settled the register — the lobby is a shell, not a table and not a
 * room — because the deck builder is the densest surface in the product, it is unavoidably a
 * list of hundreds of cards with filters over it, and it lives here. A spatial lobby would have
 * to hand off to a conventional one the moment a player built a deck, and that seam is worse
 * than not having the metaphor.
 *
 * The rail carries **Play**, **Decks**, **Settings** with the player's identity at its foot, out
 * of the way of the thing they came to do. At narrow widths it becomes a bar across the bottom;
 * the destinations and their order do not change, which is what makes one shell rather than two.
 *
 * **Which destination you are on is the client's answer. Which contract you are on is the
 * server's.** This component knows the first and nothing at all about the second: it is never
 * handed a view, so there is no way for it to grow an opinion about what the server said.
 *
 * The content region is the one place in this client that may scroll. §3's no-scrolling rule is
 * about the *board* — a board a player has to scroll is a board they cannot read — and a list of
 * tables is not a board. It scrolls; the rail does not move when it does.
 */
import type { ReactNode } from 'react'

import { DESTINATIONS, type Destination } from './../../shell'

export function Shell({
  destination,
  onDestination,
  identity,
  children,
}: {
  destination: Destination
  onDestination(destination: Destination): void
  /** What the server calls you: the name it accepted, or the id it issued. */
  identity: string
  children: ReactNode
}) {
  return (
    <div className="shell">
      <nav className="shell__rail" aria-label="Destinations">
        <span className="shell__mark" aria-hidden="true">
          SAGE
        </span>
        <ul className="shell__list">
          {DESTINATIONS.map((entry) => (
            <li key={entry.id}>
              <button
                type="button"
                className={`shell__go${destination === entry.id ? ' shell__go--on' : ''}`}
                // The destination in force, stated rather than only drawn: an accent on a
                // button is not a fact a screen reader can read.
                aria-current={destination === entry.id ? 'page' : undefined}
                onClick={() => onDestination(entry.id)}
              >
                <span className="shell__glyph" aria-hidden="true">
                  {entry.glyph}
                </span>
                <span className="shell__label">{entry.label}</span>
              </button>
            </li>
          ))}
        </ul>
        {/* Who the server says you are, at the foot, where a task performed once belongs —
            never in the chrome of every screen as something to fill in. */}
        <p className="shell__you" title={identity}>
          {identity}
        </p>
      </nav>

      <div className="shell__content">{children}</div>
    </div>
  )
}
