/**
 * Holding a card opens it big enough to read.
 *
 * The preview beside the board answers this on a desktop, but a finger has no hover and there is
 * no side column on a phone — so the gesture is a press, and it is wired through context rather
 * than threaded down as a prop, because every card everywhere should answer it the same way
 * (`docs/client-design.md` §6.6).
 */
import { createContext, useContext } from 'react'

import type { CardFace } from './../../card-face'

export const PeekContext = createContext<((face: CardFace) => void) | null>(null)

export function usePeek() {
  return useContext(PeekContext)
}

/** Long enough not to fire on a tap, short enough not to feel broken. */
export const PEEK_MS = 420

/** A press that travels this far was a pan or a swipe, not a hold. */
export const PEEK_SLOP = 8
