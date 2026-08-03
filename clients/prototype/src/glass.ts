import { createContext, useContext } from "react";

/* Whether cards wear the chrome's glass, wired through context for the
   same reason peek is: every card everywhere should answer it alike. The
   card lab overrides it per card, so it can hold both up at once. */
export const GlassContext = createContext(false);

export function useGlass() {
  return useContext(GlassContext);
}

/* How much of a card is ours. The frame alone is what the prototype
   draws; the other two spend a fetched picture on the art window or on
   the whole face. Wired the same way, for the same reason. */
export type CardView = "frame" | "art" | "full";

export const ViewContext = createContext<CardView>("frame");

export function useCardView() {
  return useContext(ViewContext);
}
