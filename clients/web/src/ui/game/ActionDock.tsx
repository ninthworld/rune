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
 * Three lists survive that contextual path on purpose. **What the game is waiting on** comes
 * first and only when the game is in fact waiting: an action a player cannot decline is not a
 * thing to go looking for, so while one is owed the dock holds it out rather than leaving a
 * player to guess which card is asking. **Global actions** — pass, concede, the combat
 * declarations, a mulligan — own no object, so there is nothing on the table to click and the
 * dock is their home. **Every action** is the disclosure below them, and it lists all of them,
 * including the ones reachable from a card. A subject is not guaranteed to be visible: it may be
 * a card inside a collapsed pile, or an id in no rendered zone at all. Until contextual coverage
 * is *proven* rather than assumed, the flat list stays, and nothing becomes unreachable because
 * a surface did not happen to draw it (#626).
 *
 * One action is asked twice. Conceding ends the match and nothing in the game undoes it, so it
 * takes over the dock with a question rather than firing on the click that reached it — and any
 * other click is a "no", because every other transition drops the question (`interaction.ts`).
 *
 * **Its band is `scene()`'s, and its contents scale into it** (`docs/client-design.md` §6.5 rule
 * 5, §3). The box responds to *whether* the game is asking and never to how much there is to ask
 * about; on half the supported viewports it is 44px even then, so the type, the padding, and the
 * gaps are handed down from `dockDensity` as one scale rather than each element deciding its own.
 * That is the ladder §3 gave every region and the dock's contents never had — and it is why the
 * answer to a question that will not fit is now a smaller question, never a cut one.
 */
import type { CSSProperties } from 'react'

import type { GameResult, ValidAction } from './../../protocol'
import type { Rect } from './../../scene'
import { dockDensity, dockNarrates, dockTone, dockWording } from './../../dock'
import {
  actionsFor,
  clear,
  disarm,
  focus,
  globalActions,
  owedActions,
  release,
  unask,
  type Interaction,
} from './../../interaction'
import { RulesText } from './../RulesText'
import { ActionDraft } from './ActionDraft'

export interface DockProps {
  actions: readonly ValidAction[]
  interaction: Interaction
  result: GameResult | undefined
  /** The band `scene()` allocated, which is the whole of what the contents scale into. */
  box: Rect
  /**
   * The ids the table is currently drawing.
   *
   * Presentation and nothing else: it says which objects this client put a box on, so a question
   * about them can be answered on them (§6.5) and the dock can carry the ones it did not draw.
   */
  drawn: ReadonlySet<string>
  /** Turn and step, restated where the controls are — see the band's note below. */
  where: string
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
  box,
  drawn,
  where,
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
  const owed = owedActions(actions)
  const blocked = interaction.pending !== undefined
  // Still offered? An action awaiting confirmation can be withdrawn by the next view like any
  // other, and asking about one the server no longer lists would be asking about nothing.
  const asking = actions.find((action) => action.id === interaction.confirming)

  const button = (action: ValidAction) => (
    <li key={action.id}>
      <button type="button" onClick={() => take(action)} disabled={blocked}>
        {/* An action's label is server text like any other — `{T}: Add {G}.` is a button here
            as often as it is a line on a card, and the same symbol must not look like two
            different things depending on where it is read. */}
        <RulesText text={action.label} />
        {/* Server-computed (CR 605), so a mana ability can be offered as the one-click gesture
            it is without the client ever classifying an ability itself. */}
        {action.mana_ability ? ' ⟨mana⟩' : ''}
      </button>
    </li>
  )

  const tone = dockTone(actions, interaction, result)
  const density = dockDensity(box.height)

  return (
    <div
      className={`dock dock--${tone}`}
      // One scale for the whole band (§7), handed down from the box `scene()` allocated rather
      // than measured off the content — the contents scale into the region, never the reverse.
      style={
        {
          '--dock-text': `${density.text}px`,
          '--dock-pad-y': `${density.padY}px`,
          '--dock-pad-x': `${density.padX}px`,
          '--dock-gap': `${density.gap}px`,
          '--dock-row-gap': `${density.rowGap}px`,
        } as CSSProperties
      }
    >
      <section aria-labelledby="actions-heading">
        <h2 id="actions-heading">Actions</h2>

        {/* What the game wants, in colour and in words, on the band a player's eyes rest on
            between actions. The colour is what makes it answerable from peripheral vision; the
            words are what make it answerable at all, because a colour nobody has learnt yet, a
            colour two of which look alike, and a colour a screen reader cannot see all say
            nothing on their own (`dock.ts`).

            Drawn only where nothing below it is already stating the question (§6.5 rule 2): a
            draft's own controls are the question, and a confirmation asks in as many words, so
            adding "the game is waiting on your answer" above either is the same fact twice and
            it is a row of height the band does not have to spare.

            The step is restated here rather than only in the header. It is the question asked
            most often after "is it my turn", the header is the full width of the screen away
            from the controls, and a player mid-decision should not have to look up. */}
        {dockNarrates(actions, interaction, result) && (
          <p className="dock__tone" role="status">
            <span className="dock__tone-mark" aria-hidden="true" />
            <strong>{dockWording(tone)}</strong>
            <span className="dock__where">{where}</span>
          </p>
        )}

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
            drawn={drawn}
            labelFor={labelFor}
            update={update}
            confirm={confirm}
            // Back rather than away: the object stays selected, so cancelling a mis-armed
            // action returns to that object's other actions instead of an empty dock.
            cancel={() => update(disarm(interaction))}
          />
        ) : (
          <>
            {/* What the game will not proceed past, held out rather than left to be found on the
                board. The band above already says the game is waiting on an answer, so this does
                not say it a second time in a box of its own (§2.1 rules 4 and 5) — the emphasis
                is on the controls themselves, which is where a player is going to click. */}
            {owed.length > 0 && (
              <ul className="actions actions--owed" aria-label="Actions you owe">
                {owed.map(button)}
              </ul>
            )}

            {selected === undefined ? (
              // Suppressed while something is owed: the buttons are already here, and telling a
              // player to go find a card is the opposite of what this state needs.
              owed.length === 0 && (
                <p className="dock__hint">Click a highlighted card or player to act on it.</p>
              )
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
