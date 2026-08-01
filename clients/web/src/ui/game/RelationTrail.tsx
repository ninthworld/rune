/**
 * One object's relationships, drawn as a line of controls.
 *
 * The same trail hangs under a permanent, under an object on the stack, and beside a seat,
 * because a relationship is one kind of fact wherever it is read: something the server said
 * about which objects point at which. A board that described combat one way and the stack
 * another would make a player learn two vocabularies for the same arrow.
 *
 * Each named entity is a button rather than text. That is the traversal: a spell on the stack
 * reaches what it targeted, an attacker reaches its blockers, and an enchanted creature reaches
 * its aura — with the same click that reaches any other object, routed through the same rule
 * (`interaction.ts`). Reaching an object it names is often the only way to *see* it, because the
 * other end may be sitting in a pile or on the far half of the table.
 */
import type { RelationLine } from './../../relations'
import type { Surface } from './surface'

export function RelationTrail({
  lines,
  surface,
}: {
  lines: readonly RelationLine[]
  surface: Surface
}) {
  if (lines.length === 0) return null

  return (
    <p className="trail">
      {lines.map((line) => (
        <span key={`${line.kind}-${line.direction}`} className="trail__line">
          <span className="trail__label">{line.label}</span>
          {/* An edge the server stated without naming its other end — an attack with no
              defender projected — is the label alone. There is nothing to click through to,
              and inventing something to point at would be answering a question about the
              game that the view declined to answer. */}
          {line.ids.map((id) => (
            <button
              key={id}
              type="button"
              className="trail__end"
              // The phrase comes with it. Read on its own — by a screen reader, or by anyone
              // looking at a control out of context — a button labelled `Serra Angel` says
              // nothing about why it is under this card, and `attached to Serra Angel` says
              // all of it. It also keeps the visible text short enough to sit in a trail.
              aria-label={`${line.label} ${surface.labelFor(id)}`}
              onClick={() => surface.activate(id)}
            >
              {surface.labelFor(id)}
            </button>
          ))}
        </span>
      ))}
    </p>
  )
}
