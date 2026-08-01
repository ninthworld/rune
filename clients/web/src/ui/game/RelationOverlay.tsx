/**
 * The lines across the board: an attacker to what it is attacking, a blocker to what it stopped,
 * a spell to what it named.
 *
 * The trail under a card says the same things in words, and it stays — a drawn line is not
 * readable, and replacing the sentence with the picture would take combat away from anybody using
 * a screen reader. This is the second copy, for the reading a player actually does across a
 * table: which of my creatures is that one pointed at, and did anything stop it.
 *
 * **It draws `relations.ts` and nothing else.** Every line is an identifier the server projected,
 * joined once and turned into geometry by `overlay.ts`; there is no path through this component
 * that can produce a line the view did not state. That is the whole reason the two modules are
 * separate — the one that decides *what* is related has no idea where anything is, and the one
 * that knows where everything is cannot decide anything.
 *
 * The measuring is here because it is the only part that needs a browser. The overlay is a sheet
 * over the board that takes no clicks and reads nothing about the game: every box comes from an
 * element the surfaces tagged with the id the *server* gave it, so an object nothing is drawing
 * has no box and no line, and the trail is what still names it.
 */
import { useLayoutEffect, useRef, useState } from 'react'

import { ANCHOR, intersect, overlayEdges, type Rect } from './../../overlay'
import type { Relation, RelationKind } from './../../relations'

/** The kinds an arrowhead is defined for — all of them, since every kind is directed. */
const KINDS: readonly RelationKind[] = ['attacking', 'blocking', 'attached', 'targeting', 'source']

interface Measured {
  width: number
  height: number
  anchors: ReadonlyMap<string, Rect>
}

const NOTHING: Measured = { width: 0, height: 0, anchors: new Map() }

export function RelationOverlay({
  relations,
  traced,
}: {
  /** Every stated relationship in this view, in the order the view listed them. */
  relations: readonly Relation[]
  /** The object the player is looking at, if any. Emphasis only; it selects nothing. */
  traced?: string
}) {
  const ref = useRef<SVGSVGElement>(null)
  const [measured, setMeasured] = useState<Measured>(NOTHING)

  // Re-run on every render rather than against a dependency list. The thing being measured is
  // the laid-out board, and it moves for reasons no prop describes — a new view, a card that
  // finished turning, a pile opening beside the table. Measuring is a read of the DOM the
  // browser has already computed, and the result is compared before it is stored, so a run that
  // finds nothing new ends here instead of rendering again.
  useLayoutEffect(() => {
    const host = ref.current?.parentElement
    if (!host) return

    const measure = () => {
      const frame = host.getBoundingClientRect()
      const anchors = new Map<string, Rect>()
      for (const element of host.querySelectorAll<HTMLElement>(`[${ANCHOR}]`)) {
        const id = element.dataset.entity
        // First one wins, so the answer does not depend on how a surface happens to nest its
        // markup. Nothing draws the same object twice today.
        if (id === undefined || anchors.has(id)) continue
        const box = showing(element, host)
        if (!box) continue
        anchors.set(id, {
          x: box.x - frame.x,
          y: box.y - frame.y,
          width: box.width,
          height: box.height,
        })
      }
      const next: Measured = { width: frame.width, height: frame.height, anchors }
      setMeasured((current) => (same(current, next) ? current : next))
    }

    measure()

    // Everything that moves a card without re-rendering this component: the window, the regions
    // that scroll inside their own areas, and the half second a permanent takes to turn.
    const observer = new ResizeObserver(measure)
    observer.observe(host)
    for (const element of host.querySelectorAll(`[${ANCHOR}]`)) observer.observe(element)
    host.addEventListener('scroll', measure, true)
    host.addEventListener('transitionend', measure, true)
    window.addEventListener('resize', measure)
    return () => {
      observer.disconnect()
      host.removeEventListener('scroll', measure, true)
      host.removeEventListener('transitionend', measure, true)
      window.removeEventListener('resize', measure)
    }
  })

  const edges = overlayEdges(
    relations,
    measured.anchors,
    { x: 0, y: 0, width: measured.width, height: measured.height },
    { traced },
  )

  return (
    // Hidden from assistive technology on purpose: the relationship trails under the cards are
    // the readable copy of every fact drawn here, and a screen reader announcing a hundred
    // unlabelled line elements would be worse than one announcing none.
    <svg ref={ref} className="overlay" aria-hidden="true" focusable="false">
      <defs>
        {KINDS.map((kind) => (
          <marker
            key={kind}
            id={`overlay-head-${kind}`}
            // Sized in the overlay's own units so a head stays the same size whatever the
            // stroke under it is doing, and placed with its tip on the endpoint.
            markerUnits="userSpaceOnUse"
            markerWidth="9"
            markerHeight="9"
            refX="9"
            refY="4.5"
            orient="auto"
          >
            <path className={`overlay__head overlay__head--${kind}`} d="M0,0 L9,4.5 L0,9 Z" />
          </marker>
        ))}
      </defs>

      {edges.map((edge, index) => (
        <line
          key={`${index}:${edge.kind}:${edge.from}:${edge.to}`}
          className={`overlay__edge overlay__edge--${edge.kind} overlay__edge--${edge.emphasis}`}
          // The ids the line joins, on the line. Nothing in the client reads them back; they are
          // what lets a browser test assert that the drawn board connects the objects the
          // fixture stated, without measuring pixels.
          data-kind={edge.kind}
          data-from={edge.from}
          data-to={edge.to}
          x1={edge.x1}
          y1={edge.y1}
          x2={edge.x2}
          y2={edge.y2}
          markerEnd={`url(#overlay-head-${edge.kind})`}
        />
      ))}
    </svg>
  )
}

/**
 * How much of an element is on the screen, in viewport coordinates.
 *
 * Every region of the board scrolls inside its own area, so being laid out and being visible are
 * different things: a permanent below the fold of its own row has a perfectly good box and is not
 * there to be pointed at. Each ancestor that clips its content takes a bite out of the answer,
 * and an element that survives none of them is not an anchor at all — the trail under the card is
 * what still names it.
 */
function showing(element: HTMLElement, host: HTMLElement): Rect | undefined {
  let rect: Rect | undefined = element.getBoundingClientRect()
  for (
    let parent = element.parentElement;
    parent && rect && parent !== host.parentElement;
    parent = parent.parentElement
  ) {
    const style = getComputedStyle(parent)
    if (style.overflowX === 'visible' && style.overflowY === 'visible') continue
    rect = intersect(rect, parent.getBoundingClientRect())
  }
  return rect
}

/** Whether two measurements describe the same board, so an unchanged one renders nothing new. */
function same(current: Measured, next: Measured): boolean {
  if (current.width !== next.width || current.height !== next.height) return false
  if (current.anchors.size !== next.anchors.size) return false
  for (const [id, box] of next.anchors) {
    const was = current.anchors.get(id)
    if (
      !was ||
      was.x !== box.x ||
      was.y !== box.y ||
      was.width !== box.width ||
      was.height !== box.height
    ) {
      return false
    }
  }
  return true
}
