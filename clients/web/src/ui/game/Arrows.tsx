/**
 * Targeting and combat, drawn (`docs/client-design.md` §6.6).
 *
 * A row is a clipped, panning strip, so an arrow cannot live inside one: it would be cut off the
 * moment it left its own seat. Every arrow is therefore drawn on **one fixed overlay above the
 * whole table**, anchored to measured rectangles rather than to the tree. Anything that can take
 * an arrow carries a `data-anchor` — a permanent, a card in hand, a stack object, a seat — and
 * an arrow is drawn only when both of its ends are on screen. An end inside an unopened pile has
 * no box, and an arrow pointing confidently at a card nobody can see is worse than the sentence
 * that still names it.
 *
 * **It carries no fact alone.** Every edge here is also a phrase in `relationLines`, which is
 * what a screen reader gets; this sheet is `aria-hidden` and takes no pointer events, so
 * everything under it stays reachable.
 *
 * The ribbon is built the way the action bar is (§5.5): a deep tinted panel you can see the
 * board through, a brighter keyline around it, and a specular line along its outer edge. It
 * starts wide at the thing acting, narrows as it travels, and flares into a head at the thing
 * being acted on — where the tint runs up to nearly solid, so the end that carries the meaning is
 * the end that holds the light.
 */
import { useLayoutEffect, useRef, useState } from 'react'

import type { Arrow, ArrowTone } from './../../arrows'

type Pt = { x: number; y: number }
type Ring = {
  x: number
  y: number
  w: number
  h: number
  r: number
  tone: ArrowTone
  hit: boolean
}
type Shape = { d: string; sheen: string; a: Pt; b: Pt; tone: ArrowTone }
type Frame = { shapes: Shape[]; rings: Ring[] }

/** The action bar's own palettes: panel, keyline, button, bright text. */
const TONE = {
  target: {
    deep: '#14263e',
    mid: '#24507f',
    lit: '#3a6ca8',
    rim: '#4a7cb8',
    sheen: '#cfe0f4',
    wash: 'rgba(42, 74, 114, 0.30)',
  },
  combat: {
    deep: '#351d1c',
    mid: '#6e3128',
    lit: '#a24d43',
    rim: '#b3594c',
    sheen: '#f0d4ce',
    wash: 'rgba(138, 64, 56, 0.32)',
  },
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))
const r1 = (v: number) => Math.round(v * 10) / 10

/**
 * An anchor inside a strip is only as visible as the strip lets it be — clamping to the strip
 * keeps an arrow pointing at the edge the card was panned past instead of at a card that is not
 * on screen.
 */
function anchorRect(id: string): DOMRect | null {
  const el = document.querySelector(`[data-anchor="${CSS.escape(id)}"]`)
  if (!el) return null
  const r = el.getBoundingClientRect()
  if (r.width === 0 && r.height === 0) return null
  const strip = el.closest('.strip')
  if (!strip) return r
  const s = strip.getBoundingClientRect()
  const x0 = clamp(r.left, s.left, s.right)
  const x1 = clamp(r.right, s.left, s.right)
  const y0 = clamp(r.top, s.top, s.bottom)
  const y1 = clamp(r.bottom, s.top, s.bottom)
  return new DOMRect(x0, y0, x1 - x0, y1 - y0)
}

const centre = (r: DOMRect): Pt => ({ x: r.left + r.width / 2, y: r.top + r.height / 2 })

/** Where a ray leaving the box's centre crosses its edge, padded out. */
function edge(r: DOMRect, toward: Pt, pad: number): Pt {
  const c = centre(r)
  const dx = toward.x - c.x
  const dy = toward.y - c.y
  if (!dx && !dy) return c
  const hw = r.width / 2 + pad
  const hh = r.height / 2 + pad
  const t = Math.min(dx ? hw / Math.abs(dx) : Infinity, dy ? hh / Math.abs(dy) : Infinity)
  return { x: c.x + dx * t, y: c.y + dy * t }
}

/** Every arrow bows the same way, so two sharing a source fan out instead of overlapping. */
const BOW = 0.16

function control(a: Pt, b: Pt): Pt {
  const dx = b.x - a.x
  const dy = b.y - a.y
  const d = Math.hypot(dx, dy) || 1
  const bow = clamp(d * BOW, 14, 110)
  return { x: (a.x + b.x) / 2 - (dy / d) * bow, y: (a.y + b.y) / 2 + (dx / d) * bow }
}

function quad(a: Pt, c: Pt, b: Pt, t: number): Pt {
  const u = 1 - t
  return {
    x: u * u * a.x + 2 * u * t * c.x + t * t * b.x,
    y: u * u * a.y + 2 * u * t * c.y + t * t * b.y,
  }
}

const SAMPLES = 48
const EDGES = 20

/** One point along the curve, and how far along it that point is. */
interface Sample {
  p: Pt
  d: number
}

/** The curve, walked at a fixed number of steps, carrying its own arc length. */
function walk(a: Pt, c: Pt, b: Pt): Sample[] {
  const out: Sample[] = []
  let previous: Pt | undefined
  let d = 0
  for (let i = 0; i <= SAMPLES; i += 1) {
    const p = quad(a, c, b, i / SAMPLES)
    if (previous) d += Math.hypot(p.x - previous.x, p.y - previous.y)
    out.push({ p, d })
    previous = p
  }
  return out
}

/** Where the curve is at a given distance along it, and the normal there. */
function along(samples: readonly Sample[], distance: number): { p: Pt; n: Pt } {
  const first = samples[0]
  if (!first) return { p: { x: 0, y: 0 }, n: { x: 0, y: 0 } }
  let previous = first
  for (const sample of samples.slice(1)) {
    if (sample.d >= distance) {
      const seg = sample.d - previous.d || 1
      const f = (distance - previous.d) / seg
      const tx = (sample.p.x - previous.p.x) / seg
      const ty = (sample.p.y - previous.p.y) / seg
      return {
        p: {
          x: previous.p.x + (sample.p.x - previous.p.x) * f,
          y: previous.p.y + (sample.p.y - previous.p.y) * f,
        },
        n: { x: -ty, y: tx },
      }
    }
    previous = sample
  }
  return { p: previous.p, n: { x: 0, y: 0 } }
}

/**
 * The ribbon: sampled along the curve, offset either side by a width that tapers from tail to
 * head base, then closed by the head's triangle.
 *
 * Two paths come back — the outline, and the outer edge on its own. The second is the sheen:
 * glass reads as glass because one edge catches the light, and the curve always bows the same
 * way, so that edge is always the same one.
 */
function ribbon(
  a: Pt,
  c: Pt,
  b: Pt,
  w0: number,
  w1: number,
  headW: number,
  headLen: number,
): { d: string; sheen: string } {
  const samples = walk(a, c, b)
  const total = samples[samples.length - 1]?.d ?? 0
  if (total < 4) return { d: '', sheen: '' }
  const head = Math.min(headLen, total * 0.55)
  const stem = total - head

  const left: Pt[] = []
  const right: Pt[] = []
  for (let i = 0; i <= EDGES; i += 1) {
    const f = i / EDGES
    const { p, n } = along(samples, stem * f)
    const h = (w0 + (w1 - w0) * f) / 2
    left.push({ x: p.x + n.x * h, y: p.y + n.y * h })
    right.push({ x: p.x - n.x * h, y: p.y - n.y * h })
  }
  const l0 = left[0]
  const r0 = right[0]
  if (!l0 || !r0) return { d: '', sheen: '' }

  const base = along(samples, stem)
  const bl = { x: base.p.x + base.n.x * (headW / 2), y: base.p.y + base.n.y * (headW / 2) }
  const br = { x: base.p.x - base.n.x * (headW / 2), y: base.p.y - base.n.y * (headW / 2) }

  const line = (p: Pt) => `L ${r1(p.x)} ${r1(p.y)}`
  const move = (p: Pt) => `M ${r1(p.x)} ${r1(p.y)}`
  return {
    d: [
      move(r0),
      /* a rounded butt at the tail, so the widest end doesn't read as a cut */
      `A ${r1(w0 / 2)} ${r1(w0 / 2)} 0 0 1 ${r1(l0.x)} ${r1(l0.y)}`,
      ...left.slice(1).map(line),
      line(bl),
      line(b),
      line(br),
      ...right.slice(1).reverse().map(line),
      'Z',
    ].join(' '),
    /* the sheen stops at the head's shoulder: past it the edge is the head's own, and a
       highlight running out to the tip would read as a second, thinner arrow */
    sheen: [move(l0), ...left.slice(1).map(line), line(bl)].join(' '),
  }
}

function build(arrows: readonly Arrow[]): Frame {
  const shapes: Shape[] = []
  const rings = new Map<string, Ring>()
  /* one weight for the whole board: an arrow off a phone-sized card is the same arrow, or it
     stops being visible where it matters most */
  const px = Math.min(window.innerWidth, window.innerHeight)
  const w0 = clamp(px * 0.0115, 7, 15)
  const w1 = w0 * 0.4
  const headW = w0 * 1.8
  const headLen = headW * 1.15

  const ring = (id: string, r: DOMRect, tone: ArrowTone, hit: boolean) => {
    /* a card panned out of its row has no box left to ring — the arrow still leaves from the
       edge it was clamped to */
    if (r.width < 6 || r.height < 6) return
    if (rings.get(id)?.hit) return
    rings.set(id, {
      x: r1(r.left - 2),
      y: r1(r.top - 2),
      w: r1(r.width + 4),
      h: r1(r.height + 4),
      r: r1(clamp(Math.min(r.width, r.height) * 0.09, 3, 10)),
      tone,
      hit,
    })
  }

  for (const arrow of arrows) {
    const from = anchorRect(arrow.from)
    const to = anchorRect(arrow.to)
    if (!from || !to) continue
    const c = control(centre(from), centre(to))
    const a = edge(from, c, 4)
    /* the head stops just clear of the ring rather than on the thing under it — a player's zone
       icons are not what is being targeted */
    const b = edge(to, c, 6)
    const { d, sheen } = ribbon(a, c, b, w0, w1, headW, headLen)
    if (!d) continue
    shapes.push({ d, sheen, a, b, tone: arrow.tone })
    ring(arrow.from, from, arrow.tone, false)
    ring(arrow.to, to, arrow.tone, true)
  }
  return { shapes, rings: [...rings.values()] }
}

/**
 * How long after a change the overlay keeps following the layout, in frames.
 *
 * A ring is measured from a box, and a box can still be **moving** when it is measured: `Motion`
 * plays a flight for 320ms with the Web Animations API, and a card settling into a re-laid-out
 * row moves without changing size. Neither is a render and neither is a resize, so nothing tells
 * this overlay to look again — which is how an outline came to sit at the coordinates a card had
 * a moment ago and stay there until a hover happened to re-render the board (issue #715).
 *
 * Long enough to outlast the longest motion, and it stops early the moment the measurement stops
 * changing, so an idle board does no work.
 */
const SETTLE_FRAMES = 40

/** Frames of an unchanged measurement that count as "the layout has stopped moving". */
const STILL_FRAMES = 3

export function Arrows({ arrows }: { arrows: readonly Arrow[] }) {
  const [frame, setFrame] = useState<Frame>({ shapes: [], rings: [] })
  const seen = useRef('')

  /* re-measured on every render, on anything that can move a rectangle underneath one — a strip
     panned, the window resized, a seat resized — and then on every frame until the layout stops
     moving. The frame is compared before it is set, so the measure the state change triggers
     settles rather than loops. */
  useLayoutEffect(() => {
    let raf = 0
    /** Returns whether this pass found the layout somewhere new. */
    const measure = (): boolean => {
      const next = build(arrows)
      const key = JSON.stringify(next)
      if (key === seen.current) return false
      seen.current = key
      setFrame(next)
      return true
    }
    /* Follow the layout for a bounded run of frames rather than trusting one reading: an
       animation, a re-flowed row, and a font arriving all move a box after the render that
       caused them. Restarted by every measure, so a second change extends the window instead of
       being cut off by the first one's. */
    const follow = () => {
      cancelAnimationFrame(raf)
      let left = SETTLE_FRAMES
      let still = 0
      const step = () => {
        still = measure() ? 0 : still + 1
        left -= 1
        if (left > 0 && still < STILL_FRAMES) raf = requestAnimationFrame(step)
      }
      raf = requestAnimationFrame(step)
    }
    const remeasure = () => {
      measure()
      follow()
    }
    remeasure()
    const observer = new ResizeObserver(remeasure)
    observer.observe(document.documentElement)
    for (const el of document.querySelectorAll('[data-anchor], .strip')) observer.observe(el)
    window.addEventListener('resize', remeasure)
    window.addEventListener('scroll', remeasure, true)
    return () => {
      cancelAnimationFrame(raf)
      observer.disconnect()
      window.removeEventListener('resize', remeasure)
      window.removeEventListener('scroll', remeasure, true)
    }
  })

  if (!frame.shapes.length) return null
  return (
    <svg className="arrows" aria-hidden="true">
      <defs>
        <filter id="arrow-lift" x="-40%" y="-40%" width="180%" height="180%">
          <feDropShadow dx="0" dy="2" stdDeviation="3" floodColor="#05070a" floodOpacity="0.75" />
        </filter>
        {frame.shapes.map((shape, i) => {
          const tone = TONE[shape.tone]
          return (
            <linearGradient
              key={i}
              id={`arrow-${i}`}
              gradientUnits="userSpaceOnUse"
              x1={shape.a.x}
              y1={shape.a.y}
              x2={shape.b.x}
              y2={shape.b.y}
            >
              {/* thin at the tail, where the board should show through; nearly solid at the
                  head, which has to be read */}
              <stop offset="0%" stopColor={tone.deep} stopOpacity="0.52" />
              <stop offset="62%" stopColor={tone.mid} stopOpacity="0.68" />
              <stop offset="100%" stopColor={tone.lit} stopOpacity="0.92" />
            </linearGradient>
          )
        })}
      </defs>

      {frame.rings.map((r, i) => (
        <rect
          key={i}
          className="arrow-ring"
          x={r.x}
          y={r.y}
          width={r.w}
          height={r.h}
          rx={r.r}
          fill={r.hit ? TONE[r.tone].wash : 'none'}
          stroke={TONE[r.tone].rim}
        />
      ))}

      {frame.shapes.map((shape, i) => (
        <g key={i} filter="url(#arrow-lift)">
          {/* the casing sits outside the glass rather than under it: a translucent body over an
              ivory card has no edge of its own */}
          <path className="arrow-cast" d={shape.d} />
          <path
            className="arrow-body"
            d={shape.d}
            fill={`url(#arrow-${i})`}
            stroke={TONE[shape.tone].rim}
          />
          <path className="arrow-sheen" d={shape.sheen} stroke={TONE[shape.tone].sheen} />
        </g>
      ))}
    </svg>
  )
}
