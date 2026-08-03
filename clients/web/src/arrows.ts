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
import type { Slot } from './interaction'
import type { ValidAction } from './protocol'
import type { Relation } from './relations'

export type ArrowTone = 'target' | 'combat'

/** One arrow: two entity ids the surfaces tagged, and how it is drawn. */
export interface Arrow {
  from: string
  to: string
  tone: ArrowTone
}

/**
 * The arrows for an answer still being assembled.
 *
 * The board draws what the *server* stated, and a draft is by definition not stated yet — but a
 * declaration is exactly the moment a player needs the picture, because three attackers pointed
 * at two defenders is a fact about their own intent that the words in the bar cannot hold. So a
 * draft draws the same two tones, from the same ids, the moment they are chosen: an attacker to
 * what it attacks, a spell to what it is aimed at.
 *
 * Every end here is still an identifier the server listed — a `subject` it stated and a
 * `candidate` it enumerated — so this states nothing about the game either. It is a picture of
 * the message this client is about to send, and it disappears with the draft (`settle`).
 *
 * A slot the server said belongs to a subject draws from that subject; every other slot draws
 * from the object the action itself belongs to, which is the card being cast. A slot with
 * neither draws nothing rather than picking somewhere for the line to start.
 */
export function draftArrows(
  action: ValidAction | undefined,
  slots: readonly Slot[],
): readonly Arrow[] {
  if (!action) return []
  const owner = (action.subject ?? [])[0]
  return slots.flatMap((slot): Arrow[] => {
    if (!slot.byEntity) return []
    const from = slot.subject ?? owner
    if (from === undefined) return []
    // Combat when the server paired the slot with one of the objects taking part, targeting
    // when it is the action's own — the same two tones, told apart the same way.
    const tone: ArrowTone = slot.subject === undefined ? 'target' : 'combat'
    return slot.chosen.filter((to) => to !== from).map((to) => ({ from, to, tone }))
  })
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
