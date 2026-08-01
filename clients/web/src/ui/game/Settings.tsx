/**
 * Everything about *this device* rather than about this game, behind one control.
 *
 * Three panels used to be three buttons in the header band — pace, keys, art — and a header
 * carrying seventeen small controls is a header nobody reads. They are one panel now, opened by
 * one gear, because they share the only property that matters for placement: none of them is
 * about the board, and none of them is needed while a decision is being made.
 *
 * The pace is the exception that proves it. It *is* about the game, and it stays here anyway,
 * because it is set between turns rather than during them and its keys work without opening
 * anything (`keys.ts`). What is printed beside each control is the key that does the same thing,
 * which is how a player finds out there is a keyboard at all.
 */
import { useEffect } from 'react'

import { BINDINGS } from './../../keys'
import type { StopPreset } from './../../turn'
import { ArtControls } from './../ArtSettings'
import { PacePresets } from './PacePresets'

export function Settings({
  preset,
  onPreset,
  onClose,
}: {
  preset?: StopPreset
  onPreset(preset: StopPreset): void
  onClose(): void
}) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    <div className="inspector-backdrop" onClick={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        className="inspector settings"
        onClick={(event) => event.stopPropagation()}
      >
        <section aria-labelledby="pace-heading">
          <h2 id="pace-heading">Pace</h2>
          <p className="settings__note">
            How far the game runs before it stops for you. Each of these replaces the whole
            preference the server honours; the turn steps down the left edge set it one step at a
            time.
          </p>
          <PacePresets current={preset} onPreset={onPreset} />
        </section>

        <section aria-labelledby="keys-heading">
          <h2 id="keys-heading">Keyboard</h2>
          <dl className="shortcuts__list">
            {BINDINGS.map((binding) => (
              <div key={binding.keys} className="shortcuts__row">
                <dt>
                  <kbd>{binding.keys}</kbd>
                </dt>
                <dd>{binding.does}</dd>
              </div>
            ))}
          </dl>
        </section>

        <section aria-labelledby="art-heading">
          <h2 id="art-heading">Card art</h2>
          <ArtControls />
        </section>

        <button type="button" className="inspector__close" onClick={onClose} autoFocus>
          Close
        </button>
      </div>
    </div>
  )
}
