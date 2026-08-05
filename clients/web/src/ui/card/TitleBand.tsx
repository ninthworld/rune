/**
 * The title bar: the name, fitted, and the printed cost as pips, on the bulged ivory bar §6 puts
 * them on.
 *
 * **One drawing, three callers** — the frame draws it, a full-art face draws it as the band a
 * player may ask back (§9.6), and the deck builder's title list is nothing but it. The name is
 * fitted here rather than by whoever placed the bar, because the box it has to fit in is the
 * bar's, not the caller's.
 */
import { useId, useLayoutEffect, useRef } from 'react'

import type { CardFace } from './../../card-face'
import { manaSymbols, spokenSymbol } from './../../mana'
import { fit, tooWide } from './../fit'
import { TITLE, TITLE_H, TITLE_W, TITLE_X, TITLE_Y } from './bars'
import { Pip } from './Pips'
import { tintClass } from './tint'

/**
 * The band, placed where the card's grid puts it. `edged` is the accent the printed frame carries
 * around it; over art the caller casts a shadow instead, which is the same edge in a view that
 * has no slab to sit on.
 */
export function TitleBand({ face, edged }: { face: CardFace; edged?: boolean }) {
  const uid = useId()
  const nameRef = useRef<HTMLSpanElement>(null)
  useLayoutEffect(() => {
    fit(nameRef, 10, 6, tooWide)
  })

  return (
    <g transform={`translate(${TITLE_X} ${TITLE_Y})`}>
      <defs>
        <clipPath id={`${uid}-tb`}>
          <path d={TITLE} />
        </clipPath>
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
      </defs>

      {edged && (
        <path
          d={TITLE}
          strokeWidth="3.5"
          style={{ fill: 'var(--accent)', stroke: 'var(--accent)' }}
        />
      )}
      <path d={TITLE} fill={`url(#${uid}-bar)`} />
      <g clipPath={`url(#${uid}-tb)`}>
        <rect x="0" y="15.5" width={TITLE_W} height="1" style={{ fill: 'var(--key-soft)' }} />
      </g>
      <path d={TITLE} fill="none" strokeWidth="1" style={{ stroke: 'var(--key)' }} />
      <foreignObject x="5" y="1.5" width="173" height="13.5">
        <div className="c-title-row">
          <span className="c-name" ref={nameRef}>
            {face.name}
          </span>
          <span className="c-cost">
            {manaSymbols(face.manaCost).map((symbol, i) => (
              <Pip key={i} symbol={symbol.glyph} label={spokenSymbol(symbol)} />
            ))}
          </span>
        </div>
      </foreignObject>
    </g>
  )
}

/**
 * The same band on its own, cropped to itself — a card's title where no card is drawn. It washes
 * itself in the face's own tint, because that is the one thing a caller cannot be trusted to
 * compose (`tint.ts`).
 */
export function CardTitle({ face }: { face: CardFace }) {
  return (
    <svg
      className={`c-band ${tintClass(face)}`}
      viewBox={`${TITLE_X - 1} ${TITLE_Y - 1} ${TITLE_W + 2} ${TITLE_H + 2}`}
      role="img"
      aria-label={face.name}
    >
      <TitleBand face={face} edged />
    </svg>
  )
}
