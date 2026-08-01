/**
 * The keyboard, as intents.
 *
 * A game whose every move costs a trip to a button in the corner is slow in a way no amount of
 * layout fixes. The single most-pressed control in a game of Magic is "I have nothing to do
 * here, go on" — pressed hundreds of times a game — and it belongs under a thumb.
 *
 * The bindings follow XMage's, because that is the vocabulary players arriving here already
 * have: a confirm key that both answers the current question and passes when there is none, an
 * escape that backs out, and a small ladder of skip keys that decide how far the game runs
 * before it asks again. What differs is the mechanism behind the ladder, and the difference is
 * deliberate. XMage's skips are a client-side "stop asking me until X". Here they are the
 * server's own stop preference (`turn.ts`, ADR 0010): the key sends one `set_stops` and one
 * pass, and the pacing that follows is the server's settle acting on a preference it stores.
 * Nothing in this client loops, waits, or passes on the player's behalf — which is the seam the
 * project protects most carefully, and a keyboard shortcut is not a reason to cross it.
 *
 * This module is pure and knows nothing about the game. It turns a keypress into a name for
 * something the player asked for; whether that thing is currently possible is decided where the
 * view is (`Game.tsx`), out of `valid_actions` like everything else.
 */
import type { StopPreset } from './turn'

/** What the player asked for. */
export type Intent =
  | { kind: 'confirm' }
  | { kind: 'cancel' }
  | { kind: 'stops'; preset: StopPreset }
  | { kind: 'help' }

/**
 * One keypress, reduced to what the binding depends on.
 *
 * `typing` and `onControl` are about *where the key landed*, and they are the whole reason this
 * takes a description rather than a `KeyboardEvent`: a shortcut that fires while someone is
 * filling in the X of an X spell is a bug, and one that fires while the browser is already
 * activating the button under the keyboard is a double action.
 */
export interface KeyPress {
  key: string
  ctrl: boolean
  meta: boolean
  alt: boolean
  shift: boolean
  /** Focus is in something that takes text, so every key belongs to it and none belongs here. */
  typing: boolean
  /** Focus is on a control the browser itself activates with Enter. */
  onControl: boolean
}

/**
 * What a keypress means, or nothing.
 *
 * Modified keypresses are always nothing: `Ctrl+Space` and `Cmd+F5` belong to the browser and to
 * the operating system, and a game that swallowed them would be taking keys it does not own.
 */
export function intentFor(press: KeyPress): Intent | undefined {
  if (press.ctrl || press.meta || press.alt) return undefined
  if (press.typing) return undefined

  switch (press.key) {
    // The one key that carries the game. Taken even when a control has focus, because a player
    // who has just clicked a card still has that card focused and still means "go on" — which is
    // why the caller must suppress the browser's own space-activates-the-button, and why Enter
    // below deliberately does not do the same.
    case ' ':
    case 'F2':
      return { kind: 'confirm' }

    // Enter says the same thing, but yields: on a button it is the browser's to handle, and
    // taking it would make every focused control do two things at once.
    case 'Enter':
      return press.onControl ? undefined : { kind: 'confirm' }

    case 'Escape':
      return { kind: 'cancel' }

    // The ladder, from most stops to fewest. `F3` is XMage's "cancel my skips" and means the
    // same thing here: stop deciding for me.
    case 'F3':
      return { kind: 'stops', preset: 'everywhere' }
    case 'F4':
      return { kind: 'stops', preset: 'mains' }
    case 'F5':
      return { kind: 'stops', preset: 'nowhere' }

    case '?':
      return { kind: 'help' }

    default:
      return undefined
  }
}

/**
 * Whether the browser's own handling of this key has to be suppressed.
 *
 * Only where this client is taking a key the browser would otherwise act on itself: space
 * scrolls a page and activates a focused button, and `F5` reloads. Everything else is left
 * alone, because a page that blocks keys it does not use is a page a player cannot escape.
 */
export const claims = (press: KeyPress, intent: Intent | undefined): boolean =>
  intent !== undefined && (press.key === ' ' || press.key.startsWith('F'))

/** The bindings, in the order they are worth learning. The help panel renders this. */
export const BINDINGS: readonly { keys: string; does: string }[] = [
  { keys: 'Space', does: 'Confirm what you are drafting, or pass priority' },
  { keys: 'Enter', does: 'Confirm — unless a button has focus, which the browser handles' },
  { keys: 'Esc', does: 'Close what is open, or back out of the action you armed' },
  { keys: 'F3', does: 'Stop at every step — the way back from any skip' },
  { keys: 'F4', does: 'Stop at your main phases, and pass' },
  { keys: 'F5', does: 'Stop only where the game must ask, and pass' },
  { keys: '?', does: 'This list' },
  { keys: 'Right-click', does: 'Read any card, whatever else is in progress' },
]
