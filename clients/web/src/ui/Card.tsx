/**
 * One card frame, at five sizes.
 *
 * Every card-shaped thing on the screen comes through here: a card in hand, a permanent on the
 * battlefield, an object on the stack, an emblem, a compact row in a pile, and the full face in
 * the inspector. They share a model (`card-face.ts`) and they share this component, so the board
 * cannot end up disagreeing with the hand about what a counter or a token looks like.
 *
 * A variant is a budget, not a different card. Each one decides how much of the same face there
 * is room to show, and anything it drops or clamps is still one gesture away — the pointer's
 * preview, or the inspector, which is the only variant that shows everything. That is the whole
 * contract: nothing is ever the *only* place a fact appears.
 *
 * The frame is a real card's anatomy — name band, art window, type band, text box, and a stat in
 * the corner — at a real card's proportions, because that is what makes a board readable at a
 * glance rather than a list of tiles. The tint under it is `costTint`: the colours of the pips
 * that were printed, and explicitly not colour, colour identity, or anything else the rules
 * define (`mana.ts`). It is a wash under text that says the real thing.
 *
 * No decision about the game is made here. `state` is assigned by the caller from what the
 * server advertised; this component styles it and draws no conclusion of its own.
 */
import type { MouseEvent } from 'react'

import type { CardFace, CardFaceLink, CardFaceState, CardFaceVariant } from './../card-face'
import { costTint } from './../mana'
import { useCardArt } from './art'
import { CardArt } from './CardArt'
import { ManaCost } from './Mana'

/** What each variant has room for. Everything omitted here remains available in `inspect`. */
const SHOWS: Record<
  CardFaceVariant,
  {
    cost: boolean
    art: boolean
    typeLine: boolean
    rulesText: boolean
    keywords: boolean
    /** Counters, marked damage, and tap state — what the battlefield adds to a card. */
    board: boolean
  }
> = {
  inspect: { cost: true, art: true, typeLine: true, rulesText: true, keywords: true, board: true },
  hand: { cost: true, art: true, typeLine: true, rulesText: true, keywords: true, board: false },
  battlefield: {
    cost: false,
    art: true,
    typeLine: true,
    rulesText: false,
    keywords: true,
    board: true,
  },
  stack: { cost: true, art: false, typeLine: true, rulesText: true, keywords: false, board: false },
  compact: {
    cost: true,
    art: false,
    typeLine: true,
    rulesText: false,
    keywords: false,
    board: false,
  },
}

/** Wording for a state, so it is announced rather than being colour alone. */
const STATE_LABELS: Record<CardFaceState, string | undefined> = {
  idle: undefined,
  selected: 'Selected',
  candidate: 'Choosable',
  pending: 'Pending',
  disabled: 'Unavailable',
}

/** The same, for a relationship: emphasis a screen reader cannot see is not emphasis. */
const LINK_LABELS: Record<'focus' | 'linked', string> = {
  focus: 'Tracing from',
  linked: 'Related',
}

export interface CardProps {
  face: CardFace
  variant: CardFaceVariant
  /**
   * How this face is taking part in the interaction in progress. Presentation only: the caller
   * decides it from `valid_actions`, and nothing here infers legality.
   */
  state?: CardFaceState
  /**
   * Where this object sits in the relationships being traced (`relations.ts`). Independent of
   * `state`: a creature can be a legal target *and* the blocker of whatever the player just
   * clicked, and a board that showed only one of those would hide the other.
   */
  link?: CardFaceLink
  /**
   * This card was clicked. One gesture for every card on the screen; what it does — take the one
   * action the server offered for it, fill a target slot, open the dock — is decided by the
   * caller from what the server advertised (`interaction.ts`), never here.
   */
  onActivate?(id: string): void
  /**
   * This card was right-clicked. Reading a card is never a game action, so it is on the gesture
   * that costs nothing and is available whatever else is in progress.
   */
  onInspect?(id: string): void
  /**
   * The pointer or the keyboard reached this card, or left it (`undefined`).
   *
   * Looking is its own channel: it raises the preview that makes a small frame readable, and it
   * traces the relationships the server projected about this object. Neither is hung off
   * selection, because the objects most worth reading and tracing — a blocker, an enchanted
   * creature, anything with no action of its own — are exactly the ones a click cannot reach.
   */
  onTrace?(id: string | undefined): void
}

export function Card({
  face,
  variant,
  state = 'idle',
  link,
  onActivate,
  onInspect,
  onTrace,
}: CardProps) {
  const shows = SHOWS[variant]
  const stateLabel = STATE_LABELS[state]
  const tapped = shows.board && face.tapped

  // A player-supplied illustration, if this device has a source and this card has resolved
  // (`ui/art.tsx`). Absent is the normal answer and costs nothing: the frame draws its
  // procedural face, which is what it draws when nothing is turned on at all.
  const art = useCardArt(face)
  // The whole card image as the face (ADR 0012). Only where the window has room to be a card:
  // a one-line row in a pile and an object on the stack have no art window to fill, so they keep
  // SAGE's text whatever the preference says.
  const full = shows.art && face.typeLine !== undefined && art?.style === 'full'

  const className = [
    'card',
    `card--${variant}`,
    `card--tint-${costTint(face.manaCost)}`,
    full && 'card--full',
    state !== 'idle' && `card--${state}`,
    link && `card--${link}`,
    tapped && 'card--tapped',
  ]
    .filter(Boolean)
    .join(' ')

  const body = (
    <>
      {/* Under a full card image the printed text is on the picture, so SAGE's copy of it is
          hidden rather than removed: it is still this control's accessible name, still what a
          screen reader reads, and still what a search of the page finds. What is *never* hidden
          is anything the server computed — the stat, the counters, the damage, the state — which
          is why those sit outside this block. */}
      <span className={full ? 'visually-hidden' : 'card__band'}>
        <span className="card__name" title={face.name}>
          {face.name}
        </span>
        {shows.cost && !full && <ManaCost cost={face.manaCost} className="card__cost" />}
      </span>

      {/* Withheld for an object with no type line — an emblem, or a bare ability on the stack —
          because those are not printed cards and framing them as one would imply a face that
          does not exist. */}
      {shows.art && face.typeLine && <CardArt face={face} url={art?.url} />}

      {shows.typeLine && face.typeLine && (
        <span className={full ? 'visually-hidden' : 'card__type'} title={face.typeLine}>
          {face.typeLine}
        </span>
      )}

      {(shows.rulesText || shows.keywords) && (face.rulesText || face.keywords.length > 0) && (
        <span className={full ? 'visually-hidden' : 'card__text'}>
          {shows.rulesText && face.rulesText && (
            <span className="card__rules" title={face.rulesText}>
              {face.rulesText}
            </span>
          )}
          {shows.keywords && face.keywords.length > 0 && (
            <span className="card__keywords">{face.keywords.join(' · ')}</span>
          )}
        </span>
      )}

      <span className="card__badges">
        {stateLabel && <span className="badge badge--state">{stateLabel}</span>}
        {link && <span className="badge badge--link">{LINK_LABELS[link]}</span>}
        {face.markers.map((marker) => (
          <span key={marker} className="badge badge--marker">
            {marker}
          </span>
        ))}
        {tapped && <span className="badge badge--tapped">Tapped</span>}
        {shows.board &&
          face.counters.map((counter) => (
            <span key={counter.kind} className="badge badge--counter">
              {counter.count}× {counter.kind}
            </span>
          ))}
        {shows.board && face.damage !== undefined && (
          <span className="badge badge--damage">{face.damage} damage</span>
        )}
      </span>

      {/* Outside the badge row and in the corner the printed one sits in, because it is the
          number a player scans a whole board for and a row that reflows is the wrong place to
          keep the one thing that must stay findable. */}
      {face.stat && (
        <span className={`card__stat card__stat--${face.stat.kind}`}>
          <span className="visually-hidden">{face.stat.label} </span>
          {face.stat.value}
        </span>
      )}
    </>
  )

  // Pointer and keyboard say the same thing, so the preview and the trace are not mouse-only
  // affordances.
  const looked = onTrace && {
    onMouseEnter: () => onTrace(face.id),
    onMouseLeave: () => onTrace(undefined),
    onFocus: () => onTrace(face.id),
    onBlur: () => onTrace(undefined),
  }

  // The browser's own menu is suppressed only where this offers one of its own, so a surface
  // that passed no handler behaves exactly as it did.
  const inspected = onInspect && {
    onContextMenu: (event: MouseEvent) => {
      event.preventDefault()
      onInspect(face.id)
    },
  }

  // A button whenever it is clickable, so a keyboard reaches it on the same terms as a mouse and
  // no key handling has to be reinvented. Never `disabled`: the gesture always leads somewhere —
  // at worst to the inspector — and reading an object a player cannot act on is exactly when
  // they most need to.
  if (!onActivate) {
    return (
      <div className={className} {...looked} {...inspected}>
        {body}
      </div>
    )
  }
  return (
    <button
      type="button"
      className={className}
      onClick={() => onActivate(face.id)}
      {...looked}
      {...inspected}
    >
      {body}
    </button>
  )
}
