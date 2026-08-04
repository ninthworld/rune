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
import type { CardFace } from './../../card-face'
import { dockCandidates, type BarTone } from './../../dock'
import {
  answer,
  fill,
  remainingCost,
  waysToPay,
  type Interaction,
  type Slot,
} from './../../interaction'
import { manaSymbols, spokenSymbol } from './../../mana'
import type { ValidAction } from './../../protocol'
import type { ManaPip } from './../../table'
import { Pip } from './../card/Pips'
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
  paying,
  pool,
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
  /**
   * The card the player said they are playing, while its cost is being made. Absent whenever
   * they are not — including the moment the card leaves the hand, which is how it ends.
   */
  paying?: CardFace
  /** What this seat currently has floating, as the server stated it. */
  pool: readonly ManaPip[]
  labelFor(id: string): string
  update(next: Interaction): void
  confirm(): void
  cancel(): void
  take(action: ValidAction): void
}) {
  // A slot whose own controls are the answers states itself; every other one has nothing on
  // screen naming what it wants, so it says it once.
  const asking = action !== undefined
  const cost = manaSymbols(paying?.manaCost)
  // The cost still owed, and the question this client is posing itself about a source that can
  // pay the pip being filled more than one way.
  const owed = remainingCost(slots)
  const pending = interaction.asking
  const pendingSlot = pending && slots.find((slot) => slot.slot === pending.slot)
  const askingWhich =
    pending && pendingSlot
      ? { slot: pending.slot, ways: waysToPay(pendingSlot, pending.source) }
      : undefined

  return (
    <div className={`action-bar action-${tone}`} role="region" aria-label="Actions">
      <div className="action-text">
        <span className="action-prompt">
          <Symbols text={asking ? action.label : paying ? `Pay for ${paying.name}` : prompt} />
        </span>
        <span className="action-phase">{where}</span>
      </div>

      {/* Paying, said the only way a cost can be said: the cost as printed, and what is
          floating so far. Neither is compared against the other — that is arithmetic about a
          rule, and the server answers it by offering the cast or not offering it. Confirm goes
          live the moment it does. */}
      {!asking && paying && (
        <div className="action-pay" role="group" aria-label={`Paying for ${paying.name}`}>
          <span className="pay-part">
            <span className="pay-label">Cost</span>
            {cost.length === 0 ? (
              <span className="pay-none">—</span>
            ) : (
              cost.map((symbol, i) => (
                <Pip key={i} symbol={symbol.glyph} label={spokenSymbol(symbol)} />
              ))
            )}
          </span>
          <span className="pay-part" role="status">
            <span className="pay-label">Floating</span>
            {pool.length === 0 ? (
              <span className="pay-none">nothing yet — tap a source</span>
            ) : (
              pool.flatMap((pip, index) =>
                manaSymbols(pip.symbol).map((symbol, i) => (
                  <Pip
                    key={`${index}:${i}`}
                    symbol={symbol.glyph}
                    label={`${spokenSymbol(symbol)}${pip.restricted ? ', restricted' : ''}`}
                  />
                )),
              )
            )}
          </span>
        </div>
      )}

      {/* What is still owed, as pips — the unanswered `pay_mana` slots and nothing else. No
          cost is subtracted from anything here: a pip is drawn while its slot is empty and
          stops being drawn when it is filled, which is the whole of the arithmetic. */}
      {asking && owed.length > 0 && (
        <div className="action-pay" role="group" aria-label="Still to pay">
          <span className="pay-part" role="status">
            <span className="pay-label">Pay</span>
            {owed.flatMap((pip, index) =>
              manaSymbols(pip).map((symbol, i) => (
                <Pip key={`${index}:${i}`} symbol={symbol.glyph} label={spokenSymbol(symbol)} />
              )),
            )}
          </span>
        </div>
      )}

      {/* The dual-land question. The server listed one permanent twice for this pip, which is
          the entire reason there is anything to ask; the answers are its own labels. */}
      {askingWhich && (
        <div className="action-slots" role="group" aria-label="Which mana?">
          <span className="slot">
            <span className="slot-ask">Tap for</span>
            {askingWhich.ways.map((way) => (
              <button
                key={way.id}
                className="action-done action-alt"
                onClick={() => update(answer(interaction, askingWhich.slot, [way.id]))}
              >
                <Symbols text={way.label} />
              </button>
            ))}
          </span>
        </div>
      )}

      {asking && (
        <div className="action-slots">
          {slots.map((slot) => {
            const candidates =
              slot.kind === 'option'
                ? slot.options
                : dockCandidates(slot, drawn).map((id) => ({ id, label: labelFor(id) }))
            // A pip is answered by clicking a source on the board, and the line above already
            // says which pips are still owed. Its own row would be the third copy — and the one
            // that grows with the board, since every untapped land can pay a generic pip.
            if (slot.kind === 'mana' && candidates.length === 0) return null
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
                          onClick={() => update(fill(interaction, slot, candidate.id, slots))}
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
        {asking || paying ? (
          <>
            {/* One control, two depths: it takes back the answers first and leaves the
                question, and only lets go of the question once there is nothing to take back.
                A combat declaration aimed at the wrong things is undone in a click without
                also undoing "I am declaring attackers". */}
            <button className="action-done action-alt" onClick={cancel}>
              {asking && Object.values(interaction.draft).some((ids) => ids.length > 0)
                ? 'Start again'
                : 'Cancel'}
            </button>
            <button className="action-done" disabled={!ready || blocked} onClick={confirm}>
              {asking && slots.some((slot) => slot.kind === 'mana') ? 'Cast' : 'Confirm'}
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
    case 'mana':
      // The pip line above already says what is still owed, and each pip's own controls say
      // what can pay it. A count here would be the same fact a third time.
      return ''
  }
}
