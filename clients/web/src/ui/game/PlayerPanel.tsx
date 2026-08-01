/**
 * One seat: who they are, what they have left, and how to open what is in front of them.
 *
 * The same component draws you and your opponent, because the difference between the two is
 * data rather than structure — you have a mana pool and a visible hand, they have a hand size —
 * and giving each its own component is how the two drift until a status shows on one side only.
 *
 * Everything is drawn from what the server projected for that seat. A field the view did not
 * carry is not rendered at all: a seat with no life total shows no life, because `0 life` is a
 * player who has lost and is not a thing to say about a seat nobody sent a number for.
 *
 * **A zone a player may look through is a button; a zone they may not is a number.** That is the
 * one line in the panel that carries a rule: a library and an opponent's hand are hidden zones
 * and the view projects a count and nothing else, so a count is all there is to draw and there
 * is nothing to open. The public piles are itemized, so they open. The distinction is the
 * server's — this panel opens what it was sent cards for.
 */
import type { Seat, SeatPile } from './../../table'
import type { RelationLine } from './../../relations'
import { anchorProps } from './../../overlay'
import { ManaCost } from './../Mana'
import { RelationTrail } from './RelationTrail'
import type { Surface } from './surface'

/** One entry in the zone row: a public pile to open, or a hidden zone's count. */
interface ZoneChip {
  key: string
  label: string
  count: number
  /** Present only where the view itemized the cards, which is what makes it browsable. */
  zone?: SeatPile['zone']
}

const PUBLIC_ZONES = [
  ['graveyard', 'Graveyard'],
  ['exile', 'Exile'],
  ['command', 'Command'],
] as const

export function PlayerPanel({
  seat,
  lines,
  open,
  onOpen,
  surface,
}: {
  seat: Seat
  /** What the view relates this seat to — an attack aimed at it, a spell that named it. */
  lines: readonly RelationLine[]
  /** The pile of this seat's currently open in the browser, if any. */
  open?: SeatPile['zone']
  onOpen(zone: SeatPile['zone']): void
  surface: Surface
}) {
  const state = surface.stateOf(seat.id)
  const link = surface.linkOf(seat.id)
  // Life is lifted out of the list it used to be a substring of. It is the number a player
  // watches more than any other, and `9 life · Library (28) · Hand (4)` gave it exactly as much
  // weight as the size of a graveyard. Everything else stays a list, because everything else is.
  const marks = [
    ...seat.statuses,
    seat.eliminated && 'eliminated',
    !seat.connected && 'disconnected',
    seat.ai && 'AI',
  ].filter((entry): entry is string => typeof entry === 'string')

  const zones: ZoneChip[] = []
  if (seat.librarySize !== undefined) {
    zones.push({ key: 'library', label: 'Library', count: seat.librarySize })
  }
  if (seat.handSize !== undefined) zones.push({ key: 'hand', label: 'Hand', count: seat.handSize })
  for (const [zone, label] of PUBLIC_ZONES) {
    const pile = seat.piles.find((each) => each.zone === zone)
    if (pile) zones.push({ key: zone, label, count: pile.faces.length, zone })
    // A seat whose graveyard was summarised rather than itemized still has a graveyard, and a
    // player counting cards in it needs the number. It is not openable, because there is
    // nothing to open — the view sent no cards.
    else if (zone === 'graveyard' && seat.graveyardSize !== undefined) {
      zones.push({ key: zone, label, count: seat.graveyardSize })
    }
  }

  return (
    <section
      className={`seat ${seat.isYou ? 'seat--you' : 'seat--opponent'}`}
      aria-label={seat.isYou ? 'Your seat' : `${seat.name} seat`}
    >
      {/* The seat itself is clickable, because a player is a target: "any target" and "target
          player" name a seat the same way a burn spell names a creature, and a table where the
          creature can be clicked but the person cannot is a table where half of red is
          unplayable without hunting through a list. The state is the server's own answer about
          this id, exactly as it is for a permanent.

          It is anchored for the same reason: an attack aimed at this seat is a relationship the
          server stated, and the line the board draws for it has to end somewhere. */}
      <p className="seat__who">
        <button
          type="button"
          className={['seat__name', state !== 'idle' && `card--${state}`, link && `card--${link}`]
            .filter(Boolean)
            .join(' ')}
          {...anchorProps(seat.id)}
          onClick={() => surface.activate(seat.id)}
          onMouseEnter={() => surface.trace(seat.id)}
          onMouseLeave={() => surface.trace(undefined)}
          onFocus={() => surface.trace(seat.id)}
          onBlur={() => surface.trace(undefined)}
        >
          <strong>{seat.name}</strong>
          {seat.isYou && ' (you)'}
        </button>
      </p>

      {seat.life !== undefined && (
        <p className="seat__life">
          {/* The unit is said once, to a screen reader, because a bare number read out is not a
              life total — and drawing the word beside a figure this size would halve it. */}
          <span className="seat__life-value">{seat.life}</span>
          <span className="visually-hidden"> life</span>
        </p>
      )}

      {marks.length > 0 && <p className="seat__marks">{marks.join(' · ')}</p>}

      <p className="seat__zones">
        {zones.map(({ key, label, count, zone }) =>
          zone ? (
            <button
              key={key}
              type="button"
              className="seat__zone"
              aria-pressed={open === zone}
              onClick={() => onOpen(zone)}
            >
              {label} ({count})
            </button>
          ) : (
            <span key={key} className="seat__zone seat__zone--closed">
              {label} ({count})
            </span>
          ),
        )}
      </p>

      {seat.manaPool.length > 0 && (
        <p className="seat__pool">
          Pool:{' '}
          {seat.manaPool.map((pip, index) => (
            <span
              key={index}
              className={`pip${pip.restricted ? ' pip--restricted' : ''}`}
              // Marked twice, because the mark is the whole point: an asterisk a sighted player
              // can see, and the word a screen reader hears. Colour alone would say neither.
              title={pip.restricted ? 'Restricted mana' : undefined}
            >
              {/* The same disc the cost on a card is drawn with. Floating mana and the cost it
                  is about to pay are the same symbols, and a pool that spelled them differently
                  would make the one comparison a player makes constantly harder than it is. */}
              <ManaCost cost={pip.symbol} />
              {pip.restricted && (
                <>
                  *<span className="visually-hidden"> restricted</span>
                </>
              )}
            </span>
          ))}
          {/* The legend appears only when there is something to explain. What it can say is
              bounded by what the server said: the wire marks a pip restricted, never what to,
              so this reports the restriction and does not guess at its condition. */}
          {seat.manaPool.some((pip) => pip.restricted) && (
            <span className="seat__pool-note"> — * spendable only on what made it</span>
          )}
        </p>
      )}

      {seat.commanderName !== undefined && (
        <p className="seat__commander">
          Commander: {seat.commanderName}
          {seat.commanderCasts !== undefined && <> · cast {seat.commanderCasts}×</>}
          {seat.commanderTax !== undefined && <> · tax {seat.commanderTax}</>}
        </p>
      )}

      {seat.commanderDamage.length > 0 && (
        <p className="seat__commander">
          {/* Commander damage is a per-source total that kills at 21 (CR 903.10a), so it is
              named by the commander that dealt it rather than summed into one number. */}
          Commander damage:{' '}
          {seat.commanderDamage
            .map((damage) => `${surface.labelFor(damage.from)} ${damage.amount}`)
            .join(' · ')}
        </p>
      )}

      <RelationTrail lines={lines} surface={surface} />
    </section>
  )
}
