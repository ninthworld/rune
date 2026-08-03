/**
 * A strip of cards wider than the space it was given — a battlefield row, or the hand.
 *
 * §3 and §5: a card's size is the height of the region it is in, so a row that is short and
 * narrow runs out of *width* long before it runs out of permanents. The row then **pans
 * sideways at full card size** rather than fanning them into each other; nothing on the board
 * scrolls vertically and no region ever grows a scrollbar.
 *
 * The native scrollbar is hidden: it would eat into the height the cards are measured from, and
 * a bar across the board reads as a defect. The scrollable edges are masked away to transparent
 * instead, which says "there is more this way" without one.
 *
 * Hiding the bar means the strip has to earn its input back, because `overflow-x` alone only
 * answers a horizontal wheel and a touch drag — neither of which a mouse produces. So a wheel of
 * either axis moves it, and it can be dragged like a map.
 *
 * Returns the ref to put on the scrolling element and the edge classes that drive the mask.
 */
import { useLayoutEffect, useRef, useState } from 'react'

export function useScrollStrip<T extends HTMLElement>() {
  const ref = useRef<T>(null)
  const [edges, setEdges] = useState('')

  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const update = () => {
      const left = el.scrollLeft > 1
      const right = el.scrollLeft + el.clientWidth < el.scrollWidth - 1
      setEdges(`${left ? ' scroll-l' : ''}${right ? ' scroll-r' : ''}`)
    }
    update()
    el.addEventListener('scroll', update, { passive: true })
    const observer = new ResizeObserver(update)
    observer.observe(el)
    for (const child of el.children) observer.observe(child)
    return () => {
      el.removeEventListener('scroll', update)
      observer.disconnect()
    }
  })

  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return

    const room = () => el.scrollWidth - el.clientWidth

    /* a mouse only ever sends deltaY, so take whichever axis moved most and spend it sideways;
       hand the event back at either end so the strip never traps a gesture meant for something
       else */
    const onWheel = (event: WheelEvent) => {
      const max = room()
      if (max <= 0) return
      const raw = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY
      const step = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? el.clientWidth : 1
      const next = Math.max(0, Math.min(max, el.scrollLeft + raw * step))
      if (next === el.scrollLeft) return
      event.preventDefault()
      el.scrollLeft = next
    }

    let from = 0
    let at = 0
    let panning = false
    const onMove = (event: PointerEvent) => {
      /* a few pixels of slop, so a click on a card is still a click */
      if (!panning && Math.abs(event.clientX - from) < 4) return
      panning = true
      el.scrollLeft = at - (event.clientX - from)
    }
    const onUp = () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      el.classList.remove('panning')
      /* a pan that ends over a card must not also read as a click on it */
      if (panning) {
        el.addEventListener('click', (event) => event.stopPropagation(), {
          capture: true,
          once: true,
        })
      }
    }
    const onDown = (event: PointerEvent) => {
      if (event.button !== 0 || room() <= 0) return
      from = event.clientX
      at = el.scrollLeft
      panning = false
      el.classList.add('panning')
      window.addEventListener('pointermove', onMove)
      window.addEventListener('pointerup', onUp)
    }

    el.addEventListener('wheel', onWheel, { passive: false })
    el.addEventListener('pointerdown', onDown)
    return () => {
      el.removeEventListener('wheel', onWheel)
      el.removeEventListener('pointerdown', onDown)
      onUp()
    }
    /* bound once: scrolling re-renders the strip, and re-running the effect would tear down a
       drag that is still in the user's hand */
  }, [])

  return { ref, edges }
}
