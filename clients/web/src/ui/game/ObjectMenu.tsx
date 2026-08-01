/**
 * An object's actions, drawn at the object.
 *
 * The dock still has this list and always will — a subject can be a card inside a collapsed pile
 * or an id in no rendered zone at all, and #626's rule is that nothing becomes unreachable
 * because a surface did not happen to draw it. What this adds is proximity: a creature with two
 * abilities is chosen between while looking at the creature, not by travelling to the bottom of
 * the screen and back.
 *
 * **It is opened by the click that already selected the object**, not by a gesture of its own.
 * `interaction.ts` returns `{kind: 'select'}` for exactly the case this exists for — an object
 * the server attached more than one action to, where a click has no single meaning — so this
 * component adds no rule and no gesture. Right-click still opens the inspector, which is what
 * keeps reading a card free.
 *
 * Everything in it is a `<button>` reached the same way by a mouse and by a keyboard: it takes
 * focus when it opens, the arrows walk it, Escape closes it and hands focus back to the object it
 * belongs to. That is the whole reason it is a popover of buttons rather than a native context
 * menu — a gesture with no keyboard equivalent is one that has to be reinvented for every control
 * scheme this client is ever ported to.
 *
 * Position is presentation and nothing else: it is measured from the same `data-entity` anchor
 * the drawn relationships use, and if the object is not on the screen there is nothing to anchor
 * to and the dock is still holding the identical list.
 */
import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react'

import type { ValidAction } from './../../protocol'
import type { ObjectMenu as Menu } from './../../menu'
import { ANCHOR } from './../../overlay'
import { RulesText } from './../RulesText'

/** How far off the object's edge the panel sits, and how close to the viewport it may come. */
const GAP = 8
const MARGIN = 8

export function ObjectMenu({
  menu,
  label,
  take,
  inspect,
  close,
}: {
  menu: Menu
  label(id: string): string
  take(action: ValidAction): void
  inspect(id: string): void
  /** Give up the selection. The same thing Escape and the dock's "Clear selection" do. */
  close(): void
}) {
  const ref = useRef<HTMLDivElement>(null)
  const [at, setAt] = useState<{ left: number; top: number } | undefined>(undefined)

  // Measured after layout, from the element the surfaces tagged with this id — the same anchor
  // the drawn relationships are measured from, so the menu and the lines cannot disagree about
  // where an object is.
  useLayoutEffect(() => {
    const panel = ref.current
    // Found by reading the attribute back rather than by building a selector out of it: a
    // server id is opaque to this client, and an id containing a quote would otherwise be a
    // selector this file wrote and the browser refused.
    const anchor = [...document.querySelectorAll<HTMLElement>(`[${ANCHOR}]`)].find(
      (element) => element.dataset.entity === menu.id,
    )
    if (!panel || !anchor) return

    const box = anchor.getBoundingClientRect()
    const size = panel.getBoundingClientRect()
    // Beside the object by preference and flipped when there is no room, so a card at the right
    // edge of the table does not open its own actions off the screen.
    const right = box.right + GAP
    const left =
      right + size.width + MARGIN <= window.innerWidth ? right : box.left - GAP - size.width
    const top = box.top
    setAt({
      left: clamp(left, MARGIN, window.innerWidth - size.width - MARGIN),
      top: clamp(top, MARGIN, window.innerHeight - size.height - MARGIN),
    })
  }, [menu.id, menu.actions])

  // The keyboard follows the menu, and goes back where it came from when the menu closes.
  // Without the second half, closing a list leaves focus on nothing and the next Tab starts from
  // the top of the document — the failure that makes keyboard users abandon a control that works
  // perfectly well with a mouse. Mounted per object (`key` in `Game.tsx`), so both halves are
  // about one object and never carry a stale one between them.
  const first = useRef<HTMLButtonElement>(null)
  const returnTo = useRef<HTMLElement | null>(null)
  useEffect(() => {
    if (document.activeElement instanceof HTMLElement) returnTo.current = document.activeElement
    return () => {
      const from = returnTo.current
      // Only if it is still on the screen: the object may have left the view entirely, and
      // focusing a detached element puts the keyboard back at the top of the document.
      if (from && document.contains(from)) from.focus()
    }
  }, [])

  // After placement, never before it: an element that is still `hidden` cannot take focus, so
  // focusing on mount would silently do nothing and leave the keyboard on the card.
  const placed = at !== undefined
  useEffect(() => {
    if (placed) first.current?.focus()
  }, [placed])

  const walk = (event: KeyboardEvent) => {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
    const items = [...(ref.current?.querySelectorAll('button') ?? [])]
    const index = items.indexOf(document.activeElement as HTMLButtonElement)
    if (index === -1) return
    event.preventDefault()
    const next = event.key === 'ArrowDown' ? index + 1 : index - 1
    items[(next + items.length) % items.length]?.focus()
  }

  return (
    <div
      ref={ref}
      className="menu"
      // A group rather than a `menu` role: these are ordinary buttons in the tab order, and
      // claiming menu semantics would promise the arrow-key-only navigation model that goes
      // with it — which is the model that hides controls from anyone who expects Tab to work.
      role="group"
      aria-label={`${label(menu.id)} actions`}
      style={at && { left: `${at.left}px`, top: `${at.top}px` }}
      // Hidden until placed. One frame at the wrong corner of the screen is a flicker a player
      // sees on every single click.
      hidden={at === undefined}
      onKeyDown={walk}
    >
      <p className="menu__who">
        <strong>{label(menu.id)}</strong>
      </p>

      {menu.actions.length === 0 ? (
        <p className="menu__empty">Nothing to do with this one right now.</p>
      ) : (
        <ul className="menu__actions">
          {menu.actions.map((action, index) => (
            <li key={action.id}>
              <button
                ref={index === 0 ? first : undefined}
                type="button"
                onClick={() => take(action)}
              >
                {/* Server text, drawn the way server text is drawn everywhere else: `{T}: Add
                    {G}.` is the same sentence here as on the card it came from. */}
                <RulesText text={action.label} />
                {action.mana_ability ? ' ⟨mana⟩' : ''}
              </button>
            </li>
          ))}
        </ul>
      )}

      <p className="menu__reading">
        {/* Reading is offered here as well as on the right-click, because a menu that lists
            everything you can *do* with an object and no way to find out what it is is a menu
            that sends you back to the board to hunt for a gesture. */}
        <button
          ref={menu.actions.length === 0 ? first : undefined}
          type="button"
          onClick={() => inspect(menu.id)}
        >
          Inspect
        </button>{' '}
        <button type="button" onClick={close}>
          Close
        </button>
      </p>
    </div>
  )
}

const clamp = (value: number, low: number, high: number): number =>
  Math.max(low, Math.min(value, Math.max(low, high)))
