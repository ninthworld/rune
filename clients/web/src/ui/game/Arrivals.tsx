/**
 * The one piece of motion on the table that is about the game rather than about a card: an object
 * that was not here a message ago arrives instead of appearing fully formed.
 *
 * It draws nothing. Everything it touches is already in its final position, drawn by the surface
 * that owns it, and this only plays the last quarter-second of getting there. That is the whole
 * of what keeps an animation from becoming state: interrupt it, refresh mid-flight, or ask the
 * system for no motion at all, and the board is the one the latest `GameView` describes.
 *
 * What counts as an arrival is `motion.ts`'s answer and never this component's — an id in this
 * view that was not in the previous one. Objects are found through the same `data-entity` anchor
 * the drawn relationships and the action list use, so a surface gets this for free by tagging
 * what it draws, and one that draws nothing for an id simply has nothing to animate.
 *
 * A card *leaving* is deliberately not here: it is already gone from the view, so there is no
 * element to move and the honest alternative — holding a ghost of it — would be the client
 * drawing an object the server no longer states. The larger absence is a flight from one zone to
 * another, which the wire cannot express today; `motion.ts` says why.
 */
import { useLayoutEffect, useRef } from 'react'

import { ANCHOR } from './../../overlay'

/** Short enough to be over before a player reaches for the next card, long enough to be seen. */
const DURATION = 220

export function Arrivals({ arrived }: { arrived: ReadonlySet<string> }) {
  // Every id already played for the current set, so nothing arrives twice off one message.
  const played = useRef(new Set<string>())
  const of = useRef(arrived)

  // Before paint, not after it. An arrival that started a frame late is a card drawn at full
  // size and then snapped back to nothing, which reads as a glitch rather than as motion.
  useLayoutEffect(() => {
    if (of.current !== arrived) {
      played.current = new Set()
      of.current = arrived
    }

    // Asked at the moment of animating rather than cached: a player can change the setting
    // mid-game, and the honest answer is always the current one.
    if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) {
      // Still recorded as played. Reaching the same state *instantly* is the request being
      // honoured, and a later render must not decide the arrival is still owed.
      for (const id of arrived) played.current.add(id)
      return
    }

    for (const element of document.querySelectorAll<HTMLElement>(`[${ANCHOR}]`)) {
      const id = element.dataset.entity
      if (id === undefined || !arrived.has(id) || played.current.has(id)) continue
      played.current.add(id)
      // Web Animations rather than a class: it composites off the main thread, it leaves no
      // style behind on the element it finished with, and starting it again on an object that
      // arrives twice simply replaces the running one.
      //
      // `scale` rather than `transform: scale(…)`. A permanent on the board is already
      // transformed — centred in its slot, and turned when it taps — and an animation on the
      // `transform` property *replaces* that for its whole duration, so every card would arrive
      // by sliding half its own width sideways. The independent property composes instead.
      element.animate(
        [
          { opacity: 0, scale: '0.88' },
          { opacity: 1, scale: '1' },
        ],
        { duration: DURATION, easing: 'cubic-bezier(0.2, 0.7, 0.3, 1)' },
      )
    }
  }, [arrived])

  return null
}
