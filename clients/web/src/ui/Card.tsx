/**
 * One card frame, in whatever box it was given.
 *
 * Every card-shaped thing on the screen comes through here: a card in hand, a permanent on the
 * battlefield, an object on the stack, an emblem, a card in an opened pile, and the full face in
 * the preview and the inspector. They share a model (`card-face.ts`) and they share this
 * component, so the board cannot end up disagreeing with the hand about what a counter looks
 * like.
 *
 * **The presentation follows the box, never the caller.** A battlefield card and a hand card of
 * the same size are the same card (`docs/client-design.md` §6), so there is no variant to pass:
 * the surface's own stylesheet gives the frame a box, this measures it, and `fit.ts` — pure, and
 * where the whole policy is tested — answers what there is room to say in it. What is dropped is
 * still one gesture away in the pointer's preview or the inspector, and it is never dropped from
 * assistive technology at all: an abbreviation is visual only.
 *
 * The frame is a real card's anatomy — a name band that owns its row, a cost over the art's
 * corner, an art window that takes what is left, a type line, a text box, and a stat in the
 * corner — at a real card's proportions, because that is what makes a board readable at a glance
 * rather than a list of tiles. The tint under it is `costTint`: the colours of the pips that were
 * printed, and explicitly not colour, colour identity, or anything else the rules define
 * (`mana.ts`). It is a wash under text that says the real thing.
 *
 * No decision about the game is made here. `state` is assigned by the caller from what the
 * server advertised, and dropping a cost from a small tile is a drawing decision that concludes
 * nothing about what is castable — the server states that through `valid_actions`.
 */
import { useCallback, useRef, useState, type CSSProperties, type MouseEvent } from 'react'

import type { CardFace, CardFaceLink, CardFaceState } from './../card-face'
import { cardPlan, type Box, type Presentation } from './../fit'
import { costTint } from './../mana'
import { anchorProps } from './../overlay'
import { useCardArt } from './art'
import { CardArt } from './CardArt'
import { ManaCost } from './Mana'
import { RulesText } from './RulesText'

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

/**
 * What to plan against before the frame has been measured.
 *
 * The designed permanent of §5, which is the shape most cards on a table are. One commit later
 * the real box has been read and the plan is redone; this only decides what a renderer with no
 * layout at all — a server render, a DOM test — draws.
 */
const UNMEASURED: Box = { width: 130, height: 182 }

/**
 * The frame's own box, read from the element the browser laid out.
 *
 * Measuring rather than being told is deliberate for now: the surface already states the box in
 * CSS, and a second copy of those numbers in TypeScript is a copy that goes stale at some zoom
 * level nobody tested. A `ResizeObserver` also makes zoom, resolution, and aspect one problem, as
 * §1 says they are — the card is re-planned by the same path whatever changed the box.
 */
function useMeasuredBox(): [(node: HTMLElement | null) => void, Box | undefined] {
  const [box, setBox] = useState<Box>()
  const observer = useRef<ResizeObserver>(undefined)

  const measure = useCallback((node: HTMLElement | null) => {
    observer.current?.disconnect()
    observer.current = undefined
    if (!node) return
    const read = () => {
      // `offsetWidth`, not `getBoundingClientRect` — the laid-out box rather than the painted
      // one. An object arriving on the table is animated in from a scale of its own
      // (`Motion.tsx`), and its painted rectangle during that quarter second is a fraction of
      // the box it is laid out in: reading that would re-plan the card at every frame of the
      // animation and settle it on a presentation it only briefly deserved.
      const width = node.offsetWidth
      const height = node.offsetHeight
      if (width <= 0 || height <= 0) return
      setBox((previous) =>
        previous && previous.width === width && previous.height === height
          ? previous
          : { width, height },
      )
    }
    read()
    if (typeof ResizeObserver === 'undefined') return
    observer.current = new ResizeObserver(read)
    observer.current.observe(node)
  }, [])

  return [measure, box]
}

export interface CardProps {
  face: CardFace
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
   * Whether the last step of the fitting ladder is available on this surface.
   *
   * `false` in the hand and in the deck builder, where §6 forbids abbreviating a name outright —
   * those are the two places a player *chooses* from what they are reading. This is not a
   * variant by another name: it does not change what the card is or which presentation it gets,
   * it removes one way of degrading it.
   */
  mayAbbreviate?: boolean
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
  state = 'idle',
  link,
  mayAbbreviate = true,
  onActivate,
  onInspect,
  onTrace,
}: CardProps) {
  const [measure, measured] = useMeasuredBox()
  const plan = cardPlan(face, measured ?? UNMEASURED, { mayAbbreviate })
  const stateLabel = STATE_LABELS[state]

  // A player-supplied illustration, if this device has a source and this card has resolved
  // (`ui/art.tsx`). Absent is the normal answer and costs nothing: the frame draws its
  // procedural face, which is what it draws when nothing is turned on at all.
  const art = useCardArt(face)
  // The whole card image as the face (ADR 0012). Only where the window has room to be a card:
  // a chip and an object with no type line have no art window to fill, so they keep SAGE's own
  // text whatever the preference says.
  const imageFace = plan.art && face.typeLine !== undefined && art?.style === 'full'

  // With the picture carrying the printed text, the frame's own type line and text box move to
  // assistive technology rather than being drawn twice.
  const typeLine = imageFace ? '' : plan.typeLine
  const text = imageFace ? undefined : plan.text

  const className = [
    'card',
    `card--${plan.presentation}`,
    `card--tint-${costTint(face.manaCost)}`,
    imageFace && 'card--image',
    state !== 'idle' && `card--${state}`,
    link && `card--${link}`,
    face.tapped && 'card--tapped',
  ]
    .filter(Boolean)
    .join(' ')

  // The name band owns its row and nothing shares it. SAGE ships no card art, so this is the one
  // thing that identifies a card, and the cost — which used to sit beside it and win — is over
  // the art's corner now, where a pip is graphic and reads at sizes text does not.
  const band = (
    <span className={`card__band${imageFace ? ' card__band--over' : ''}`}>
      <span
        className="card__name"
        title={face.name}
        style={nameStyle(plan.name.size, plan.name.lines, plan.presentation)}
        aria-hidden={plan.name.abbreviated || undefined}
      >
        {plan.name.text}
      </span>
      {/* An abbreviation is a drawing, not a fact. Whatever is on screen, this is the name. */}
      {plan.name.abbreviated && <span className="visually-hidden">{face.name}</span>}
    </span>
  )

  // State marks, and the number a player scans a whole board for. Both lie on the picture where
  // there is one — the one surface on a dense frame with room to spare, and the corner the
  // printed stat sits in — and fall back into the flow where there is not.
  const marks = (
    <>
      <span className="card__badges">
        {stateLabel && <span className="badge badge--state">{stateLabel}</span>}
        {link && <span className="badge badge--link">{LINK_LABELS[link]}</span>}
        {face.markers.map((marker) => (
          <span key={marker} className="badge badge--marker">
            {marker}
          </span>
        ))}
        {/* Tapped is the mark drawn across the whole face (`cards.css`), so there is no badge:
            a pill saying it would spend the frame's scarcest room restating what the card
            already looks like. The word is what assistive technology gets, which can perceive
            neither a mark nor a turn. */}
        {face.tapped && <span className="visually-hidden">Tapped</span>}
        {face.counters.map((counter) => (
          <span key={counter.kind} className="badge badge--counter">
            {counter.count}× {counter.kind}
          </span>
        ))}
        {face.damage !== undefined && (
          <span className="badge badge--damage">{face.damage} damage</span>
        )}
      </span>

      {face.stat && (
        <span
          className={`card__stat card__stat--${face.stat.kind}`}
          style={{ fontSize: `${plan.statSize}px` }}
        >
          <span className="visually-hidden">{face.stat.label} </span>
          {face.stat.value}
        </span>
      )}
    </>
  )

  // Withheld for an object with no type line — an emblem, or a bare ability on the stack —
  // because those are not printed cards and framing them as one would imply a face that does not
  // exist. Withheld too when the fitting had nothing left to give it: the art window is the only
  // element that degrades to nothing without costing a fact, which is why it goes last.
  const window = plan.art && face.typeLine !== undefined

  const body = (
    <>
      {band}

      {window && (
        <span className="card__window">
          <CardArt face={face} url={art?.url} />
          {plan.costSize > 0 && (
            <ManaCost
              cost={face.manaCost}
              className="card__cost"
              style={{ fontSize: `${plan.costSize}px` }}
            />
          )}
          {marks}
        </span>
      )}

      {typeLine && (
        <span
          className="card__type"
          title={face.typeLine}
          style={{ fontSize: `${plan.typeSize}px` }}
          aria-hidden={typeLine !== face.typeLine || undefined}
        >
          {typeLine}
        </span>
      )}
      {/* Degraded by rule — `Legendary Creature — Elf Druid` drawn as `Creature` — so the whole
          line is stated where there is no room to draw it. */}
      {face.typeLine && typeLine !== face.typeLine && (
        <span className="visually-hidden">{face.typeLine}</span>
      )}

      {text && (
        <span className="card__text" style={{ fontSize: `${text.size}px` }}>
          {text.rulesText && face.rulesText && (
            <RulesText className="card__rules" text={face.rulesText} />
          )}
          {text.keywords && face.keywords.length > 0 && (
            <span className="card__keywords">{face.keywords.join(' · ')}</span>
          )}
        </span>
      )}
      {/* Never an empty box: where the text did not fit whole, no box is drawn at all and the
          prose is stated here instead. The visible copy is the one that gives way. */}
      {!text?.rulesText && face.rulesText && (
        <span className="visually-hidden">
          <RulesText text={face.rulesText} />
        </span>
      )}
      {!text?.keywords && face.keywords.length > 0 && (
        <span className="visually-hidden">{face.keywords.join(' · ')}</span>
      )}

      {!window && marks}
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
  // Where this object *is*, for the sheet drawn over the board (`overlay.ts`). It carries the
  // server's own id and nothing about the game: a line between two cards is a relationship the
  // view stated, and this is only how the drawing finds the two ends of it.
  const anchor = anchorProps(face.id)

  if (!onActivate) {
    return (
      <div ref={measure} className={className} {...anchor} {...looked} {...inspected}>
        {body}
      </div>
    )
  }
  return (
    <button
      ref={measure}
      type="button"
      className={className}
      onClick={() => onActivate(face.id)}
      {...anchor}
      {...looked}
      {...inspected}
    >
      {body}
    </button>
  )
}

/**
 * The name's own type, and — on a chip alone — the number of lines it was fitted into.
 *
 * The clamp used to be everywhere, on the reasoning that a browser disagreeing with `fit.ts`'s
 * estimate by a hair should lose a pixel of the band. What it actually loses is the **line**: an
 * estimate a percent optimistic makes `Colossal Dreadmaw` wrap where one line was planned, and a
 * one-line clamp then hides the second line entirely — the card reads `Colossal`, which is the
 * `C…` defect wearing different clothes. So a card face lets the band take the line it needs and
 * the art window gives way, which is the trade §6 states outright: art is the one element that
 * degrades to nothing without costing a fact.
 *
 * A chip keeps the clamp, because it has no art window to give and its whole box is one 30px
 * row: there, a second line has nowhere to come from.
 */
const nameStyle = (size: number, lines: number, presentation: Presentation): CSSProperties => ({
  fontSize: `${size}px`,
  ...(presentation === 'chip' ? { maxHeight: `${Math.ceil(size * 1.2) * lines}px` } : {}),
})
