/**
 * An object's own actions, beside the object.
 *
 * The same list the action bar would carry for the same selection, from the same `actionsFor` —
 * drawn where the player is looking, because the trip to the bar is the whole cost of a
 * creature's second ability and a player choosing between two of them is looking at the creature.
 *
 * **It is not a context menu.** It opens on the click that already selected the object
 * (`menu.ts`), never on a gesture of its own, and taking one goes through the same `take` a
 * button does. Right-click still means read.
 *
 * It is placed at the object's own anchor — the same `data-anchor` an arrow finds its ends by —
 * so tagging what a surface draws gives it the menu as well as the arrows.
 */
import { useLayoutEffect, useRef } from 'react'

import type { ObjectMenu as Menu } from './../../menu'
import type { ValidAction } from './../../protocol'
import { Symbols } from './../card/Symbols'

export function ObjectMenu({
  menu,
  label,
  take,
  close,
}: {
  menu: Menu
  label(id: string): string
  take(action: ValidAction): void
  close(): void
}) {
  const panel = useRef<HTMLDivElement>(null)
  const first = useRef<HTMLButtonElement>(null)

  // Written straight onto the element rather than held as state: where a menu lands is a fact
  // about the DOM at this moment, and putting a measurement through React only to hand it back
  // to the same element is a render nobody needs.
  useLayoutEffect(() => {
    const anchor = document.querySelector(`[data-anchor="${CSS.escape(menu.id)}"]`)
    const box = anchor?.getBoundingClientRect()
    const el = panel.current
    if (!box || !el) return
    el.style.left = `${Math.min(window.innerWidth - 210, box.left + box.width / 2 - 90)}px`
    el.style.top = `${Math.min(window.innerHeight - 40, box.bottom + 6)}px`
    first.current?.focus()
  }, [menu.id])

  return (
    <div className="menu-scrim" onClick={close}>
      <div
        ref={panel}
        className="obj-menu"
        role="dialog"
        aria-label={label(menu.id)}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="obj-menu-head">{label(menu.id)}</div>
        {menu.actions.length === 0 ? (
          <div className="obj-menu-empty">Nothing to take right now.</div>
        ) : (
          menu.actions.map((action, index) => (
            <button
              key={action.id}
              ref={index === 0 ? first : undefined}
              className="obj-menu-item"
              onClick={() => take(action)}
            >
              <Symbols text={action.label} />
            </button>
          ))
        )}
      </div>
    </div>
  )
}
