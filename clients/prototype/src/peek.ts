import { createContext, useContext } from "react";
import type { CardData } from "./Card";

/* Holding a card opens it big enough to read. The hover preview in the
   sidebar answers this on a desktop, but a finger has no hover and the
   sidebar isn't there on a phone — so the gesture is a press, and it is
   wired through context rather than threaded down as a prop, because
   every card everywhere should answer it the same way. */
export const PeekContext = createContext<((card: CardData) => void) | null>(
  null,
);

export function usePeek() {
  return useContext(PeekContext);
}

/* long enough not to fire on a tap, short enough not to feel broken */
export const PEEK_MS = 420;

/* a press that travels this far was a pan or a swipe, not a hold */
export const PEEK_SLOP = 8;
