/**
 * Mana pips.
 *
 * The subjects are the game's own conventions — a sun means white, a skull means black, a drop
 * means blue — but every path in here is drawn from scratch in a 100×100 box. Nothing is traced
 * from a printed set, and the marks are deliberately built out of different parts than the
 * printed ones: this sun is a disc with eight tapered rays rather than a ringed starburst, this
 * tree is a lobed canopy on a flared trunk, and the life-payment mark is a pierced heart rather
 * than a glyph. The project ships no official symbol and no downloaded one.
 *
 * The coin wears the chrome's glass (§5.5): a colour body, a gloss falling from the top edge, a
 * lit rim across the top, a dark keyline under it. Both gradients are white- and black-alpha
 * only, so they work over any colour and can be defined once — `<PipDefs/>` is mounted at the
 * app root and every pip on screen points at it.
 *
 * What a pip *is* comes from `mana.ts`, which reads the server's cost string. This file draws
 * whatever it is handed and reads nothing: a symbol this build has never seen is set as its own
 * letters on a plain coin rather than dropped.
 */
import type { ReactNode } from 'react'

const INK = '#131110'

const COIN: Record<string, string> = {
  w: '#f2edd8',
  u: '#a9c7e0',
  b: '#b3aab8',
  r: '#eba382',
  g: '#97c297',
  c: '#b8b3a8',
  /* generic costs, X/Y/Z, and the untinted half of a monocolour hybrid */
  n: '#bdb9ae',
}

const R = 47

/** A point on the coin, in the 100×100 box. */
function at(deg: number, r: number): string {
  const a = (deg * Math.PI) / 180
  return `${(50 + r * Math.cos(a)).toFixed(1)} ${(50 + r * Math.sin(a)).toFixed(1)}`
}

/** A slice of an annulus — the body of the rotation arrows. */
function band(from: number, to: number, ro: number, ri: number): string {
  const big = Math.abs(to - from) > 180 ? 1 : 0
  const dir = to > from ? 1 : 0
  return (
    `M ${at(from, ro)} A ${ro} ${ro} 0 ${big} ${dir} ${at(to, ro)} ` +
    `L ${at(to, ri)} A ${ri} ${ri} 0 ${big} ${dir ? 0 : 1} ${at(from, ri)} Z`
  )
}

/**
 * A rotation arrow: a band with a head on the leading end. Tap turns the card the way you turn
 * it on a table; untap is the same mark reversed.
 */
function turn(cw: boolean): ReactNode {
  const [from, to] = cw ? [165, 55] : [15, 125]
  const tip = cw ? to + 26 : to - 26
  return (
    <>
      <path d={band(from, cw ? to + 360 : to - 360, 40, 26)} />
      <path d={`M ${at(to, 48)} L ${at(tip, 33)} L ${at(to, 18)} Z`} />
    </>
  )
}

const SUN_RAYS = Array.from({ length: 8 }, (_, k) => {
  const a = k * 45
  return `M ${at(a - 17, 20)} L ${at(a, 47)} L ${at(a + 17, 20)} Z`
}).join(' ')

const FLAKE = Array.from({ length: 6 }, (_, k) => {
  const a = k * 60
  return (
    `M ${at(a, 0)} L ${at(a, 44)} M ${at(a, 26)} L ${at(a - 38, 42)} ` +
    `M ${at(a, 26)} L ${at(a + 38, 42)}`
  )
}).join(' ')

/**
 * Each glyph is a fragment of shapes with no fill of its own, so it takes the ink from the group
 * it is dropped into and can be scaled bodily into half a coin for a hybrid.
 */
const GLYPH: Record<string, ReactNode> = {
  /* white — a disc throwing eight rays */
  w: (
    <>
      <circle cx="50" cy="50" r="19" />
      <path d={SUN_RAYS} />
    </>
  ),

  /* blue — a falling drop: a point drawn out of a round belly */
  u: <path d="M 50 11 C 59 31 73 44 73 57 A 23 23 0 0 1 27 57 C 27 44 41 31 50 11 Z" />,

  /* black — a skull: domed cranium, sunken sockets, a squared jaw */
  b: (
    <path
      fillRule="evenodd"
      d={
        'M 22 50 A 28 28 0 0 1 78 50 L 78 60 C 78 67 73 72 66 73 L 66 82 ' +
        'C 66 85 64 87 61 87 L 39 87 C 36 87 34 85 34 82 L 34 73 ' +
        'C 27 72 22 67 22 60 Z ' +
        'M 29 48 a 9 9 0 1 0 18 0 a 9 9 0 1 0 -18 0 Z ' +
        'M 53 48 a 9 9 0 1 0 18 0 a 9 9 0 1 0 -18 0 Z ' +
        'M 50 62 L 56 74 L 44 74 Z'
      }
    />
  ),

  /* red — a flame with one curl licking off its left edge */
  r: (
    <path
      d={
        'M 51 9 C 63 28 77 40 77 57 A 27 27 0 0 1 23 57 ' +
        'C 23 45 31 38 37 27 C 37 41 44 45 48 40 C 52 34 51 20 51 9 Z'
      }
    />
  ),

  /* green — a lobed canopy on a flared trunk */
  g: (
    <>
      <circle cx="34" cy="45" r="17" />
      <circle cx="66" cy="45" r="17" />
      <circle cx="50" cy="31" r="19" />
      <path d="M 43 52 L 57 52 L 55 80 C 60 82 64 84 66 88 L 34 88 C 36 84 40 82 45 80 Z" />
    </>
  ),

  /* colourless — a cut stone */
  c: <path d="M 50 12 L 84 50 L 50 88 L 16 50 Z" />,

  /* snow — six spokes, each barbed */
  s: <path d={FLAKE} fill="none" stroke="currentColor" strokeWidth="9" strokeLinecap="round" />,

  /* the life payment: a heart run through */
  p: (
    <path
      fillRule="evenodd"
      d={
        'M 50 85 C 25 66 17 55 17 43 A 16 16 0 0 1 50 35 ' +
        'A 16 16 0 0 1 83 43 C 83 55 75 66 50 85 Z ' +
        'M 45 14 L 55 14 L 55 92 L 45 92 Z'
      }
    />
  ),

  t: turn(true),
  q: turn(false),
}

/** What a pip is, once its symbol has been read. */
type Spec =
  | { kind: 'solid'; coin: string; glyph?: ReactNode; text?: string; ink?: string }
  | { kind: 'split'; a: Spec; b: Spec }

/** A coin colour by key, falling back to the untinted one for anything unrecognised. */
const coin = (key: string): string => COIN[key] ?? '#bdb9ae'

const textPip = (key: string, text: string): Spec => ({ kind: 'solid', coin: coin(key), text })

function read(symbol: string): Spec {
  const s = symbol.trim()
  /* hybrid and phyrexian both arrive as A/B */
  if (s.includes('/')) {
    const [left = '', right = ''] = s.split('/')
    if (right.toLowerCase() === 'p') {
      return { kind: 'solid', coin: coin(left.toLowerCase()), glyph: GLYPH.p }
    }
    return { kind: 'split', a: read(left), b: read(right) }
  }
  const key = s.toLowerCase()
  /* untap is tap's mark on an inverted coin — mirroring alone is invisible at the 10px a cost
     line gives it */
  if (key === 'q') return { kind: 'solid', coin: '#2f2b26', glyph: GLYPH.q, ink: '#efe9da' }
  const glyph = GLYPH[key]
  /* tap, untap and snow are marks on an untinted coin; everything else that has a glyph has a
     colour to go with it */
  if (glyph) return { kind: 'solid', coin: coin(key), glyph }
  return textPip('n', s)
}

/**
 * Numerals shrink to fit rather than overflowing the coin.
 *
 * Set as large as the coin will take, which is larger than it looks: the glyphs a pip draws are
 * pictures and read at any size, but a generic cost is a *number a player adds up*, and at a
 * board card's 12px coin the old sizing left it barely legible. The floor is the width of the
 * coin's flat middle band, not the circle — three digits are set to clear the rim.
 */
function textSize(text: string): number {
  if (text.length >= 3) return 46
  if (text.length === 2) return 60
  return 74
}

const coinOf = (spec: Spec): string => (spec.kind === 'split' ? coinOf(spec.b) : spec.coin)

function Face({ spec, cx, cy, k }: { spec: Spec; cx: number; cy: number; k: number }) {
  if (spec.kind === 'split') return null
  return (
    <g
      transform={`translate(${cx} ${cy}) scale(${k}) translate(-50 -50)`}
      style={{ color: spec.ink ?? INK }}
      fill="currentColor"
    >
      {spec.glyph}
      {spec.text && (
        <text
          x="50"
          y="50"
          textAnchor="middle"
          dominantBaseline="central"
          fontFamily="system-ui, sans-serif"
          fontWeight="700"
          fontSize={textSize(spec.text)}
        >
          {spec.text}
        </text>
      )}
    </g>
  )
}

/**
 * One pip.
 *
 * `symbol` is the contents of the braces the server printed — `2`, `G`, `W/U`, `T` — which is
 * exactly `ManaSymbol.glyph`. `label` is what assistive technology is told instead of the
 * drawing, because a disc with a letter in it says nothing when read aloud.
 */
export function Pip({
  symbol,
  label,
  inline,
}: {
  symbol: string
  label?: string
  inline?: boolean
}) {
  const spec = read(symbol)
  const split = spec.kind === 'split'
  return (
    <svg
      className={`c-pip${inline ? ' c-pip-inline' : ''}`}
      viewBox="0 0 100 100"
      role="img"
      aria-label={label ?? symbol}
    >
      {/* the coin: one colour, or two cut along the diagonal so each half keeps the corner its
          glyph sits in */}
      <circle cx="50" cy="50" r={R} fill={coinOf(spec)} />
      {split && (
        <path d={`M ${at(135, R)} A ${R} ${R} 0 0 1 ${at(315, R)} Z`} fill={coinOf(spec.a)} />
      )}

      {split ? (
        <>
          <Face spec={spec.a} cx={34} cy={34} k={0.46} />
          <Face spec={spec.b} cx={66} cy={66} k={0.46} />
        </>
      ) : (
        <Face spec={spec} cx={50} cy={50} k={0.82} />
      )}

      {/* the glass, over everything: gloss falling from the top edge, a lit rim across it, and
          the keyline that holds the coin against a light card face as well as a dark one */}
      <circle cx="50" cy="50" r={R} fill="url(#sage-pip-gloss)" />
      <circle cx="50" cy="50" r={R - 1.5} fill="none" stroke="url(#sage-pip-rim)" strokeWidth="3" />
      <circle cx="50" cy="50" r={R} fill="none" stroke="rgba(0,0,0,0.5)" strokeWidth="4" />
    </svg>
  )
}

/**
 * Mounted once, at the root. Both gradients are colour-free, so every pip on the board shares
 * these two rather than carrying its own.
 */
export function PipDefs() {
  return (
    <svg className="pip-defs" aria-hidden="true">
      <defs>
        <linearGradient id="sage-pip-gloss" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="rgba(255,255,255,0.42)" />
          <stop offset="34%" stopColor="rgba(255,255,255,0.08)" />
          <stop offset="56%" stopColor="rgba(255,255,255,0)" />
          <stop offset="100%" stopColor="rgba(0,0,0,0.20)" />
        </linearGradient>
        <linearGradient id="sage-pip-rim" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="rgba(255,255,255,0.65)" />
          <stop offset="40%" stopColor="rgba(255,255,255,0)" />
        </linearGradient>
      </defs>
    </svg>
  )
}
