/**
 * The one piece of motion on the table that is about the game rather than about a card: an object
 * that was not here a message ago arrives instead of appearing fully formed, and a card that was
 * drawn in one zone and is now drawn in another travels between the two.
 *
 * It draws nothing. Everything it touches is already in its final position, drawn by the surface
 * that owns it, and this only plays the last quarter-second of getting there. That is the whole
 * of what keeps an animation from becoming state: interrupt it, refresh mid-flight, or ask the
 * system for no motion at all, and the board is the one the latest `GameView` describes.
 *
 * What counts as an arrival or a flight is `motion.ts`'s answer and never this component's.
 * A flight in particular is a **join the server stated** — `physical_card`, the physical card
 * (CR 108.1) two projections are of — and it claims nothing about identity: under CR 400.7 the
 * two ends are two different objects, and all this does is move a picture between them. Objects
 * are found through the same `data-anchor` the arrows and the object menu
 * find their subjects by, so a surface gets this for free by tagging what it draws, and one that draws nothing for
 * an id simply has nothing to animate.
 *
 * A card *leaving* for nowhere is deliberately not here: it is already gone from the view, so
 * there is no element to move and the honest alternative — holding a ghost of it — would be the
 * client drawing an object the server no longer states. A flight is the case where there *is* a
 * second element, which is why it can be honest.
 */
import { useLayoutEffect, useRef } from 'react'

import type { Changes } from './../../motion'

/** Short enough to be over before a player reaches for the next card, long enough to be seen. */
const DURATION = 220
/** A flight crosses the table rather than settling in place, so it is given a little longer. */
const FLIGHT_DURATION = 320

/** Where every anchored element was, the last time this ran. */
type Boxes = ReadonlyMap<string, DOMRect>

const boxesNow = (): Boxes => {
  const boxes = new Map<string, DOMRect>()
  for (const element of document.querySelectorAll<HTMLElement>('[data-anchor]')) {
    const id = element.dataset.anchor
    if (id !== undefined) boxes.set(id, element.getBoundingClientRect())
  }
  return boxes
}

export function Motion({ changes }: { changes: Changes }) {
  // Every id already played for the current set, so nothing animates twice off one message.
  const played = useRef(new Set<string>())
  const of = useRef(changes)
  // Where things were a message ago. A flight's origin has left the DOM by the time this runs —
  // the new view is already committed — so the only place its position still exists is here.
  const boxes = useRef<Boxes>(new Map())

  // Before paint, not after it. An arrival that started a frame late is a card drawn at full
  // size and then snapped back to nothing, which reads as a glitch rather than as motion.
  useLayoutEffect(() => {
    if (of.current !== changes) {
      played.current = new Set()
      of.current = changes
    }

    const previous = boxes.current
    // Recorded before anything animates, and unconditionally: this is where the *next* message's
    // flights start from, and it has to be the resting layout whether or not motion ran.
    boxes.current = boxesNow()

    // Asked at the moment of animating rather than cached: a player can change the setting
    // mid-game, and the honest answer is always the current one.
    const still = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false

    for (const element of document.querySelectorAll<HTMLElement>('[data-anchor]')) {
      const id = element.dataset.anchor
      if (id === undefined || played.current.has(id)) continue

      const flight = changes.flights.find((candidate) => candidate.to === id)
      const arriving = changes.arrived.has(id)
      if (!flight && !arriving) continue

      // Marked played even when nothing runs. Reaching the same state *instantly* is the
      // reduced-motion request being honoured, and a later render must not decide it is owed.
      played.current.add(id)
      if (still) continue

      // Web Animations rather than a class: it composites off the main thread, it leaves no
      // style behind on the element it finished with, and starting it again on an object that
      // moves twice simply replaces the running one.
      //
      // `translate` and `scale` rather than `transform: translate(…) scale(…)`. An animation on
      // the `transform` property *replaces* whatever transform the surface put on the element
      // for its whole duration, so a card drawn with one would fly from the wrong place and land
      // sideways. The independent properties compose with it instead, which keeps this
      // component's ignorance of what it is moving honest.
      const from = flight && previous.get(flight.from)
      if (flight && from) {
        const to = element.getBoundingClientRect()
        // Both boxes are in viewport coordinates, read in the same frame's layout, so the
        // difference is the distance the card travelled and needs no shared container.
        const dx = from.x + from.width / 2 - (to.x + to.width / 2)
        const dy = from.y + from.height / 2 - (to.y + to.height / 2)
        const ratio = to.width > 0 ? from.width / to.width : 1
        element.animate(
          [
            { translate: `${dx}px ${dy}px`, scale: String(ratio) },
            { translate: '0px 0px', scale: '1' },
          ],
          { duration: FLIGHT_DURATION, easing: 'cubic-bezier(0.3, 0.8, 0.3, 1)' },
        )
        continue
      }

      // An arrival, or a flight whose origin this client never had a box for — a card that
      // travelled while the panel it started in was closed, say. Both are an object appearing
      // where it now is, which is the honest thing to show when there is no journey to draw.
      element.animate(
        [
          { opacity: 0, scale: '0.88' },
          { opacity: 1, scale: '1' },
        ],
        { duration: DURATION, easing: 'cubic-bezier(0.2, 0.7, 0.3, 1)' },
      )
    }
  }, [changes])

  return null
}
