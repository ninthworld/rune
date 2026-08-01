/**
 * How far the game runs before it asks you again.
 *
 * The turn strip beside this sets a stop one step at a time, which is the right control for
 * "hand me priority in my opponent's end step" and the wrong one for the thing a player wants
 * between two turns: a pace. These are the three paces, and they are the same `set_stops`
 * message the strip sends — one that replaces the whole preference rather than editing a step
 * of it (`turn.ts`).
 *
 * They are drawn as buttons *and* bound to keys, in that order of importance. A shortcut nobody
 * can find is a shortcut nobody uses, so the key is printed on the control it duplicates, which
 * is also how a player learns there is a keyboard here at all.
 *
 * Which one is on is read off the view. A preference edited step by step matches none of them and
 * none of the buttons claims to be pressed, which is the honest answer — there is no client-held
 * idea of a "current pace" to disagree with the server about.
 */
import { presetWording, type StopPreset } from './../../turn'

/** The key each preset is bound to (`keys.ts`), printed on the control it duplicates. */
const KEYS: Record<StopPreset, string> = { everywhere: 'F3', mains: 'F4', nowhere: 'F5' }

/** Short enough for a row of three; the full sentence is the accessible name. */
const SHORT: Record<StopPreset, string> = {
  everywhere: 'Every step',
  mains: 'My mains',
  nowhere: 'Only when asked',
}

const ORDER: readonly StopPreset[] = ['everywhere', 'mains', 'nowhere']

export function PacePresets({
  current,
  onPreset,
  onHelp,
  onArt,
}: {
  /** The preset the server's effective lists currently match, if they match one. */
  current?: StopPreset
  onPreset(preset: StopPreset): void
  onHelp(): void
  onArt(): void
}) {
  return (
    <div className="pace" role="group" aria-label="Pace">
      <span className="pace__label">Stops</span>
      {ORDER.map((preset) => (
        <button
          key={preset}
          type="button"
          className="pace__preset"
          aria-pressed={current === preset}
          aria-label={presetWording(preset)}
          onClick={() => onPreset(preset)}
        >
          {SHORT[preset]} <kbd>{KEYS[preset]}</kbd>
        </button>
      ))}
      <button type="button" className="pace__keys" onClick={onHelp}>
        Keys <kbd>?</kbd>
      </button>
      {/* Beside the keys because both are settings about *this device* rather than about the
          game, and the header is the one region on the table that is never about a card. */}
      <button type="button" className="pace__keys" onClick={onArt}>
        Art
      </button>
    </div>
  )
}
