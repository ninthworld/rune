/**
 * Design tokens shared by BOTH card renderers (the Pixi factory and the HTML
 * card component). Locked in docs/design/ui-design-notes.md — change there first.
 */
export const PALETTE = {
  W: "#CFC7AC",
  U: "#4E86C1",
  B: "#77688C",
  R: "#C05B4D",
  G: "#57935F",
  M: "#C9A84C", // multicolor
  C: "#8C949C", // colorless
  L: "#A08A6E", // land
} as const;

export const PT_TEXT = {
  W: "#2A2820",
  U: "#0D1F33",
  B: "#17111F",
  R: "#2E0D08",
  G: "#122015",
  M: "#2E240A",
  C: "#1C2024",
  L: "#241C12",
} as const;

export const SURFACES = {
  board: "#15171A",
  cardBody: "#23262B",
  nameText: "#E8E6E1",
  selection: "#7FB2E5",
  targeting: "#E0784A",
} as const;

/** Card size tiers: digest (opponent chips) / field / support / hand / full (inspect). */
export const TIER = {
  chip: { w: 44, h: 60 },
  support: { w: 66, h: 92 },
  field: { w: 84, h: 118 },
  hand: { w: 104, h: 146 },
} as const;

export type ColorIdentity = keyof typeof PALETTE;
