/**
 * The shell's own controls, because the platform's do not survive this design.
 *
 * `docs/client-design.md` §9.2 rule 7: **no native form control.** The argument is one screenshot
 * — a full-width `<select>` whose own arrow is clipped at its right edge at 120% zoom — and it
 * generalises. A native select draws chrome the page cannot size, cannot fit text into, and
 * cannot hold above the 11px floor; its popup is painted by the operating system in a palette
 * that has nothing to do with a dark table; and at the sizes §1 asks this client to work at, it
 * is the first thing that breaks.
 *
 * So the two shapes a choice comes in are built here, once, and every surface uses them:
 *
 * - **`Choice`** is a group of options all on screen at the same time. It is the right drawing
 *   wherever there are few enough to show, because a choice you can see is one nobody has to
 *   open — and where an option has something to say about itself (a server's region, an AI's
 *   description), a row can carry it without a sentence printed beside the control forever.
 * - **`Picker`** is one button and a list that opens, for the sets too long to show at once.
 *
 * **Keyboard parity is the requirement, not the courtesy.** A native select is reachable by tab
 * and driven by arrows, and a replacement that is not would be a regression dressed as a
 * redesign. `Choice` is one tab stop with arrow keys inside it, which is what a radio group is;
 * `Picker` opens on Enter, Space, or Down, moves on arrows, commits on Enter, and abandons on
 * Escape, which is what a select does.
 */
import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from 'react'

export interface Option {
  value: string
  label: string
  /** What this option is, where that is worth saying at all. Drawn on the option, never beside it. */
  detail?: string
}

/** Where the arrow keys go next, wrapping, which is what a radio group does. */
function step(options: readonly Option[], from: string, delta: number): string {
  const at = options.findIndex((option) => option.value === from)
  const next = (at + delta + options.length) % options.length
  return options[next < 0 ? 0 : next]?.value ?? from
}

/**
 * Every option on screen, one of them chosen.
 *
 * `columns` lays the options out as rows rather than as a strip — the shape a list of servers,
 * each with a region, or a list of opponents, each with a description, actually wants.
 */
export function Choice({
  label,
  value,
  options,
  onChange,
  columns = false,
}: {
  /** The accessible name of the group. It is also what the visible legend says, when there is one. */
  label: string
  value: string
  options: readonly Option[]
  onChange(value: string): void
  columns?: boolean
}) {
  const group = useRef<HTMLDivElement>(null)
  // A group where nothing is chosen yet still has to be reachable by tab, so the first option
  // holds the group's one tab stop until a choice is made.
  const focused = options.some((option) => option.value === value) ? value : options[0]?.value

  // Arrow keys move the choice *and* the focus, together: in a radio group the focused option is
  // the chosen one, and a group where they can disagree is one a keyboard cannot read back.
  const move = (delta: number) => {
    const next = step(options, value, delta)
    onChange(next)
    const index = options.findIndex((option) => option.value === next)
    group.current?.querySelectorAll('button')[index]?.focus()
  }

  return (
    <div
      ref={group}
      role="radiogroup"
      aria-label={label}
      className={`choice${columns ? ' choice--columns' : ''}`}
      onKeyDown={(event) => {
        const delta =
          event.key === 'ArrowRight' || event.key === 'ArrowDown'
            ? 1
            : event.key === 'ArrowLeft' || event.key === 'ArrowUp'
              ? -1
              : 0
        if (delta === 0) return
        event.preventDefault()
        move(delta)
      }}
    >
      {options.map((option) => {
        const chosen = option.value === value
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={chosen}
            // One tab stop for the whole group, as a radio group has: tab reaches the choice,
            // arrows move within it, and tab leaves.
            tabIndex={option.value === focused ? 0 : -1}
            className={`choice__option${chosen ? ' choice__option--on' : ''}`}
            onClick={() => onChange(option.value)}
          >
            <span className="choice__label">{option.label}</span>
            {option.detail && <span className="choice__detail">{option.detail}</span>}
          </button>
        )
      })}
    </div>
  )
}

/**
 * One button, and a list that opens under it.
 *
 * For the sets `Choice` cannot show at once — a catalog's keywords are the case that exists
 * today. The list is a real listbox with real focus in it rather than a menu of divs, so a
 * screen reader announces the same thing a select would and a keyboard drives it the same way.
 */
export function Picker({
  label,
  value,
  options,
  onChange,
  placeholder,
}: {
  label: string
  value: string
  options: readonly Option[]
  onChange(value: string): void
  /** What the button says when nothing is chosen. */
  placeholder?: string
}) {
  const [open, setOpen] = useState(false)
  const root = useRef<HTMLDivElement>(null)
  const listId = useId()
  const chosen = options.find((option) => option.value === value)

  const close = useCallback((focus: boolean) => {
    setOpen(false)
    if (focus) root.current?.querySelector('button')?.focus()
  }, [])

  // A list that stays open after the pointer has gone elsewhere is a list nobody dismissed.
  useEffect(() => {
    if (!open) return
    const onDown = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [open])

  const focusOption = (index: number) => {
    const buttons = root.current?.querySelectorAll<HTMLButtonElement>('[role="option"]')
    const wrapped = ((index % options.length) + options.length) % options.length
    buttons?.[wrapped]?.focus()
  }

  return (
    <div ref={root} className="picker">
      <button
        type="button"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        className="picker__button"
        onClick={() => setOpen((was) => !was)}
        onKeyDown={(event) => {
          if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
          event.preventDefault()
          setOpen(true)
          // The list has not rendered yet on the keypress that opens it.
          requestAnimationFrame(() => focusOption(event.key === 'ArrowDown' ? 0 : -1))
        }}
      >
        <span className="picker__value">{chosen?.label ?? placeholder ?? label}</span>
        <span className="picker__caret" aria-hidden="true">
          ▾
        </span>
      </button>

      {open && (
        <ul
          id={listId}
          role="listbox"
          aria-label={label}
          className="picker__list"
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault()
              close(true)
            }
          }}
        >
          {options.map((option, index) => (
            <li key={option.value}>
              <button
                type="button"
                role="option"
                aria-selected={option.value === value}
                className={`picker__option${option.value === value ? ' picker__option--on' : ''}`}
                onClick={() => {
                  onChange(option.value)
                  close(true)
                }}
                onKeyDown={(event) => {
                  const delta = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0
                  if (delta === 0) return
                  event.preventDefault()
                  focusOption(index + delta)
                }}
              >
                <span className="choice__label">{option.label}</span>
                {option.detail && <span className="choice__detail">{option.detail}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/**
 * A labelled text field.
 *
 * A text input is the one native control that stays: it draws no glyph of its own to clip, it
 * inherits the page's type, and every alternative to it is a `contenteditable` that would have
 * to reimplement selection, dictation, and autofill to get back to where it started.
 */
export function TextField({
  label,
  value,
  onChange,
  placeholder,
  maxLength,
  autoFocus,
  hint,
  onEnter,
}: {
  label: string
  value: string
  onChange(value: string): void
  placeholder?: string
  maxLength?: number
  autoFocus?: boolean
  /** Drawn under the field, for what the field cannot say itself. */
  hint?: ReactNode
  onEnter?(): void
}) {
  const id = useId()
  return (
    <p className="field">
      <label className="field__label" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        className="field__input"
        value={value}
        placeholder={placeholder}
        maxLength={maxLength}
        autoFocus={autoFocus}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && onEnter) {
            event.preventDefault()
            onEnter()
          }
        }}
      />
      {hint && <span className="field__hint">{hint}</span>}
    </p>
  )
}
