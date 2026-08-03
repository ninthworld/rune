/**
 * Which relationships are drawn as arrows, and in which tone.
 *
 * The split from `relations.ts` is the same one `overlay.ts` used to make and it is the
 * guarantee worth keeping: what decides *what* is related knows nothing about pixels, and what
 * knows where everything is decides nothing. This module is the seam between them — it reads
 * stated edges and answers with pairs of ids, and the component that draws them measures boxes
 * and reads nothing.
 *
 * **Two tones, and no more** (`docs/client-design.md` §6.6): targeting and combat. They are read
 * together often enough — a spell aimed at an attacker — that telling them apart matters more
 * than either being pretty.
 *
 * Two kinds of edge are deliberately not drawn:
 *
 * - **Attachment.** An Equipment is drawn *behind the permanent it is attached to* (`Field.tsx`),
 *   which says the same thing without a line across the board.
 * - **An ability's source.** It is already the card in the stack item's own thumbnail, and a line
 *   from the stack to the permanent that made it would cross the whole table to restate that.
 *
 * An edge whose other end the server did not name is dropped here exactly as it is dropped from
 * the trail (`relations.relationLines`): there is nothing to point at, and pointing confidently
 * at something the view does not describe would be this client filling in the game.
 */
import type { Relation } from './relations'

export type ArrowTone = 'target' | 'combat'

/** One arrow: two entity ids the surfaces tagged, and how it is drawn. */
export interface Arrow {
  from: string
  to: string
  tone: ArrowTone
}

export function arrowsFor(relations: readonly Relation[]): readonly Arrow[] {
  return relations.flatMap((relation): Arrow[] => {
    const to = relation.to
    if (to === undefined) return []
    if (relation.kind === 'attacking' || relation.kind === 'blocking') {
      return [{ from: relation.from, to, tone: 'combat' }]
    }
    if (relation.kind === 'targeting') return [{ from: relation.from, to, tone: 'target' }]
    return []
  })
}
