/**
 * The card: one drawing, in its own 207×291 grid, at every size it is ever asked for.
 *
 * `docs/client-design.md` §6. There is **one presentation** — a card in a hand, a permanent on a
 * battlefield, a thumbnail beside a stack item, and the preview at the side of the board are the
 * same SVG scaled, and the surface's stylesheet is what states the box. Nothing here reads how
 * big it ended up on screen, and there is no variant for a caller to pass.
 *
 * That works because every run of text is fitted *in the card's own grid*: inside each
 * `foreignObject` one CSS pixel is one reference pixel, so the same "9px" line is 18 device
 * pixels in the preview and 3 on a board card. Each run picks the largest size that fits its box
 * by bisection — the title and type line only ever shrink, because they must clear the mana pips
 * and their bars are a fixed height, while the rules text has no design size at all and is
 * simply set as large as its box will take. Two words of reminder text therefore fill the same
 * field a paragraph needs, and no card carries a half-empty text box.
 *
 * The frame is drawn under the same light as the chrome (§5.5): what is raised off the ivory
 * slab — the title bar, the type bar, the stat plaque — is lit along its top edge, and what is
 * sunk into it — the art window, the text field — is shadowed along the same one.
 *
 * **Everything drawn here was stated by the server.** The face comes from `card-face.ts`, the
 * pips from the cost string `mana.ts` tokenized, and the counters, damage and markers from the
 * view. This file decides no rule: the tint is the colours of the printed pips and nothing more
 * (`mana.ts`), and a card with no rules text is drawn with an empty field rather than a
 * placeholder, because the client cannot tell "no abilities" from "not sent".
 */
import {
  useId,
  useLayoutEffect,
  useRef,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from 'react'

import type { CardFace, CardFaceLink, CardFaceState } from './../../card-face'
import { faceSummary, keywordLine, loyaltyCost } from './../../card-face'
import { inlineSymbols, spokenSymbol } from './../../mana'
import { useCardArt } from './../art'
import { fit, tooBig, tooWide } from './../fit'
import { barPath, TITLE } from './bars'
import { Loyalty } from './Loyalty'
import { Pip } from './Pips'
import { tintClass } from './tint'
import { TitleBand } from './TitleBand'
import { PEEK_MS, PEEK_SLOP, usePeek } from './peek'

/**
 * Rules text: paragraphs split on newlines, `{X}` tokens drawn as the pips they name, a line of
 * keywords set bold, and a loyalty ability led by the symbol its cost is printed in — the wording
 * is the server's, byte for byte, and only the parts it writes as symbols change (`mana.ts`,
 * `card-face.ts`).
 */
function RulesText({ text, keywords }: { text: string; keywords: readonly string[] }) {
  return (
    <>
      {text.split('\n').map((para, index) => {
        const loyalty = loyaltyCost(para)
        const body = loyalty?.rest ?? para
        return (
          <p key={index} className={keywordLine(body, keywords) ? 'c-kw' : undefined}>
            {loyalty && <Loyalty cost={loyalty.cost} />}
            {inlineSymbols(body).map((token, i) =>
              token.kind === 'symbol' ? (
                <Pip
                  key={i}
                  symbol={token.symbol.glyph}
                  label={spokenSymbol(token.symbol)}
                  inline
                />
              ) : (
                <span key={i}>{token.text}</span>
              ),
            )}
          </p>
        )
      })}
    </>
  )
}

/**
 * The one dial: the point past which body text stops being body text, for the card that has two
 * words to say. Everything else fits between a floor and its own ceiling.
 */
const RULES_MAX = 22
/** The stat is the number the game is played on, so it takes the whole plaque until it cannot. */
const PT_SIZE = 20

/**
 * The ivory slab: gently rounded on top, rounded bottom corners ending high above the card
 * bottom; the text box overhangs it into the dark.
 */
const SLAB =
  'M 9 7 H 197 A 2 2 0 0 1 199 9 V 250 A 8 8 0 0 1 191 258 ' +
  'H 15 A 8 8 0 0 1 7 250 V 9 A 2 2 0 0 1 9 7 Z'

const TYPE = barPath(183, 17, 2.5)
const PT_OUTER = barPath(63, 30, 3)
const PT_INNER = barPath(58, 25, 2.5)

export function Card({
  face,
  style,
  anchor,
  state,
  link,
  onTrace,
  onActivate,
  onInspect,
  note,
  /** Drawn over whatever face is underneath: true of the permanent, not printed on the card. */
  overlay = true,
}: {
  face: CardFace
  style?: CSSProperties
  /** What a targeting arrow aims at, when this card is one of its ends. */
  anchor?: string
  state?: CardFaceState
  link?: CardFaceLink
  onTrace?(id: string | undefined): void
  onActivate?(id: string): void
  onInspect?(id: string): void
  /**
   * What the server related this object to, in words (`relations.relationNote`).
   *
   * The readable copy of every line the overlay draws over this card: an arrow is hidden from
   * assistive technology, so the fact it carries has to be reachable here as well.
   */
  note?: string
  overlay?: boolean
}) {
  const uid = useId()
  const peek = usePeek()
  const art = useCardArt(face)
  const held = useRef<{ timer: number; x: number; y: number } | null>(null)

  // Holding a card opens it big enough to read. The pointer's hover answers this on a desktop,
  // but a finger has no hover — so the gesture is a press, and it is the same one everywhere.
  const startHold = (event: ReactPointerEvent) => {
    if (!peek || event.button !== 0) return
    held.current = {
      timer: window.setTimeout(() => peek(face), PEEK_MS),
      x: event.clientX,
      y: event.clientY,
    }
  }
  const endHold = () => {
    if (held.current) window.clearTimeout(held.current.timer)
    held.current = null
  }
  const moveHold = (event: ReactPointerEvent) => {
    const at = held.current
    if (!at) return
    if (Math.hypot(event.clientX - at.x, event.clientY - at.y) > PEEK_SLOP) endHold()
  }

  // The name is fitted by the band that owns its box; these are the rest of the card's runs.
  const typeRef = useRef<HTMLSpanElement>(null)
  const textRef = useRef<HTMLDivElement>(null)
  const ptRef = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    fit(typeRef, 8.5, 5.5, tooWide)
    fit(textRef, RULES_MAX, 4.5, tooBig)
    fit(ptRef, PT_SIZE, 7, tooWide)
  })

  const interactive = onActivate !== undefined
  const shell = {
    className: [
      'card',
      tintClass(face),
      state && state !== 'idle' ? `card-${state}` : '',
      link ? `card-${link}` : '',
      interactive ? 'card-live' : '',
      // CR 302.6, drawn: the one board fact a player checks on every creature every turn and
      // which nothing else on the card shows. It is a wash rather than a badge because it is
      // true of the whole permanent, and the same fact is in the accessible name.
      overlay && face.summoningSick ? 'card-sick' : '',
    ]
      .filter(Boolean)
      .join(' '),
    viewBox: '0 0 207 291',
    style,
    'data-anchor': anchor,
    'data-card': face.id,
    role: interactive ? 'button' : 'img',
    tabIndex: interactive ? 0 : undefined,
    'aria-label': note ? `${faceSummary(face)} · ${note}` : faceSummary(face),
    'aria-pressed': state === 'selected' ? true : undefined,
    onMouseEnter: onTrace && (() => onTrace(face.id)),
    onMouseLeave: onTrace && (() => onTrace(undefined)),
    onClick: onActivate && (() => onActivate(face.id)),
    onKeyDown:
      onActivate &&
      ((event: React.KeyboardEvent) => {
        if (event.key !== 'Enter' && event.key !== ' ') return
        event.preventDefault()
        onActivate(face.id)
      }),
    onPointerDown: startHold,
    onPointerMove: moveHold,
    onPointerUp: endHold,
    onPointerCancel: endHold,
    onPointerLeave: endHold,
    // Right-click reads. It is the one gesture with no keyboard equivalent, which is why the
    // click is left free to act.
    onContextMenu: (event: ReactMouseEvent) => {
      if (!onInspect && !peek) return
      event.preventDefault()
      if (onInspect) onInspect(face.id)
      else if (peek) peek(face)
    },
  }

  /* What is on the permanent rather than printed on the card. There are a great many kinds of
     counter and no chance of drawing a mark for each, so a counter is a label and a count in a
     plain dark pill — one shape any kind fits, named the way the server names it. Damage and the
     server's markers wear the same pill, because they are the same kind of fact.

     A **granted** keyword joins them only on a printed face (below), where SAGE draws no rules
     box of its own and there is nowhere else for the word to go. On our own frame it is set in
     the text field with the rest of the card's abilities, which is where a player reads what a
     creature can do. */
  const pills = [
    // That there is another side (CR 712, §6.7): one glyph in the run of state marks, saying
    // *that* a card has a second face and never what is on it. Two faces in one box is two
    // cards' worth of text in one card's grid, so the board draws the face that is up and the
    // pinned preview is where the card turns over. The word is in the accessible name
    // (`faceSummary`), because a glyph is not readable.
    ...(face.otherFace ? [{ key: 'other-face', text: '⇄', count: null }] : []),
    ...(art?.style === 'full'
      ? face.grantedKeywords.map((keyword) => ({
          key: `granted:${keyword}`,
          text: keyword,
          count: null,
        }))
      : []),
    ...face.counters.map((counter) => ({
      key: `counter:${counter.kind}`,
      text: counter.kind,
      count: counter.count > 1 ? `×${counter.count}` : null,
    })),
    ...(face.damage === undefined
      ? []
      : [{ key: 'damage', text: 'damage', count: `${face.damage}` }]),
    ...face.markers.map((marker) => ({ key: `marker:${marker}`, text: marker, count: null })),
  ]

  const overlays = overlay && pills.length > 0 && (
    <foreignObject x="18" y="133" width="170" height="24">
      <div className="c-counters">
        {pills.map((pill) => (
          <span key={pill.key} className="c-ct">
            {pill.text}
            {pill.count && <b>{pill.count}</b>}
          </span>
        ))}
      </div>
    </foreignObject>
  )

  /* The **stat**, as an overlay rather than as part of the frame.
     A `full` face is the printed card, and the printed card says 2/2 about a creature the
     server just told us is a 4/4. Everything the server computed therefore rides over the
     image — the plaque as well as the pills — because a printed number standing in for a
     current one is a *wrong* board, not a plainer one (`art/settings.ts`). It is drawn where
     the printed plaque is, so it covers the number it is correcting instead of sitting beside
     it and offering a player two answers. */
  const statPlaque = face.stat && (
    <g transform="translate(140 250)">
      <path d={PT_OUTER} style={{ fill: 'var(--accent)' }} filter={`url(#${uid}-sh)`} />
      <g transform="translate(2.5 2.5)">
        <path
          d={PT_INNER}
          strokeWidth="1.2"
          style={{ fill: `url(#${uid}-chip)`, stroke: 'var(--key)' }}
        />
        <foreignObject x="3" y="0" width="52" height="25">
          <div ref={ptRef} className="c-pt-num">
            {face.stat.value}
          </div>
        </foreignObject>
      </g>
    </g>
  )

  /* The printed card, whole: none of SAGE's frame is drawn, because none of it is ours in this
     view (§6, and ADR 0012's opt-in pipeline). What still rides over it is everything the
     *server computed* — the stat, counters, damage, markers — plus the name band, which is the
     one piece of SAGE's own frame a player may ask for back (§9.6). */
  if (art?.style === 'full') {
    return (
      <svg {...shell}>
        <defs>
          <clipPath id={`${uid}-round`}>
            <rect x="0" y="0" width="207" height="291" rx="7" />
          </clipPath>
          <clipPath id={`${uid}-tb`}>
            <path d={TITLE} />
          </clipPath>
          <filter id={`${uid}-sh`} x="-30%" y="-30%" width="160%" height="160%">
            <feDropShadow dx="0" dy="1.5" stdDeviation="1.5" floodOpacity="0.55" />
          </filter>
          <linearGradient id={`${uid}-bar`} x1="0" y1="0" x2="0" y2="1">
            <stop
              offset="0%"
              style={{ stopColor: 'color-mix(in srgb, var(--title-b) 84%, #ffffff)' }}
            />
            <stop offset="55%" style={{ stopColor: 'var(--title-b)' }} />
            <stop
              offset="100%"
              style={{ stopColor: 'color-mix(in srgb, var(--title-b) 91%, #000000)' }}
            />
          </linearGradient>
          <linearGradient id={`${uid}-chip`} x1="0" y1="0" x2="0" y2="1">
            <stop
              offset="0%"
              style={{ stopColor: 'color-mix(in srgb, var(--field) 86%, #ffffff)' }}
            />
            <stop
              offset="100%"
              style={{ stopColor: 'color-mix(in srgb, var(--field) 93%, #000000)' }}
            />
          </linearGradient>
        </defs>
        <image
          href={art.url}
          x="0"
          y="0"
          width="207"
          height="291"
          preserveAspectRatio="xMidYMid slice"
          clipPath={`url(#${uid}-round)`}
        />
        {art.band && (
          <g filter={`url(#${uid}-sh)`}>
            <TitleBand face={face} />
          </g>
        )}
        {overlay && statPlaque}
        {overlays}
      </svg>
    )
  }

  return (
    <svg {...shell}>
      <defs>
        <linearGradient id={`${uid}-slab`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" style={{ stopColor: 'var(--panel)' }} />
          <stop offset="60%" style={{ stopColor: 'var(--panel)' }} />
          <stop offset="100%" style={{ stopColor: 'var(--panel-deep)' }} />
        </linearGradient>
        <clipPath id={`${uid}-tb`}>
          <path d={TITLE} />
        </clipPath>
        <clipPath id={`${uid}-ty`}>
          <path d={TYPE} />
        </clipPath>
        <filter id={`${uid}-sh`} x="-30%" y="-30%" width="160%" height="160%">
          <feDropShadow dx="0" dy="1.5" stdDeviation="1.5" floodOpacity="0.55" />
        </filter>

        <linearGradient id={`${uid}-bar`} x1="0" y1="0" x2="0" y2="1">
          <stop
            offset="0%"
            style={{ stopColor: 'color-mix(in srgb, var(--title-b) 84%, #ffffff)' }}
          />
          <stop offset="55%" style={{ stopColor: 'var(--title-b)' }} />
          <stop
            offset="100%"
            style={{ stopColor: 'color-mix(in srgb, var(--title-b) 91%, #000000)' }}
          />
        </linearGradient>
        <linearGradient id={`${uid}-chip`} x1="0" y1="0" x2="0" y2="1">
          <stop
            offset="0%"
            style={{ stopColor: 'color-mix(in srgb, var(--field) 86%, #ffffff)' }}
          />
          <stop
            offset="100%"
            style={{ stopColor: 'color-mix(in srgb, var(--field) 93%, #000000)' }}
          />
        </linearGradient>
        <linearGradient id={`${uid}-rim`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="rgba(255, 255, 255, 0.30)" />
          <stop offset="34%" stopColor="rgba(255, 255, 255, 0.04)" />
          <stop offset="100%" stopColor="rgba(255, 255, 255, 0)" />
        </linearGradient>
        <linearGradient id={`${uid}-recess`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="rgba(0, 0, 0, 0.5)" />
          <stop offset="20%" stopColor="rgba(0, 0, 0, 0)" />
        </linearGradient>
        {/* one raking highlight across the whole face, so the parts read as one sheet of glass
            rather than five lit pieces */}
        <linearGradient id={`${uid}-sheen`} x1="0" y1="0" x2="0.85" y2="1">
          <stop offset="0%" stopColor="rgba(255, 255, 255, 0.10)" />
          <stop offset="40%" stopColor="rgba(255, 255, 255, 0.02)" />
          <stop offset="100%" stopColor="rgba(255, 255, 255, 0)" />
        </linearGradient>
      </defs>

      {/* black card and slab; the area below the slab is the plain border black */}
      <rect x="0" y="0" width="207" height="291" rx="7" style={{ fill: 'var(--bg)' }} />
      <path d={SLAB} fill={`url(#${uid}-slab)`} />

      {/* art window: flush under the title bar, type bar sitting on its foot */}
      <rect x="14" y="28" width="179" height="132.5" style={{ fill: 'var(--key)' }} />
      <rect
        x="15"
        y="28"
        width="177"
        height="131.5"
        style={{ fill: 'color-mix(in srgb, var(--f2) 30%, #23262b)' }}
      />
      {/* the picture, when a player has supplied one — cropped to fill the window rather than
          letterboxed inside it */}
      {art && (
        <image
          href={art.url}
          x="15"
          y="28"
          width="177"
          height="131.5"
          preserveAspectRatio="xMidYMid slice"
        />
      )}
      {/* the art is sunk into the card, so it takes the shadow of the title bar above it */}
      <rect x="15" y="28" width="177" height="131.5" fill={`url(#${uid}-recess)`} />

      {/* rules text field: sharp box, tucked under the type bar at the same remove the type bar
          keeps from the art, its lower third overhanging the slab into the dark well. On a
          creature the text stops short of the stat plaque rather than running under it. */}
      <rect x="13" y="179" width="181" height="92" style={{ fill: 'var(--accent)' }} />
      <rect
        x="14.5"
        y="180.5"
        width="178"
        height="89"
        strokeWidth="1"
        style={{ fill: 'var(--field)', stroke: 'var(--key)' }}
      />
      {/* the same shadow on the text field, at a third the strength — any more and it dirties
          the ivory the rules text has to read off */}
      <rect
        x="14.5"
        y="180.5"
        width="178"
        height="89"
        fill={`url(#${uid}-recess)`}
        opacity="0.34"
      />
      <foreignObject x="15" y="182" width="177" height={face.stat ? 72 : 86}>
        <div className="c-text" ref={textRef}>
          {face.rulesText !== undefined && (
            <RulesText text={face.rulesText} keywords={face.keywords} />
          )}
          {/* What this object has that its card never printed — the trample a pump gave it for
              the turn, an Aura's flying (`docs/protocol.md`). It goes in the text field because
              that is where a player reads what a creature can do, and it is marked as added
              rather than dressed up as printed: the card did not say this, the board does. It is
              fitted with the rest of the text, so a card that gains one shrinks its box to hold
              it instead of hiding it. */}
          {face.grantedKeywords.length > 0 && (
            <p className="c-kw c-granted">{face.grantedKeywords.join(', ')}</p>
          )}
        </div>
      </foreignObject>

      {/* type bar: black top edge overlapping the art's foot */}
      <g transform="translate(12 162)">
        <path
          d={TYPE}
          strokeWidth="3.5"
          style={{ fill: 'var(--accent)', stroke: 'var(--accent)' }}
        />
        <path d={TYPE} fill={`url(#${uid}-bar)`} />
        <g clipPath={`url(#${uid}-ty)`}>
          <rect x="0" y="16" width="183" height="1" style={{ fill: 'var(--key-soft)' }} />
        </g>
        <path d={TYPE} fill="none" strokeWidth="1" style={{ stroke: 'var(--key)' }} />
        <foreignObject x="6" y="2" width="171" height="13.5">
          <div className="c-type-row">
            <span className="c-typeline" ref={typeRef}>
              {face.typeLine ?? ''}
            </span>
          </div>
        </foreignObject>
      </g>

      {/* title bar: sits on the slab with a sliver of it showing above. The cost keeps its
          constant width and the name is fitted into what is left, because a name that pushed the
          pips off the bar would drop the one thing on the card that is not text. */}
      <TitleBand face={face} edged />

      {/* the stat plaque: same bulged-end construction — white outer bar, dark keyline, ivory
          fill. Power and toughness, or a planeswalker's loyalty; both are the server's number,
          which is why the same plaque is what a full-art face wears over its printed one. */}
      {face.stat && <g transform="translate(2 5)">{statPlaque}</g>}

      {/* last, so they lie over every part: one sheet of glass across the face, and the lit edge
          the chrome's panes all carry */}
      <rect x="0" y="0" width="207" height="291" rx="7" fill={`url(#${uid}-sheen)`} />
      <rect
        x="0.6"
        y="0.6"
        width="205.8"
        height="289.8"
        rx="6.6"
        fill="none"
        strokeWidth="1.2"
        stroke={`url(#${uid}-rim)`}
      />

      {overlays}
    </svg>
  )
}
