/**
 * Setting a run of text as large as its box will take, by bisection.
 *
 * `docs/client-design.md` §3: **text is fitted, never truncated.** An ellipsis is the client
 * deciding a player does not need the rest of a word, and the two places that decision was
 * being made — a card's title and a seat's name — are both names of things, which is the one
 * kind of text a player reads to tell two objects apart.
 *
 * The card fits *inside its own grid*, where one CSS pixel is one reference pixel and the box
 * is a constant (`Card.tsx`), so a card's runs are fitted once per render and never again. The
 * seat bar is real pixels in a box the viewport sizes, so it also has to be refitted whenever
 * that box changes shape — which is what `useFitted` is for and why it takes the box rather
 * than the text: the box's own height is set by the board's geometry and cannot be moved by
 * what is written in it, so shrinking the text can never feed back into the measurement.
 */
import { useLayoutEffect, type RefObject } from 'react'

/** How many halvings. Seven puts the answer within a fiftieth of the range it searched. */
const STEPS = 7

export const tooWide = (el: HTMLElement) => el.scrollWidth > el.clientWidth
export const tooBig = (el: HTMLElement) =>
  el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth

/**
 * Set `ref`'s text at the largest size in `[lo, hi]` that does not overflow.
 *
 * `overflows` is a predicate on the *element being sized*, so a caller whose text has to fit
 * something other than its own box passes a closure over that instead — see `useFitted`.
 */
export function fit(
  ref: RefObject<HTMLElement | null>,
  hi: number,
  lo: number,
  overflows: (el: HTMLElement) => boolean,
): void {
  const el = ref.current
  if (!el) return
  el.style.fontSize = `${hi}px`
  if (!overflows(el)) return
  let fits = lo
  let over = hi
  for (let i = 0; i < STEPS; i += 1) {
    const mid = (fits + over) / 2
    el.style.fontSize = `${mid}px`
    if (overflows(el)) over = mid
    else fits = mid
  }
  el.style.fontSize = `${fits}px`
}

/**
 * Fit `ref`'s text so that `box` does not overflow, at its stylesheet's size or smaller.
 *
 * Two things make this different from the card's fitting. The ceiling is **whatever the
 * stylesheet says**, read back after clearing the inline size, so a size that a media query
 * changed is still the size this starts from and nothing here has to know the tiers exist. And
 * the box is refitted on **resize**, because a region of the board is sized by the viewport:
 * the same name that fits a seat on a laptop has a shorter box on a half-height window.
 */
export function useFitted(
  ref: RefObject<HTMLElement | null>,
  box: RefObject<HTMLElement | null>,
  lo: number,
): void {
  // One effect with no dependencies: it refits after every render, because the text itself may
  // have changed, and it leaves an observer behind for every reshape of the box, which no
  // render is telling anyone about.
  useLayoutEffect(() => {
    const refit = () => {
      const el = ref.current
      if (!el) return
      el.style.fontSize = ''
      const hi = Number.parseFloat(window.getComputedStyle(el).fontSize)
      if (!Number.isFinite(hi) || hi <= lo) return
      fit(ref, hi, lo, () => {
        const outer = box.current
        return outer !== null && outer.scrollHeight > outer.clientHeight
      })
    }
    refit()

    const outer = box.current
    if (!outer || typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(refit)
    observer.observe(outer)
    return () => observer.disconnect()
  })
}
