/**
 * The questions one armed action is asking, and the answers being built for them.
 *
 * Every control here is generated from a slot the server published: its prompt words, its
 * candidate ids, and — where it gave one — how many of them it will take. Nothing decides
 * whether a choice is *good*, and nothing narrows a candidate list. A slot the server said may
 * be under-filled says so in as many words, because "you may pick fewer" is invisible otherwise
 * and a player who cannot tell will sit on a finished answer waiting for a button to light up.
 *
 * **The board answers, and this carries what the board cannot** (`docs/client-design.md` §6.5).
 * A slot's candidates are highlighted where they lie and a click on one answers the slot the
 * server listed it in, so a subject that is drawn is not *also* a button here — that second copy
 * is what made a question with twenty legal blockers twenty controls tall, in a band that is
 * 44px on half the viewports this client supports. What is left is what a board cannot state: a
 * tally of chosen against needed, the way to commit, the way to cancel, and a control for every
 * subject no surface drew (`dockCandidates`).
 *
 * **A question is asked once** (§2.1 rule 5, §6.5 rule 1). This used to draw the action's label
 * as a heading *and* each slot's prompt as a legend *and* the options underneath both, which is
 * one question written three times. The heading is gone; the prompt is drawn where the controls
 * do not already state it, and where they do — a set of options whose own labels are the answers
 * — the options are the question. The action's label stays in the accessible name of the panel,
 * because a screen reader has no height problem and asking "what am I answering" costs it a
 * gesture otherwise.
 */
import type { Interaction, Slot } from './../../interaction'
import { RulesText } from './../RulesText'
import { answer, fill } from './../../interaction'
import { dockCandidates } from './../../dock'
import type { ValidAction } from './../../protocol'

export interface DraftProps {
  action: ValidAction
  slots: readonly Slot[]
  ready: boolean
  /** A submission is in flight, so nothing may be sent. */
  blocked: boolean
  interaction: Interaction
  /** The ids the table is currently drawing, so this carries only the ones it is not. */
  drawn: ReadonlySet<string>
  labelFor(id: string): string
  update(next: Interaction): void
  confirm(): void
  cancel(): void
}

/** What this slot is holding, against what the server said it would take. */
function tally(slot: Slot): string {
  const held = slot.chosen.length
  switch (slot.kind) {
    case 'target':
      // A requirement carries no count — the server enforces the arity on resolution — so the
      // honest thing to report is what is held, plus the one thing it did say.
      return slot.optional ? `${held} chosen · may be left empty` : `${held} chosen`
    case 'order':
      return `${held} of ${slot.candidates.length}, in order`
    case 'number':
      return `${slot.range?.min}–${slot.range?.max}`
    case 'option':
      return ''
    case 'zone':
      return slot.min === slot.max
        ? `${held} of ${slot.max}`
        : `${held} of ${slot.min ?? 0}–${slot.max}`
  }
}

/**
 * Whether this slot's own controls already say what it is asking.
 *
 * The server's options are the answers written out — *Keep this hand*, *Mulligan*, *Pay 1* — so a
 * sentence above them saying "keep this hand or take a mulligan?" is the same question a second
 * time. Every other slot has nothing on screen that names what it wants, so it says it, once.
 */
const statesItself = (slot: Slot): boolean => slot.kind === 'option'

export function ActionDraft({
  action,
  slots,
  ready,
  blocked,
  interaction,
  drawn,
  labelFor,
  update,
  confirm,
  cancel,
}: DraftProps) {
  return (
    // The label carries the question for anyone who is not looking at the highlighted subject.
    // Named "Choices" first so the panel keeps one stable handle whatever is being asked.
    <section className="choices" aria-label={`Choices: ${action.label}`}>
      {slots.map((slot) => {
        const status = tally(slot)
        const asked = status ? `${slot.prompt} — ${status}` : slot.prompt
        // Options are the server's own; everything else is an id, and the ones the table drew are
        // answered on the table.
        const candidates =
          slot.kind === 'option'
            ? slot.options
            : dockCandidates(slot, drawn).map((id) => ({ id, label: labelFor(id) }))

        return (
          <div key={slot.slot} className="slot" role="group" aria-label={asked}>
            {/* The tally is a live region: it is the one thing that changes as the board is
                clicked, and a player answering on the table is not looking here when it does. */}
            {!statesItself(slot) && (
              <p className="slot__ask" role="status">
                <RulesText text={slot.prompt} />
                {status && <span className="slot__tally"> — {status}</span>}
              </p>
            )}

            {slot.kind === 'number' ? (
              // The bounds are the server's, computed from mana, the source's text, and the
              // state. The control offers exactly that range and works out no affordability.
              <label className="slot__number">
                <span className="visually-hidden">{slot.prompt}</span>
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
              candidates.length > 0 && (
                <ul className="actions">
                  {candidates.map((candidate) => {
                    const position = slot.chosen.indexOf(candidate.id)
                    return (
                      <li key={candidate.id}>
                        <button
                          type="button"
                          aria-pressed={position >= 0}
                          className={position >= 0 ? 'chosen' : undefined}
                          onClick={() => update(fill(interaction, slot, candidate.id))}
                        >
                          {/* An order is a permutation, so where a thing sits in it is the
                              answer and has to be readable from the control itself. */}
                          {slot.kind === 'order' && position >= 0 && `${position + 1}. `}
                          <RulesText text={candidate.label} />
                        </button>
                      </li>
                    )
                  })}
                </ul>
              )
            )}
          </div>
        )
      })}

      <p className="choices__controls">
        {/* Enabled only once every slot the server put a count on holds a count it published.
            A slot it left uncounted never blocks: guessing an arity it declined to state is
            the client asserting a rule (`submission.ts`). */}
        <button type="button" onClick={confirm} disabled={!ready || blocked}>
          Confirm
        </button>{' '}
        <button type="button" onClick={cancel}>
          Cancel
        </button>
      </p>
    </section>
  )
}
