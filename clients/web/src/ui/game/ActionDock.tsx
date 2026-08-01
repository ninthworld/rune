/**
 * The strip along the bottom where acting happens.
 *
 * Pinned, and never scrolled away by anything above it: the controls that move the game must be
 * in the same place whether the board is empty or full. That is the constraint the rest of the
 * layout is sized around.
 *
 * What it shows follows the click, not a fixed inventory. Select an object on the table and this
 * offers exactly the actions the server attached to it; arm one and this becomes the questions
 * that action is asking. Everything a player does still ends in one `choose_action` built from
 * ids the server issued.
 *
 * Two lists survive that contextual path on purpose. **Global actions** — pass, concede, the
 * combat declarations, a mulligan — own no object, so there is nothing on the table to click and
 * the dock is their home. **Every action** is the disclosure below it, and it lists all of them,
 * including the ones reachable from a card. A subject is not guaranteed to be visible: it may be
 * a card inside a collapsed pile, or an id in no rendered zone at all. Until contextual coverage
 * is *proven* rather than assumed, the flat list stays, and nothing becomes unreachable because
 * a surface did not happen to draw it (#626).
 *
 * One action is asked twice. Conceding ends the match and nothing in the game undoes it, so it
 * takes over the dock with a question rather than firing on the click that reached it — and any
 * other click is a "no", because every other transition drops the question (`interaction.ts`).
 */
import type { GameResult, ValidAction } from './../../protocol'
import {
  actionsFor,
  clear,
  disarm,
  focus,
  globalActions,
  release,
  unask,
  type Interaction,
} from './../../interaction'
import { ActionDraft } from './ActionDraft'

export interface DockProps {
  actions: readonly ValidAction[]
  interaction: Interaction
  result: GameResult | undefined
  labelFor(id: string): string
  /** Start this action — immediately if it asks nothing, otherwise as a draft. */
  take(action: ValidAction): void
  update(next: Interaction): void
  /** Send what is drafted. */
  confirm(): void
  inspect(id: string): void
}

export function ActionDock({
  actions,
  interaction,
  result,
  labelFor,
  take,
  update,
  confirm,
  inspect,
}: DockProps) {
  const current = focus(actions, interaction)
  const selected = interaction.selected
  const owned = selected === undefined ? [] : actionsFor(actions, selected)
  const globals = globalActions(actions)
  const blocked = interaction.pending !== undefined
  // Still offered? An action awaiting confirmation can be withdrawn by the next view like any
  // other, and asking about one the server no longer lists would be asking about nothing.
  const asking = actions.find((action) => action.id === interaction.confirming)

  const button = (action: ValidAction) => (
    <li key={action.id}>
      <button type="button" onClick={() => take(action)} disabled={blocked}>
        {action.label}
        {/* Server-computed (CR 605), so a mana ability can be offered as the one-click gesture
            it is without the client ever classifying an ability itself. */}
        {action.mana_ability ? ' ⟨mana⟩' : ''}
      </button>
    </li>
  )

  return (
    <div className="dock">
      <section aria-labelledby="actions-heading">
        <h2 id="actions-heading">Actions</h2>

        {interaction.pending && (
          <p role="status" className="notice dock__pending">
            Sent “{interaction.pending.label}” — waiting for the server.{' '}
            <button type="button" onClick={() => update(release(interaction))}>
              Stop waiting
            </button>
          </p>
        )}

        {interaction.rejected && (
          <p role="status" className="notice dock__rejected">
            “{interaction.rejected}” was refused. Nothing changed — this is the current state.
          </p>
        )}

        {actions.length === 0 ? (
          // A finished game is not a game that is waiting: nobody is coming, and saying
          // otherwise leaves a player watching a screen that will never change.
          <p>{result ? 'Nothing to do — the game is over.' : 'Waiting for the other seat.'}</p>
        ) : asking ? (
          // The whole dock, so the question cannot be answered by clicking past it. Nothing has
          // been sent and nothing is drafted — this is one button asked twice.
          <div className="dock__confirm" role="group" aria-label="Confirm">
            <p>
              <strong>{asking.label}?</strong> This ends the game for you, and nothing undoes it.
            </p>
            <p>
              <button type="button" onClick={() => take(asking)} disabled={blocked}>
                Yes, {asking.label.toLowerCase()}
              </button>{' '}
              <button type="button" onClick={() => update(unask(interaction))}>
                Keep playing
              </button>
            </p>
          </div>
        ) : current.action ? (
          <ActionDraft
            action={current.action}
            slots={current.slots}
            ready={current.ready}
            blocked={blocked}
            interaction={interaction}
            labelFor={labelFor}
            update={update}
            confirm={confirm}
            // Back rather than away: the object stays selected, so cancelling a mis-armed
            // action returns to that object's other actions instead of an empty dock.
            cancel={() => update(disarm(interaction))}
          />
        ) : (
          <>
            {selected === undefined ? (
              <p className="dock__hint">Click a highlighted card or player to act on it.</p>
            ) : (
              <div className="dock__subject">
                <p className="dock__who">
                  <strong>{labelFor(selected)}</strong>
                </p>
                {owned.length > 0 ? (
                  <ul className="actions" aria-label="Actions for the selected object">
                    {owned.map(button)}
                  </ul>
                ) : (
                  <p>Nothing to do with this one right now.</p>
                )}
                <p>
                  {/* Inspection is never a game action, so it stays offered on the selection
                      even while an object that has actions gives its first click to them. */}
                  <button type="button" onClick={() => inspect(selected)}>
                    Inspect
                  </button>{' '}
                  <button type="button" onClick={() => update(clear(interaction))}>
                    Clear selection
                  </button>
                </p>
              </div>
            )}

            {globals.length > 0 && (
              <ul className="actions" aria-label="Global actions">
                {globals.map(button)}
              </ul>
            )}

            <details className="dock__all">
              <summary>Every action ({actions.length})</summary>
              <ul className="actions" aria-label="Every action">
                {actions.map(button)}
              </ul>
            </details>
          </>
        )}
      </section>
    </div>
  )
}
