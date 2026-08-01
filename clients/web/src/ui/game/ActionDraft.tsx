/**
 * The questions one armed action is asking, and the answers being built for them.
 *
 * Every control here is generated from a slot the server published: its prompt words, its
 * candidate ids, and — where it gave one — how many of them it will take. Nothing decides
 * whether a choice is *good*, and nothing narrows a candidate list. A slot the server said may
 * be under-filled says so in as many words, because "you may pick fewer" is invisible otherwise
 * and a player who cannot tell will sit on a finished answer waiting for a button to light up.
 *
 * Every slot that takes objects is answerable twice over: by clicking the highlighted object on
 * the table, or by clicking its name here. The list is not a fallback for taste — it is the only
 * path to a candidate the table cannot show, such as a card still in a library.
 */
import type { Interaction, Slot } from './../../interaction'
import { answer, fill } from './../../interaction'
import type { ValidAction } from './../../protocol'

export interface DraftProps {
  action: ValidAction
  slots: readonly Slot[]
  ready: boolean
  /** A submission is in flight, so nothing may be sent. */
  blocked: boolean
  interaction: Interaction
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

export function ActionDraft({
  action,
  slots,
  ready,
  blocked,
  interaction,
  labelFor,
  update,
  confirm,
  cancel,
}: DraftProps) {
  return (
    <section className="choices" aria-label="Choices">
      <h3 className="choices__what">{action.label}</h3>

      {slots.map((slot) => {
        const status = tally(slot)
        return (
          <fieldset key={slot.slot} className="slot">
            <legend>
              {slot.prompt}
              {status && <span className="slot__tally"> — {status}</span>}
            </legend>

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
              <>
                {slot.byEntity && (
                  <p className="slot__hint">Click a highlighted object, or choose one here.</p>
                )}
                <ul className="actions">
                  {(slot.kind === 'option'
                    ? slot.options
                    : slot.candidates.map((id) => ({ id, label: labelFor(id) }))
                  ).map((candidate) => {
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
                          {candidate.label}
                        </button>
                      </li>
                    )
                  })}
                </ul>
              </>
            )}
          </fieldset>
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
