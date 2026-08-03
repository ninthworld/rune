/**
 * A battlefield: the rows a seat's permanents divide into, and one permanent's footprint.
 *
 * **The row count is the board's, never the card's** (§5): one row per group `board.ts` made
 * from the server's own `card_types`, and the rows mirror across the table so both sets of
 * creatures meet at the dividing line. **A card's size is the height of the row it is in**, so a
 * seat that is short and narrow runs out of width long before it runs out of permanents — and
 * the row then pans sideways at full card size (`scrollStrip.ts`) rather than fanning cards into
 * each other.
 *
 * A permanent may carry things attached to it. Those step down *and to the right* and are laid
 * down first, so the creature — the permanent that attacks, blocks and dies — is the whole card
 * and the Equipment shows only its title bar. **Which permanent is attached to which is the
 * server's `attached_to`**; nothing here concludes it.
 *
 * Tapping turns the whole slot, attachments and all, and the arrow's anchor turns with it: a
 * tapped card takes its ring lying down.
 */
import type { CSSProperties } from 'react'

import { fieldRows } from './../../board'
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
}

/** How far each attached card steps, as a fraction of a card. */
const ATTACH_STEP = 0.16

function Slot({ entry, surface }: { entry: FieldEntry; surface: Surface }) {
  const { permanent, face, attached } = entry
  const cards = [...attached, face]
  const style = {
    '--n': cards.length,
    '--dx': attached.length > 0 ? ATTACH_STEP : 0,
    '--dy': cards.length > 1 ? ATTACH_STEP : 0,
  } as CSSProperties

  return (
    <div className={`perm${permanent.tapped ? ' perm-tapped' : ''}`} style={style}>
      {/* an arrow aims at the inner box: turned by the same rotation as the permanent */}
      <div className="perm-inner" data-anchor={face.id}>
        {cards.map((card, index) => (
          <Card
            key={card.id}
            face={card}
            style={{ '--i': index } as CSSProperties}
            anchor={index === cards.length - 1 ? undefined : card.id}
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

function Row({ entries, surface }: { entries: readonly FieldEntry[]; surface: Surface }) {
  const { ref, edges } = useScrollStrip<HTMLDivElement>()
  return (
    <div className="field-row">
      <div className={`strip field-scroll${edges}`} ref={ref}>
        {entries.map((entry) => (
          <Slot key={entry.permanent.id} entry={entry} surface={surface} />
        ))}
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
