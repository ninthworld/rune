/**
 * The keys, written down.
 *
 * Every shortcut this client binds has a control on screen that does the same thing, so nothing
 * here is the only way to reach anything. What a list like this is for is the other direction:
 * telling a player who has been clicking through a hundred priority passes that they did not
 * have to.
 *
 * Rendered from `BINDINGS` rather than written out again, so a key that changes cannot leave a
 * help panel quietly describing the previous build.
 */
import { useEffect } from 'react'

import { BINDINGS } from './../../keys'

export function Shortcuts({ onClose }: { onClose(): void }) {
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
        aria-label="Keyboard shortcuts"
        className="inspector shortcuts"
        onClick={(event) => event.stopPropagation()}
      >
        <h2>Keyboard</h2>
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
        <p className="shortcuts__note">
          The skip keys set the stop preference the server honours and pass once. The game moves on
          its own from there, and says where it acted for you beside the board.
        </p>
        <button type="button" className="inspector__close" onClick={onClose} autoFocus>
          Close
        </button>
      </div>
    </div>
  )
}
