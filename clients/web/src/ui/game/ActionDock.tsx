/**
 * The strip along the bottom where acting happens.
 *
 * Pinned, and never scrolled away by anything above it: the controls that move the game must be
 * in the same place whether the board is empty or full. That is the constraint the rest of the
 * layout is sized around.
 *
 * The action list is still the flat, global one the server advertised. Making it contextual —
 * clicking the card rather than reading its name off a button — is #626; until then this is the
 * complete and usable path, which is what keeps the table playable while it is being rebuilt.
 */
import type { ClientMessage, GameResult, ValidAction } from './../../protocol'
import { ActionPanel } from './../ActionPanel'

export function ActionDock({
  actions,
  result,
  labelFor,
  send,
}: {
  actions: readonly ValidAction[]
  result: GameResult | undefined
  labelFor(id: string): string
  send(message: ClientMessage): void
}) {
  return (
    <div className="dock">
      {actions.length > 0 ? (
        <ActionPanel actions={actions} labelFor={labelFor} send={send} />
      ) : (
        <section aria-labelledby="waiting-heading">
          <h2 id="waiting-heading">Actions</h2>
          {/* A finished game is not a game that is waiting: nobody is coming, and saying
              otherwise leaves a player watching a screen that will never change. */}
          <p>{result ? 'Nothing to do — the game is over.' : 'Waiting for the other seat.'}</p>
        </section>
      )}
    </div>
  )
}
