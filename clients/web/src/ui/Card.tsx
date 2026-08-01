/**
 * One card frame, at five sizes.
 *
 * Every card-shaped thing on the screen comes through here: a card in hand, a permanent on the
 * battlefield, an object on the stack, an emblem, a compact row in a pile, and the full face in
 * the inspector. They share a model (`card-face.ts`) and they share this component, so the
 * board cannot end up disagreeing with the hand about what a counter or a token looks like.
 *
 * A variant is a budget, not a different card. Each one decides how much of the same face there
 * is room to show, and anything it drops or clamps is still one click away in `inspect`, which
 * is the only variant that shows everything. That is the whole contract: nothing is ever the
 * *only* place a fact appears.
 *
 * The frame reserves an art window it currently fills with a procedural placeholder. Nothing
 * downloads and nothing is bundled — the window exists so the name band, cost, type line, and
 * stat keep their positions if ADR 0012's player-side art ever fills it.
 *
 * No decision about the game is made here. `state` is assigned by the caller from what the
 * server advertised; this component styles it and draws no conclusion of its own.
 */
import type { CardFace, CardFaceLink, CardFaceState, CardFaceVariant } from './../card-face'

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
   * This card was clicked. One gesture for every card on the screen; what it does — fill a
   * target slot, select a subject, open the inspector — is decided by the caller from what the
   * server offered for this object (`interaction.ts`), never here.
   */
  onActivate?(id: string): void
  /**
   * The pointer or the keyboard reached this card, or left it (`undefined`).
   *
   * Tracing relationships is a *look*, not a click. A card the server offered nothing for opens
   * the inspector on its first click, so hanging the trace off selection would put the objects
   * most worth tracing — a blocker, an enchanted creature, anything with no action of its own —
   * behind a modal that covers the board they are on.
   */
  onTrace?(id: string | undefined): void
}

export function Card({ face, variant, state = 'idle', link, onActivate, onTrace }: CardProps) {
  const shows = SHOWS[variant]
  const stateLabel = STATE_LABELS[state]
  const className = [
    'card',
    `card--${variant}`,
    state !== 'idle' && `card--${state}`,
    link && `card--${link}`,
    shows.board && face.tapped && 'card--tapped',
  ]
    .filter(Boolean)
    .join(' ')

  const body = (
    <>
      <span className="card__band">
        <span className="card__name" title={face.name}>
          {face.name}
        </span>
        {shows.cost && face.manaCost && <span className="card__cost">{face.manaCost}</span>}
      </span>

      {/* Reserved by ADR 0012 and deliberately empty: a procedural placeholder, no bundled or
          remote art, and the layout below it does not move if art ever arrives. Withheld for an
          object with no type line — an emblem, or a bare ability on the stack — because those
          are not printed cards and framing them as one would imply a face that does not exist. */}
      {shows.art && face.typeLine && <span className="card__art" aria-hidden="true" />}

      {shows.typeLine && face.typeLine && (
        <span className="card__type" title={face.typeLine}>
          {face.typeLine}
        </span>
      )}

      {shows.rulesText && face.rulesText && (
        <span className="card__rules" title={face.rulesText}>
          {face.rulesText}
        </span>
      )}

      {shows.keywords && face.keywords.length > 0 && (
        <span className="card__keywords">{face.keywords.join(' · ')}</span>
      )}

      <span className="card__badges">
        {stateLabel && <span className="badge badge--state">{stateLabel}</span>}
        {link && <span className="badge badge--link">{LINK_LABELS[link]}</span>}
        {face.markers.map((marker) => (
          <span key={marker} className="badge badge--marker">
            {marker}
          </span>
        ))}
        {shows.board && face.tapped && <span className="badge badge--tapped">Tapped</span>}
        {shows.board &&
          face.counters.map((counter) => (
            <span key={counter.kind} className="badge badge--counter">
              {counter.count}× {counter.kind}
            </span>
          ))}
        {shows.board && face.damage !== undefined && (
          <span className="badge badge--damage">{face.damage} damage</span>
        )}
        {face.stat && (
          <span className={`card__stat card__stat--${face.stat.kind}`}>
            <span className="visually-hidden">{face.stat.label} </span>
            {face.stat.value}
          </span>
        )}
      </span>
    </>
  )

  // Pointer and keyboard say the same thing, so tracing is not a mouse-only affordance.
  const traced = onTrace && {
    onMouseEnter: () => onTrace(face.id),
    onMouseLeave: () => onTrace(undefined),
    onFocus: () => onTrace(face.id),
    onBlur: () => onTrace(undefined),
  }

  // A button whenever it is clickable, so a keyboard reaches it on the same terms as a mouse and
  // no key handling has to be reinvented. Never `disabled`: the gesture always leads somewhere —
  // at worst to the inspector — and reading an object a player cannot act on is exactly when
  // they most need to.
  if (!onActivate) {
    return (
      <div className={className} {...traced}>
        {body}
      </div>
    )
  }
  return (
    <button type="button" className={className} onClick={() => onActivate(face.id)} {...traced}>
      {body}
    </button>
  )
}
