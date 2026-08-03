/**
 * The bar above the hand: what the game is asking, and the answer that is not on the board
 * (`docs/client-design.md` §6.5).
 *
 * **Permanent.** It does not trade places with the hand and it does not resize for the question
 * — its shape is fixed, and what changes inside it is the wording and the buttons.
 *
 * **The board answers; this stays small.** A question about objects on the screen is answered on
 * the screen: the objects that can answer it are highlighted, clicking one answers it, and this
 * carries only what the board cannot — a tally of what has been chosen against what is needed,
 * the way to commit, the way to cancel, and a control for every subject no surface drew
 * (`dockCandidates`). That is why a question with twenty legal blockers is the same height as a
 * question with two.
 *
 * **The prompt is drawn once.** Where the server's own options state the question — *Keep this
 * hand* · *Mulligan* — the options are the question, and nothing is written above them.
 *
 * The tone is the turn's (`dock.barTone`), and it carries no fact alone: the prompt says what is
 * being asked and the line under it says which step you are in.
 */
import { dockCandidates, type BarTone } from './../../dock'
import { answer, fill, type Interaction, type Slot } from './../../interaction'
import type { ValidAction } from './../../protocol'
import { Symbols } from './../card/Symbols'

export function ActionBar({
  tone,
  prompt,
  where,
  action,
  slots,
  ready,
  blocked,
  interaction,
  drawn,
  buttons,
  labelFor,
  update,
  confirm,
  cancel,
  take,
}: {
  tone: BarTone
  /** What the game is waiting on, in words. */
  prompt: string
  /** Which turn and step this is — the fact the tone draws. */
  where: string
  /** The action being drafted, when one is armed. */
  action?: ValidAction
  slots: readonly Slot[]
  ready: boolean
  /** A submission is in flight, so nothing may be sent. */
  blocked: boolean
  interaction: Interaction
  /** The ids the table is currently drawing, so this carries only the ones it is not. */
  drawn: ReadonlySet<string>
  /** The actions no object owns: pass, and whatever else the server offered globally. */
  buttons: readonly ValidAction[]
  labelFor(id: string): string
  update(next: Interaction): void
  confirm(): void
  cancel(): void
  take(action: ValidAction): void
}) {
  // A slot whose own controls are the answers states itself; every other one has nothing on
  // screen naming what it wants, so it says it once.
  const asking = action !== undefined

  return (
    <div className={`action-bar action-${tone}`} role="region" aria-label="Actions">
      <div className="action-text">
        <span className="action-prompt">
          <Symbols text={asking ? action.label : prompt} />
        </span>
        <span className="action-phase">{where}</span>
      </div>

      {asking && (
        <div className="action-slots">
          {slots.map((slot) => {
            const candidates =
              slot.kind === 'option'
                ? slot.options
                : dockCandidates(slot, drawn).map((id) => ({ id, label: labelFor(id) }))
            return (
              <span key={slot.slot} className="slot" role="group" aria-label={slot.prompt}>
                {slot.kind === 'number' ? (
                  <label className="slot-number">
                    <span className="slot-ask">
                      <Symbols text={slot.prompt} />
                    </span>
                    <input
                      type="number"
                      min={slot.range?.min}
                      max={slot.range?.max}
                      value={slot.chosen[0] ?? ''}
                      onChange={(event) =>
                        update(
                          answer(
                            interaction,
                            slot.slot,
                            event.target.value ? [event.target.value] : [],
                          ),
                        )
                      }
                    />
                  </label>
                ) : (
                  <>
                    {slot.kind !== 'option' && (
                      // The tally is the one thing that changes as the board is clicked, and a
                      // player answering on the table is not looking here when it does.
                      <span className="slot-ask" role="status">
                        <Symbols text={slot.prompt} />
                        <b>{tally(slot)}</b>
                      </span>
                    )}
                    {candidates.map((candidate) => {
                      const position = slot.chosen.indexOf(candidate.id)
                      return (
                        <button
                          key={candidate.id}
                          className={`action-done action-alt${position >= 0 ? ' action-chosen' : ''}`}
                          aria-pressed={position >= 0}
                          onClick={() => update(fill(interaction, slot, candidate.id))}
                        >
                          {slot.kind === 'order' && position >= 0 && `${position + 1}. `}
                          <Symbols text={candidate.label} />
                        </button>
                      )
                    })}
                  </>
                )}
              </span>
            )
          })}
        </div>
      )}

      <div className="action-btns">
        {asking ? (
          <>
            <button className="action-done action-alt" onClick={cancel}>
              Cancel
            </button>
            <button className="action-done" disabled={!ready || blocked} onClick={confirm}>
              Confirm
            </button>
          </>
        ) : (
          buttons.map((entry, index) => (
            <button
              key={entry.id}
              className={`action-done${index < buttons.length - 1 ? ' action-alt' : ''}`}
              disabled={blocked}
              onClick={() => take(entry)}
            >
              <Symbols text={entry.label} />
            </button>
          ))
        )}
      </div>
    </div>
  )
}

/** What this slot is holding, against what the server said it would take. */
function tally(slot: Slot): string {
  const held = slot.chosen.length
  switch (slot.kind) {
    case 'target':
      // A requirement carries no count — the server enforces the arity on resolution — so the
      // honest thing to report is what is held, plus the one thing it did say.
      return slot.optional ? ` ${held} chosen · may be left empty` : ` ${held} chosen`
    case 'order':
      return ` ${held} of ${slot.candidates.length}, in order`
    case 'number':
      return ` ${slot.range?.min}–${slot.range?.max}`
    case 'option':
      return ''
    case 'zone':
      return slot.min === slot.max
        ? ` ${held} of ${slot.max}`
        : ` ${held} of ${slot.min ?? 0}–${slot.max}`
  }
}
