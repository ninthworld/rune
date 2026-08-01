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
 *
 * **What gives way when the bar cannot carry all of it** is §2's "The seat bar", and it is the
 * one decision in this file that is a design decision rather than a reading of the view: life and
 * the status marks are always drawn, and the five zone counts fold into one control that opens in
 * one gesture. Life is tier 1 and stays tier 1; a library count is something a player *checks*,
 * which is tier 2 and is where it belonged. The fold is deliberately **symmetric** — your counts
 * are the ones you act on and there is a real argument for keeping them longer, but the cost is
 * that a player learns two different bars and then has to work out which one they are reading.
 * One bar, read the same way at both ends of the table, is worth more than the extra count.
 *
 * It is a `<details>` and not a popover held open by state, because the fold is not something the
 * game has an opinion about: one gesture opens it, the next closes it, a new `GameView` neither
 * knows nor cares, and nothing about it is load-bearing across a message.
 */
import type { ReactNode } from 'react'

import type { Seat, SeatPile } from './../../table'
import type { RelationLine } from './../../relations'
import { lifeWording } from './../../motion'
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
  life,
  folded = false,
  open,
  onOpen,
  surface,
}: {
  seat: Seat
  /** What the view relates this seat to — an attack aimed at it, a spell that named it. */
  lines: readonly RelationLine[]
  /**
   * How much this seat's life moved in the message that produced this view, if it moved.
   *
   * A transition rather than a fact about the game: it is the difference between the last two
   * `GameView`s (`motion.ts`), it survives no refresh, and the total beside it is the only thing
   * anybody plays off. It exists because a life total that silently changes from 20 to 17 is a
   * player wondering whether they missed something, and the log is a column away.
   */
  life?: number
  /**
   * Whether there is room for the five zone counts on the bar, or only for the fold that holds
   * them (§2, "The seat bar"). Decided by the arrangement, never by what is in the zones.
   */
  folded?: boolean
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
          {/* What just happened to it, until the next message. The sign carries the direction
              and the colour only repeats it: a number with no sign says nothing, and a red one
              says nothing to a player who cannot see red. Announced politely, because a life
              total changing is worth hearing and is never worth interrupting a sentence for. */}
          {life !== undefined && (
            <span className={`seat__delta seat__delta--${life > 0 ? 'up' : 'down'}`} role="status">
              <span aria-hidden="true">
                {life > 0 ? '+' : '−'}
                {Math.abs(life)}
              </span>
              <span className="visually-hidden">{lifeWording(life)}</span>
            </span>
          )}
        </p>
      )}

      {marks.length > 0 && <p className="seat__marks">{marks.join(' · ')}</p>}

      <Fold folded={folded} count={zones.length} seat={seat.id}>
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
                // Marked twice, because the mark is the whole point: an asterisk a sighted
                // player can see, and the word a screen reader hears. Colour alone says neither.
                title={pip.restricted ? 'Restricted mana' : undefined}
              >
                {/* The same disc the cost on a card is drawn with. Floating mana and the cost it
                    is about to pay are the same symbols, and a pool that spelled them
                    differently would make the one comparison a player makes constantly harder
                    than it is. */}
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
      </Fold>

      <RelationTrail lines={lines} surface={surface} />
    </section>
  )
}

/**
 * The part of the bar that folds, drawn either way.
 *
 * Wide, it is the bar itself and there is no control at all — a disclosure a player never has to
 * open is a disclosure that only costs them a click. Narrow, the same children move behind one
 * control carrying the count, which is the shape §2 settled on: what folds is *checked*, not
 * read, and the number of things there are to check is itself worth stating on the closed
 * control.
 *
 * A **popover**, rather than a panel positioned inside the bar. Every region of the table clips
 * what does not fit in it, which is the whole point of the frame, and a disclosure drawn inside
 * a 40px band would open into a box 40px tall and be cut off by the rule that makes the rest of
 * this work. The top layer is the browser's own answer to that, and it comes with the light
 * dismiss and the escape key already attached — so nothing about whether this is open is state
 * this client holds, and a new `GameView` neither knows nor cares.
 */
function Fold({
  folded,
  count,
  seat,
  children,
}: {
  folded: boolean
  count: number
  /** The seat's own id, so two bars on one screen do not share one disclosure. */
  seat: string
  children: ReactNode
}) {
  if (!folded) return <>{children}</>
  const id = `zones-${seat}`
  return (
    <div className="seat__fold">
      <button type="button" className="seat__fold-summary" popoverTarget={id}>
        Zones<span className="seat__fold-count"> ({count})</span>
      </button>
      <div id={id} popover="auto" className="seat__fold-body">
        {children}
      </div>
    </div>
  )
}
