/**
 * The seat: everything you can point at that is not a card on the board
 * (`docs/client-design.md` §4.2).
 *
 * Each zone is a glyph with its count — the *labels* were what made this bar taller than a seat
 * has room for — and every one of them is a bounded, hoverable box rather than a run of text, so
 * there is already an area here for a target or an arrow's head to land on.
 *
 * **The counts are glyphs and never fold.** Which zones exist is a property of the game rather
 * than of a player: either every bar has a command zone or none does, so the same glyph is
 * always in the same place whichever seat you are reading. A seat with an empty graveyard still
 * draws its graveyard.
 *
 * Every number here was stated by the server, and an absence stays absent — a seat the view
 * projected no totals for draws no count rather than a zero that would read as a real one.
 */
import type { Seat, SeatPile } from './../../table'
import { Pip } from './../card/Pips'
import { manaSymbols, spokenSymbol } from './../../mana'

/**
 * Zone glyphs, drawn rather than fetched — the project ships no icon font and no third-party
 * art. Each is a 16×16 stroke drawing chosen to stay readable at the ~14px a seat gives it,
 * which rules out anything with interior detail.
 */
function Glyph({ name }: { name: string }) {
  return (
    <svg
      className="glyph"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {/* a hand is cards held at an angle; a third card turns to mush at this size */}
      {name === 'hand' && (
        <>
          <rect x="6" y="3.4" width="6.4" height="9.6" rx="1.1" transform="rotate(17 9.2 8.2)" />
          <rect x="3.2" y="3.4" width="6.4" height="9.6" rx="1.1" transform="rotate(-14 6.4 8.2)" />
        </>
      )}
      {/* the library is the same card squared up and stacked */}
      {name === 'deck' && (
        <>
          <path d="M5 4h5.6a1.4 1.4 0 0 1 1.4 1.4V11" />
          <rect x="2.6" y="5.6" width="8.4" height="8.4" rx="1.3" />
        </>
      )}
      {/* a headstone: the one zone whose meaning survives being 14px tall */}
      {name === 'graveyard' && (
        <>
          <path d="M4.5 12.3V7.6a3.5 3.5 0 0 1 7 0v4.7" />
          <rect x="2.4" y="12.3" width="11.2" height="1.7" rx="0.6" />
        </>
      )}
      {/* exile is a card that is no longer really there */}
      {name === 'exile' && (
        <rect x="4" y="2.5" width="8" height="11" rx="1.2" strokeDasharray="2.6 1.9" />
      )}
      {name === 'command' && (
        <>
          <path d="M3 12V5.2l3 2.4 2-4.2 2 4.2 3-2.4V12z" fill="currentColor" strokeWidth="1.2" />
          <path d="M3.4 14h9.2" />
        </>
      )}
      {name === 'focus' && <path d="M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10" />}
    </svg>
  )
}

/** One zone slot on the bar: a glyph, a count, and whether there is anything to open. */
interface Zone {
  key: string
  label: string
  count?: number
  pile?: SeatPile
}

export function PlayerBar({
  seat,
  /** Your own hand is drawn along the bottom edge; the bar still carries the zone. */
  handCount,
  focused,
  onFocus,
  onOpen,
  onActivate,
  state,
  link,
}: {
  seat: Seat
  handCount?: number
  focused?: boolean
  onFocus?(): void
  onOpen?(pile: SeatPile): void
  /** A seat is a thing an action can name — a player is a target. */
  onActivate?(id: string): void
  state?: string
  link?: string
}) {
  const pile = (zone: SeatPile['zone']) => seat.piles.find((entry) => entry.zone === zone)
  const graveyard = pile('graveyard')
  const exile = pile('exile')
  const command = pile('command')

  const zones: Zone[] = [
    { key: 'hand', label: 'Hand', ...countOf(handCount ?? seat.handSize) },
    { key: 'deck', label: 'Library', ...countOf(seat.librarySize) },
    {
      key: 'graveyard',
      label: 'Graveyard',
      ...countOf(graveyard?.faces.length ?? seat.graveyardSize),
      ...(graveyard ? { pile: graveyard } : {}),
    },
    // Exile and the command zone are public and itemized, so an absent pile is an empty one —
    // unlike a hand or a library, where an absent count is the server not having said.
    {
      key: 'exile',
      label: 'Exile',
      count: exile?.faces.length ?? 0,
      ...(exile ? { pile: exile } : {}),
    },
    // The command zone rides only in a game that has one, which is a property of the game.
    ...(seat.commanderName !== undefined || command
      ? [
          {
            key: 'command',
            label: 'Command zone',
            count: command?.faces.length ?? 0,
            ...(command ? { pile: command } : {}),
          },
        ]
      : []),
  ]

  return (
    <div
      className={`player-bar${seat.eliminated ? ' player-out' : ''}`}
      data-seat={seat.id}
      data-state={state}
      data-link={link}
    >
      <div className="player-head">
        <button
          className="player-target"
          title={`${seat.name}${seat.life === undefined ? '' : ` — ${seat.life} life`}`}
          data-anchor={`player:${seat.id}`}
          aria-label={`${seat.name}${seat.life === undefined ? '' : `, ${seat.life} life`}`}
          onClick={onActivate && (() => onActivate(seat.id))}
        >
          <span className="player-name">{seat.name}</span>
          {seat.life !== undefined && <span className="player-life">{seat.life}</span>}
        </button>
        {onFocus && (
          <button
            className={`focus-btn${focused ? ' focus-on' : ''}`}
            onClick={onFocus}
            title={focused ? 'Show every seat' : 'Show only this seat'}
          >
            <Glyph name="focus" />
          </button>
        )}
      </div>

      <div className="zone-grid">
        {zones.map((zone) => {
          const open = zone.pile
          return (
            <button
              key={zone.key}
              className="zone-btn"
              data-zone={zone.key}
              title={`${zone.label}${zone.count === undefined ? '' : ` — ${zone.count}`}`}
              aria-label={`${seat.name}: ${zone.label}${zone.count === undefined ? '' : `, ${zone.count}`}`}
              disabled={open === undefined}
              onClick={open && onOpen ? () => onOpen(open) : undefined}
            >
              <Glyph name={zone.key} />
              <span className="zone-count">{zone.count ?? '—'}</span>
            </button>
          )
        })}
      </div>

      {seat.manaPool.length > 0 && (
        <div className="mana-pool" title="Mana pool">
          {seat.manaPool.flatMap((pip, index) =>
            manaSymbols(pip.symbol).map((symbol, i) => (
              <Pip
                key={`${index}:${i}`}
                symbol={symbol.glyph}
                label={`${spokenSymbol(symbol)}${pip.restricted ? ', restricted' : ''}`}
              />
            )),
          )}
        </div>
      )}

      <div className="player-status">
        {seat.statuses.map((status) => (
          <span key={status} className="player-flag">
            {status}
          </span>
        ))}
        {seat.eliminated && <span className="player-flag">Out</span>}
        {!seat.connected && <span className="player-flag">Away</span>}
      </div>
    </div>
  )
}

/** A count the server stated, or nothing at all — never a zero standing in for an absence. */
const countOf = (count: number | undefined): { count?: number } =>
  count === undefined ? {} : { count }
