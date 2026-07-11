/**
 * Derive a card's display color identity — the token key the Pixi card factory
 * frames a card with (see `src/card/cardFactory.ts`, whose `CardDisplayData`
 * deliberately leaves `colorIdentity` to the caller).
 *
 * This is **display glue, not game logic**. It never computes legality, cost, or
 * effect; it only picks which `PALETTE` swatch frames a card, from the
 * server-provided type line and displayed mana cost string. The server remains
 * authoritative for every actual characteristic.
 */
import type { CardView } from '../protocol';
import type { ColorIdentity } from '../tokens';

/** The five single-letter colored pips that map onto a palette color. */
const COLORED_PIPS: Record<string, ColorIdentity> = {
  W: 'W',
  U: 'U',
  B: 'B',
  R: 'R',
  G: 'G',
};

/**
 * Choose the frame color for a card:
 * - lands frame as land (`L`), whatever their (usually absent) cost;
 * - a cost with two or more distinct colors is multicolor (`M`);
 * - a cost with exactly one color uses that color;
 * - a cost with no colored pips (generic/`{2}`, artifacts) is colorless (`C`).
 *
 * Hybrid/Phyrexian pips such as `{W/U}` or `{G/P}` contribute each color letter
 * they name, matching how the mana-cost string reads.
 */
export function deriveColorIdentity(card: CardView): ColorIdentity {
  if (/\bland\b/i.test(card.type_line)) return 'L';

  const colors = new Set<ColorIdentity>();
  for (const match of (card.mana_cost ?? '').matchAll(/\{([^}]+)\}/g)) {
    for (const part of match[1].toUpperCase().split('/')) {
      const color = COLORED_PIPS[part];
      if (color) colors.add(color);
    }
  }

  if (colors.size > 1) return 'M';
  const [only] = colors;
  return only ?? 'C';
}
