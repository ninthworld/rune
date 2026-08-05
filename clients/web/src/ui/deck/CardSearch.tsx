/**
 * The card search, across the top of the browsing half of the builder.
 *
 * A menu of sets, a name to search for, the colours as toggles, and the way to more settings than
 * fit on one line. **Every toggle starts on**: switching one off is the player saying they do not
 * want to see that kind, so an untouched search shows the whole catalog.
 *
 * **The sets menu has nothing behind it yet.** `CatalogCard` carries no set — the wire states a
 * functional identity, a name, a cost, types and a colour identity, and nothing about printings —
 * so the menu is drawn and says so rather than being left out and having to be designed back in
 * the day the field arrives.
 */
import { useState } from 'react'

import { CARD_KINDS, type CardKind } from './../../builder'
import { Pip } from './../card/Pips'

const LABELS: Record<CardKind, string> = {
  W: 'White',
  U: 'Blue',
  B: 'Black',
  R: 'Red',
  G: 'Green',
  C: 'Colorless',
  land: 'Lands',
}

export function CardSearch({
  text,
  kinds,
  onText,
  onToggle,
  onMore,
}: {
  text: string
  kinds: readonly CardKind[]
  onText(text: string): void
  onToggle(kind: CardKind): void
  onMore(): void
}) {
  const [sets, setSets] = useState(false)

  return (
    <div className="search-bar">
      <span className="search-sets">
        <button className="view-btn" aria-expanded={sets} onClick={() => setSets(!sets)}>
          Sets ▾
        </button>
        {sets && (
          <div className="search-menu" role="dialog" aria-label="Sets">
            <div className="search-menu-acts">
              <button className="view-btn" disabled>
                All
              </button>
              <button className="view-btn" disabled>
                None
              </button>
            </div>
            <p className="search-note">The catalog carries no sets yet.</p>
          </div>
        )}
      </span>

      <input
        className="connect-input search-name"
        aria-label="Search cards"
        value={text}
        onChange={(event) => onText(event.target.value)}
        placeholder="Card name"
      />

      <span className="search-colors">
        {CARD_KINDS.filter((kind) => kind !== 'land').map((kind) => (
          <button
            key={kind}
            className={`color-btn${kinds.includes(kind) ? ' color-on' : ''}`}
            aria-pressed={kinds.includes(kind)}
            aria-label={LABELS[kind]}
            title={LABELS[kind]}
            onClick={() => onToggle(kind)}
          >
            <Pip symbol={kind} />
          </button>
        ))}
        <button
          className={`view-btn${kinds.includes('land') ? ' view-on' : ''}`}
          aria-pressed={kinds.includes('land')}
          onClick={() => onToggle('land')}
        >
          Lands
        </button>
      </span>

      <span className="topbar-fill" />

      <button className="settings-btn" title="More search settings" onClick={onMore}>
        ⚙
      </button>
    </div>
  )
}
