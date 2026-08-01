/**
 * One seat: who they are, what they have left, and what is in their public piles.
 *
 * The same component draws you and your opponent, because the difference between the two is
 * data rather than structure — you have a mana pool and a visible hand, they have a hand size —
 * and giving each its own component is how the two drift until a status shows on one side only.
 *
 * Everything is drawn from what the server projected for that seat. A field the view did not
 * carry is not rendered at all: a seat with no life total shows no life, because `0 life` is a
 * player who has lost and is not a thing to say about a seat nobody sent a number for.
 */
import type { Seat } from './../../table'
import { Card } from './../Card'
import type { Surface } from './surface'

export function PlayerPanel({ seat, surface }: { seat: Seat; surface: Surface }) {
  const state = surface.stateOf(seat.id)
  const stats = [
    seat.life !== undefined && `${seat.life} life`,
    seat.librarySize !== undefined && `${seat.librarySize} library`,
    seat.handSize !== undefined && `${seat.handSize} hand`,
    seat.graveyardSize !== undefined && `${seat.graveyardSize} graveyard`,
    ...seat.statuses,
    seat.eliminated && 'eliminated',
    !seat.connected && 'disconnected',
    seat.ai && 'AI',
  ].filter((entry): entry is string => typeof entry === 'string')

  return (
    <section
      className={`seat ${seat.isYou ? 'seat--you' : 'seat--opponent'}`}
      aria-label={seat.isYou ? 'Your seat' : `${seat.name} seat`}
    >
      {/* The seat itself is clickable, because a player is a target: "any target" and "target
          player" name a seat the same way a burn spell names a creature, and a table where the
          creature can be clicked but the person cannot is a table where half of red is
          unplayable without hunting through a list. The state is the server's own answer about
          this id, exactly as it is for a permanent. */}
      <p className="seat__who">
        <button
          type="button"
          className={['seat__name', state !== 'idle' && `card--${state}`].filter(Boolean).join(' ')}
          onClick={() => surface.activate(seat.id)}
        >
          <strong>{seat.name}</strong>
          {seat.isYou && ' (you)'}
        </button>
      </p>

      <p className="seat__stats">{stats.join(' · ')}</p>

      {seat.manaPool.length > 0 && <p className="seat__pool">Pool: {seat.manaPool.join(' ')}</p>}

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

      {/* The piles are public and often long. Collapsed by default so a full graveyard cannot
          push the table around, and openable in place — a `details` element carries its own
          open state in the DOM, so nothing here is remembered between messages. */}
      {seat.piles.map((pile) => (
        <details key={pile.zone} className="pile">
          <summary>
            {pile.label} ({pile.faces.length})
          </summary>
          <ul className="cards cards--compact">
            {pile.faces.map((face) => (
              <li key={face.id}>
                <Card
                  face={face}
                  variant="compact"
                  state={surface.stateOf(face.id)}
                  onActivate={surface.activate}
                />
              </li>
            ))}
          </ul>
        </details>
      ))}
    </section>
  )
}
