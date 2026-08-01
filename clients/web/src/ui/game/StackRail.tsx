/**
 * The column between the two battlefields, where the stack lives.
 *
 * The stack is given a fixed place rather than a panel that appears and disappears, because a
 * player has to be able to look at the same spot and see *nothing on the stack* — which is a
 * different answer from having missed the panel.
 *
 * **Resolution order, top first.** The wire lists the stack bottom-first, which is the order the
 * objects were put there and the reverse of the order anything happens in. What a player needs
 * from this column is "what resolves next, and what after that", so it is read top down and the
 * top object says so in words. Position is stated rather than implied by height: a column of six
 * cards does not tell you which end is the top, and getting it backwards is the difference
 * between holding priority and losing the game.
 *
 * Everything each object is *about* — its source, its controller, what it targets, and what
 * targeted it — comes from the relationship join (`relations.ts`) and renders as the same trail
 * that hangs under a permanent. A counterspell and the spell it named trace to each other from
 * both ends, without either one reading the other's rules text.
 *
 * Emblems sit under it. An emblem (CR 114) is in no zone and is never removed, so it belongs
 * beside the board rather than on it, and this rail is the board-adjacent column.
 */
import type { Emblem, StackItem } from './../../protocol'
import type { CardFace } from './../../card-face'
import type { RelationLine } from './../../relations'
import { Card } from './../Card'
import { RulesText } from './../RulesText'
import { RelationTrail } from './RelationTrail'
import type { Surface } from './surface'

export interface StackEntry {
  item: StackItem
  face: CardFace
  /** Everything the view relates this object to, in either direction. */
  lines: readonly RelationLine[]
}

export interface EmblemEntry {
  emblem: Emblem
  face: CardFace
}

export function StackRail({
  stack,
  emblems,
  collapsed = false,
  label,
  surface,
}: {
  /** Bottom first, exactly as the server sent it. Reversed here, never upstream. */
  stack: readonly StackEntry[]
  emblems: readonly EmblemEntry[]
  /**
   * §3, step 7: there is no width for a rail, so the stack is a badge.
   *
   * What a badge keeps is what §3 says never degrades — the top item **by name**, and a count —
   * and everything under it moves into the accessibility tree rather than out of the document,
   * so a screen reader still reads the whole stack in resolution order. Drawing the rest of it
   * as faces is #662's; this is the flag arriving and the one fact being protected.
   */
  collapsed?: boolean
  label(id: string): string
  surface: Surface
}) {
  const resolving = [...stack].reverse()

  if (collapsed) return <StackBadge resolving={resolving} label={label} />

  return (
    <div className="rail">
      <section className="rail__zone" aria-label="Stack">
        <h2 className="rail__heading">Stack</h2>
        {resolving.length === 0 ? (
          <p className="rail__empty">Empty.</p>
        ) : (
          <ol className="cards cards--stack">
            {resolving.map(({ item, face, lines }, index) => (
              <li key={item.id} className={index === 0 ? 'stack stack--top' : 'stack'}>
                <p className="stack__order">
                  {index === 0
                    ? 'Resolves next'
                    : // Counted from the top, because that is the order it will happen in;
                      // counting from the bottom would number them by an order nothing uses.
                      `${index + 1} of ${resolving.length}`}
                </p>

                <Card
                  face={face}
                  state={surface.stateOf(face.id)}
                  link={surface.linkOf(face.id)}
                  onActivate={surface.activate}
                  onInspect={surface.inspect}
                  onTrace={surface.trace}
                />

                <p className="cards__aside">
                  {/* The server composes a description for the stack object itself, which is
                      not always the card's name — "Counterspell targeting Twin Bolt" says
                      something the face does not. Kept only when it adds something: for many
                      spells it is just the name, or verbatim the rules text already above. */}
                  {item.description !== face.name && item.description !== face.rulesText && (
                    <>
                      <RulesText text={item.description} /> —{' '}
                    </>
                  )}
                  {label(item.controller)}
                </p>

                <RelationTrail lines={lines} surface={surface} />
              </li>
            ))}
          </ol>
        )}
      </section>

      {emblems.length > 0 && (
        <section className="rail__zone" aria-label="Emblems">
          <h2 className="rail__heading">Emblems</h2>
          <ul className="cards cards--emblems">
            {emblems.map(({ emblem, face }) => (
              <li key={emblem.id}>
                <Card
                  face={face}
                  state={surface.stateOf(face.id)}
                  link={surface.linkOf(face.id)}
                  onActivate={surface.activate}
                  onInspect={surface.inspect}
                  onTrace={surface.trace}
                />
                <p className="cards__aside">{label(emblem.controller)}</p>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  )
}

/**
 * The stack with no room for a rail: the top item's name, and how many are behind it.
 *
 * The name is the visible half because that is the fact §3 protects, and it is the *card's* name
 * rather than the server's composed description — a description is a sentence and a badge is a
 * word, and the sentence is directly below it for anything that reads the document. Everything
 * the rail would have drawn is still here in resolution order, so the top item, its description,
 * its controller, and every item under it are one query away for assistive technology and for
 * the gesture #662 will hang on this.
 */
function StackBadge({
  resolving,
  label,
}: {
  resolving: readonly StackEntry[]
  label(id: string): string
}) {
  const top = resolving[0]
  if (!top) return null

  return (
    <section className="badge-rail" aria-label="Stack">
      <h2 className="visually-hidden">Stack</h2>
      <ol className="badge-rail__items">
        {resolving.map(({ item, face }, index) => (
          <li key={item.id} className={index === 0 ? 'stack stack--top' : 'stack visually-hidden'}>
            {index === 0 ? (
              <>
                <span className="badge-rail__name">{face.name}</span>
                <span className="badge-rail__count">
                  ×{resolving.length}
                  <span className="visually-hidden"> on the stack</span>
                </span>
                <span className="visually-hidden">
                  Resolves next — {item.description} — {label(item.controller)}
                </span>
              </>
            ) : (
              <>
                {index + 1} of {resolving.length} — {item.description} — {label(item.controller)}
              </>
            )}
          </li>
        ))}
      </ol>
    </section>
  )
}
