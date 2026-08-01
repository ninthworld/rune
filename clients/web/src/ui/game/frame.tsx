/**
 * The viewport, measured once, and a region's box drawn from it.
 *
 * The table used to be a document: a grid of `auto` tracks whose heights were whatever the
 * content in them asked for, with `overflow-y: auto` underneath to absorb the difference. That
 * is the substrate `docs/client-design.md` §5 rejects — "a region's position is computed, never
 * the residue of what came before it" — and this module is the seam where the rejection is
 * implemented. `scene.ts` decides every box from two numbers; everything here does is measure
 * those two numbers and turn the answer into `left/top/width/height`.
 *
 * **Measured in one place.** One `ResizeObserver`, on the document's root element, feeding one
 * `scene()` call per render. No surface asks the browser how much room it has and no surface is
 * allowed to claim room by being large — a region is handed a box and draws inside it, which is
 * what makes zoom, resolution, and aspect the single problem §1 says they are: all three change
 * the same two numbers, and nothing else in the client has to know which one moved.
 *
 * **Derived, never stored.** The viewport is browser state, not game state; it is read every
 * render from the element the browser laid out, and a refresh mid-game produces the same boxes
 * from the same window. Nothing here crosses a message boundary.
 */
import { useEffect, useState, type CSSProperties, type ReactNode } from 'react'

import type { Rect, Viewport } from './../../scene'

/**
 * The layout viewport, in CSS px.
 *
 * `documentElement.clientWidth/Height` rather than `innerWidth/Height`: the former is the initial
 * containing block and excludes a scrollbar the latter counts, and a two-pixel disagreement is a
 * region hanging two pixels off the bottom of every screen that has one. At 200% zoom a 1280×720
 * screen reports 640×360 here, which is the whole of why zoom needs no separate answer.
 */
const measure = (): Viewport =>
  typeof document === 'undefined'
    ? { width: 0, height: 0 }
    : {
        width: document.documentElement.clientWidth,
        height: document.documentElement.clientHeight,
      }

/** The viewport, and a new one whenever the window, the zoom, or the device orientation moves. */
export function useViewport(): Viewport {
  const [viewport, setViewport] = useState(measure)

  useEffect(() => {
    const read = () =>
      setViewport((current) => {
        const next = measure()
        // Compared before it is stored, so an observation that found nothing new ends here
        // rather than rendering the whole table again.
        return current.width === next.width && current.height === next.height ? current : next
      })

    read()
    // The root element, because that is the box the initial containing block is derived from —
    // and because a `resize` event alone misses the zoom changes that do not fire one.
    const observer = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(read)
    observer?.observe(document.documentElement)
    window.addEventListener('resize', read)
    return () => {
      observer?.disconnect()
      window.removeEventListener('resize', read)
    }
  }, [])

  return viewport
}

/** One region's rectangle, as the style that places it. */
export const boxOf = (rect: Rect): CSSProperties => ({
  left: `${rect.x}px`,
  top: `${rect.y}px`,
  width: `${rect.width}px`,
  height: `${rect.height}px`,
})

/**
 * One region of the table, at the box the scene gave it.
 *
 * A surface still knows nothing about geometry: it is handed a box by being drawn inside one,
 * exactly as it was handed its answers by `Surface`. A region the ladder removed — an empty
 * stack, a side column that became a drawer — has a zero box and draws nothing at all, which is
 * the difference between a region that is absent and a region that is merely empty.
 *
 * Its overflow is `hidden` in the stylesheet and never `auto`. That is §3's last line as CSS: a
 * region that cannot hold its content is a region whose contents have to pack tighter, and there
 * is no scrollbar to fall through to while they learn how.
 */
export function Region({
  name,
  rect,
  children,
}: {
  name: string
  rect: Rect
  children: ReactNode
}) {
  if (rect.width <= 0 || rect.height <= 0) return null
  return (
    <div className={`region region--${name}`} style={boxOf(rect)}>
      {children}
    </div>
  )
}

/**
 * A rectangle divided into `count` columns, for the seats that share one band.
 *
 * The composition is two-player and the scene states one opponent box, but a table may seat
 * more than one — so the several share the one band rather than each claiming a band of their
 * own. The number of seats is fixed for the whole game, so this is not the content-driven
 * sizing §5 forbids: nothing a player does during a game changes it.
 */
export const share = (rect: Rect, index: number, count: number): Rect => {
  const width = Math.floor(rect.width / Math.max(1, count))
  return { x: rect.x + index * width, y: rect.y, width, height: rect.height }
}
