/**
 * The column between the two battlefields, where the stack lives.
 *
 * The stack is given a fixed place rather than a panel that appears and disappears, because a
 * player has to be able to look at the same spot and see *nothing on the stack* — which is a
 * different answer from having missed the panel.
 *
 * Emblems sit under it. An emblem (CR 114) is in no zone and is never removed, so it belongs
 * beside the board rather than on it, and this rail is the board-adjacent column.
 */
import type { Emblem, StackItem } from './../../protocol'
import type { CardFace } from './../../card-face'
import { list } from './../../normalize'
import { Card } from './../Card'
import type { Surface } from './surface'

export interface StackEntry {
  item: StackItem
  face: CardFace
}

export interface EmblemEntry {
  emblem: Emblem
  face: CardFace
}

export function StackRail({
  stack,
  emblems,
  label,
  surface,
}: {
  stack: readonly StackEntry[]
  emblems: readonly EmblemEntry[]
  label(id: string): string
  surface: Surface
}) {
  return (
    <div className="rail">
      <section className="rail__zone" aria-label="Stack">
        <h2 className="rail__heading">Stack</h2>
        {stack.length === 0 ? (
          <p className="rail__empty">Empty.</p>
        ) : (
          // Bottom first on the wire; the top of the stack resolves first, so it reads last.
          <ol className="cards cards--stack">
            {stack.map(({ item, face }) => (
              <li key={item.id}>
                <Card
                  face={face}
                  variant="stack"
                  state={surface.stateOf(face.id)}
                  onActivate={surface.activate}
                />
                <p className="cards__aside">
                  {/* The server composes a description for the stack object itself, which is
                      not always the card's name — "Counterspell targeting Twin Bolt" says
                      something the face does not. Kept only when it adds something: for many
                      spells it is just the name, or verbatim the rules text already above. */}
                  {item.description !== face.name && item.description !== face.rulesText && (
                    <>{item.description} — </>
                  )}
                  {label(item.controller)}
                  {list(item.targets).length > 0 && (
                    <>
                      {' '}
                      →{' '}
                      {list(item.targets)
                        .map((t) => ('id' in t ? surface.labelFor(t.id) : label(t.player)))
                        .join(', ')}
                    </>
                  )}
                </p>
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
                  variant="stack"
                  state={surface.stateOf(face.id)}
                  onActivate={surface.activate}
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
