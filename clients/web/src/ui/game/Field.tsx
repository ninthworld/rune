/**
 * A battlefield: the rows a seat's permanents divide into, and one permanent's footprint.
 *
 * **The row count is the board's, never the card's** (§5): two rows, split by the server's own
 * `card_types` (`board.ts`), mirrored across the table so both sets of creatures meet at the
 * dividing line. **A card's size is the height of the row it is in**, so a seat that is short and
 * narrow runs out of width long before it runs out of permanents — and the row then pans sideways
 * at full card size (`scrollStrip.ts`) rather than fanning cards into each other.
 *
 * **What a row does fan is a pile**: a run of permanents a player has no reason to tell apart is
 * drawn overlapping, with a count on it, so eight Forests cost the width of about three. Which
 * permanents those are is `board.piles`'s answer over the key `stackKey` states below, and the
 * key is deliberately strict — anything the board would have drawn differently on one of them
 * breaks the pile, so no fact ends up behind a card.
 *
 * A permanent may carry things attached to it. Those step down *and to the right* and are laid
 * down first, so the creature — the permanent that attacks, blocks and dies — is the whole card
 * and the Equipment shows only its title bar. **Which permanent is attached to which is the
 * server's `attached_to`**; nothing here concludes it.
 *
 * Tapping turns the whole slot, attachments and all, and the arrow's anchor turns with it: a
 * tapped card takes its ring lying down. **A permanent the player's own draft would tap turns
 * the same way** (`FieldEntry.turning`): a land picked for a pip and a creature put into the
 * declaration are both about to be tapped, nothing has been sent yet, and a player assembling
 * either is owed a picture of what they have committed. Clicking it again stands it back up.
 */
import type { CSSProperties } from 'react'

import { fieldRows, piles } from './../../board'
import type { CardFace } from './../../card-face'
import type { Permanent } from './../../protocol'
import { Card } from './../card/Card'
import { useScrollStrip } from './../card/scrollStrip'
import type { Surface } from './surface'

/** One permanent, with the face it draws and whatever is attached to it. */
export interface FieldEntry {
  permanent: Permanent
  face: CardFace
  attached: readonly CardFace[]
  /** What the server related it to, in words — the readable copy of the arrows over it. */
  note: string
  /**
   * Whether the answer being drafted would tap this permanent — a land picked for a pip, a
   * creature put into the declaration (`interaction.tappedByDraft`).
   *
   * It turns for the same reason a tapped one does, because it is about to be one: nothing has
   * been sent, so the *server's* board still has it standing up, and a player assembling a
   * declaration or a payment is owed a picture of what they have committed. Taking the choice
   * back stands it up again, and a new view settles it either way.
   */
  turning: boolean
}

/** How far each attached card steps, as a fraction of a card. */
const ATTACH_STEP = 0.16

function Slot({ entry, surface }: { entry: FieldEntry; surface: Surface }) {
  const { permanent, face, attached } = entry
  // The board's tap state and the one the player is in the middle of choosing are drawn the
  // same way, deliberately: what a permanent looks like is a fact about the board, and the
  // board a player is answering *on* is the one their own answers are already on.
  const tapped = permanent.tapped || entry.turning
  const cards = [...attached, face]
  const style = {
    '--n': cards.length,
    '--dx': attached.length > 0 ? ATTACH_STEP : 0,
    '--dy': cards.length > 1 ? ATTACH_STEP : 0,
  } as CSSProperties

  return (
    <div className={`perm${tapped ? ' perm-tapped' : ''}`} style={style}>
      {/* An arrow aims at the permanent's **own** card, not at the group box that holds it and
          everything attached to it (issue #715). The group is what the eye reads as one pile, but
          it is not what a spell targets: ringing it drew a highlight around an Equipment that was
          never the target. Each card in the pile carries its own anchor and is ringed only when it
          is itself the thing being pointed at. The rotation still comes from the ancestor, so a
          tapped permanent's ring is still its tapped box. */}
      <div className="perm-inner">
        {cards.map((card, index) => (
          <Card
            key={card.id}
            face={card}
            style={{ '--i': index } as CSSProperties}
            anchor={card.id}
            state={surface.stateOf(card.id)}
            link={surface.linkOf(card.id)}
            onTrace={surface.trace}
            onActivate={surface.activate}
            onInspect={surface.inspect}
            {...(index === cards.length - 1 && entry.note !== '' ? { note: entry.note } : {})}
            // Only the permanent itself wears what is true of the permanent: an Equipment behind
            // it is showing its title bar and nothing else.
            overlay={index === cards.length - 1}
          />
        ))}
      </div>
    </div>
  )
}

/**
 * What two permanents must agree on before they are drawn as one pile (`board.piles`).
 *
 * Deliberately over-strict, and the reason is in `board.ts`: anything the board would have
 * drawn differently on one of them has to break the pile, or a player loses a fact behind a
 * card. So the key is the card's own identity plus every mark the board puts on it — tap state,
 * counters, damage, markers, what is attached, and the relationships the server stated, which
 * is what keeps an attacker from hiding behind a creature that is not attacking.
 *
 * `undefined` means "never stack this one": a permanent with no card identity (a token, CR 111)
 * has nothing two of them could be the same *card* of, and a permanent this interaction has
 * singled out is one the player is being asked about by itself.
 */
function stackKey(entry: FieldEntry, surface: Surface): string | undefined {
  const { permanent, face } = entry
  if (face.artKey === undefined) return undefined
  const state = surface.stateOf(permanent.id)
  // Singled out by the interaction: the player is being asked about *this* one, or waiting on
  // an answer about it, so it is not one of a set. A `candidate` is not singled out — every
  // untapped Forest is one at once — and hiding those would mean a mana base that stops
  // stacking for the whole of a main phase, which is most of the game.
  if (state === 'selected' || state === 'pending') return undefined
  return [
    state,
    surface.linkOf(permanent.id) ?? '',
    face.artKey,
    face.tapped ? 't' : '',
    // A permanent the draft has turned is drawn differently from its twin that is still
    // standing, so it is not one of a set (`FieldEntry.turning`).
    entry.turning ? 'g' : '',
    face.summoningSick ? 's' : '',
    face.stat?.value ?? '',
    face.counters.map((counter) => `${counter.kind}:${counter.count}`).join('+'),
    face.damage ?? '',
    face.markers.join('+'),
    entry.attached.map((card) => card.id).join('+'),
    entry.note,
  ].join('|')
}

/** One pile of identical permanents, or one permanent that is nothing else's twin. */
function Pile({ entries, surface }: { entries: readonly FieldEntry[]; surface: Surface }) {
  if (entries.length === 1) {
    const only = entries[0]
    return only ? <Slot entry={only} surface={surface} /> : null
  }
  return (
    <div className="perm-fan">
      {entries.map((entry) => (
        <Slot key={entry.permanent.id} entry={entry} surface={surface} />
      ))}
      <span className="perm-fan-count" aria-hidden="true">
        {entries.length}
      </span>
    </div>
  )
}

function Row({ entries, surface }: { entries: readonly FieldEntry[]; surface: Surface }) {
  const { ref, edges } = useScrollStrip<HTMLDivElement>()
  // Which permanents are drawn as one pile is `board.piles`'s answer, over a key this file
  // states — the same split as the rows: what may be grouped is decided once, away from a
  // component, and the component only draws the grouping it was handed.
  const grouped = piles(entries, (entry) => stackKey(entry, surface))
  return (
    <div className="field-row">
      <div className={`strip field-scroll${edges}`} ref={ref}>
        {grouped.map((pile) => {
          const first = pile.entries[0]
          return first === undefined ? null : (
            <Pile key={first.permanent.id} entries={pile.entries} surface={surface} />
          )
        })}
      </div>
    </div>
  )
}

/** One seat's half of the table. */
export function Field({
  entries,
  label,
  mirrored,
  surface,
}: {
  entries: readonly FieldEntry[]
  /** Whose half this is, said rather than only drawn — the board's one spatial fact. */
  label: string
  mirrored?: boolean
  surface: Surface
}) {
  // Which rows exist, in which order, and what share of the field's height each takes — all of
  // it `board.ts`'s answer, so nothing about the arrangement is decided in a component.
  const rows = fieldRows(entries, (entry) => entry.face.cardTypes, { mirrored: mirrored === true })

  return (
    <div
      className={`field-area${mirrored ? ' field-area-mirror' : ''}`}
      style={{ gridTemplateRows: rows.map((row) => `minmax(0, ${row.share}fr)`).join(' ') }}
      role="region"
      aria-label={label}
    >
      {rows.map((row) => (
        <Row key={row.row} entries={row.entries} surface={surface} />
      ))}
    </div>
  )
}
