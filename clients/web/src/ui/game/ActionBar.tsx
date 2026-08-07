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
 *
 * Three of its controls are the announcement (§6.7), and all three draw exactly what the server
 * sent: a **mode** as numbered rows, one per mode, whose numeral is its key; **X** as a stepper
 * over the values the server enumerated, showing that value's stated cost; and a **cost the game
 * has changed** as the number a player pays beside the one the card still prints. Nothing here
 * adds a number to another or works out what a spell costs.
 */
import { useEffect, useLayoutEffect, useRef } from 'react'

import type { CardFace } from './../../card-face'
import { dockCandidates, type BarTone } from './../../dock'
import {
  answer,
  fill,
  remainingCost,
  stepTo,
  stepperAt,
  waysToPay,
  type Interaction,
  type Slot,
} from './../../interaction'
import { manaSymbols, spokenSymbol } from './../../mana'
import type { ActionCost, ValidAction } from './../../protocol'
import type { ManaPip } from './../../table'
import type { Settle } from './../../settle'
import { fit, tooWide } from './../fit'
import { Pip } from './../card/Pips'
import { SettleBand } from './SettleBand'
import { Symbols } from './../card/Symbols'

/** One cost, as the pips it is printed in. */
function Cost({ cost }: { cost: string | undefined }) {
  const symbols = manaSymbols(cost)
  if (symbols.length === 0) return <span className="pay-none">—</span>
  return (
    <>
      {symbols.map((symbol, i) => (
        <Pip key={i} symbol={symbol.glyph} label={spokenSymbol(symbol)} />
      ))}
    </>
  )
}

/**
 * A mode's own sentence, set on one line at the largest size that holds it.
 *
 * §7 sacrifices size and then line count, and never completeness: the sentence is fitted between
 * whatever size the stylesheet gives it — read back after clearing, so a short window's tighter
 * type is the size this starts from and nothing here has to know that tier exists — and the
 * 11px floor. A mode too wordy for one line at the floor takes a second line rather than losing
 * its end.
 */
const MODE_FLOOR = 11

function ModeLabel({ text }: { text: string }) {
  const ref = useRef<HTMLSpanElement>(null)
  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    el.style.whiteSpace = 'nowrap'
    el.style.fontSize = ''
    const ceiling = Number.parseFloat(window.getComputedStyle(el).fontSize)
    if (Number.isFinite(ceiling) && ceiling > MODE_FLOOR) fit(ref, ceiling, MODE_FLOOR, tooWide)
    if (tooWide(el)) el.style.whiteSpace = 'normal'
  })
  return (
    <span className="mode-text" ref={ref}>
      <Symbols text={text} />
    </span>
  )
}

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
  badged,
  buttons,
  paying,
  cost,
  pool,
  labelFor,
  settle,
  update,
  confirm,
  cancel,
  restart,
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
  /** The ids another surface is drawing **with their position in an ordering** (`dock.ts`). */
  badged: ReadonlySet<string>
  /** What the settle did before this view, when the server said it acted for this seat. */
  settle?: Settle
  /** The actions no object owns: pass, and whatever else the server offered globally. */
  buttons: readonly ValidAction[]
  /**
   * The card the player said they are playing, while its cost is being made. Absent whenever
   * they are not — including the moment the card leaves the hand, which is how it ends.
   */
  paying?: CardFace
  /**
   * What the cast in question costs, printed and as the game has it now (`ActionCost`).
   *
   * Present only where the server stated it — on a cast. The two halves are drawn side by side
   * and compared for nothing but *being the same string*: which of them is the larger number is
   * arithmetic on a cost, and `docs/protocol.md` says outright that this client parses neither.
   */
  cost?: ActionCost
  /** What this seat currently has floating, as the server stated it. */
  pool: readonly ManaPip[]
  labelFor(id: string): string
  update(next: Interaction): void
  confirm(): void
  cancel(): void
  /**
   * Take back every answer without leaving the question. Absent when there is nothing to take
   * back — a payment intent has no answers, and neither does an untouched draft.
   */
  restart?(): void
  take(action: ValidAction): void
}) {
  // A slot whose own controls are the answers states itself; every other one has nothing on
  // screen naming what it wants, so it says it once.
  const asking = action !== undefined
  // What the card prints, and what the game will charge. The server states both; where it has
  // stated neither — a card named for payment the server has not yet offered a cast for — the
  // card's own printed cost is all there is, and it is what the bar shows.
  const printed = cost?.printed ?? paying?.manaCost
  const charged = cost?.modified ?? paying?.manaCost
  // A cost the game has changed, which is a string comparison and the whole of the judgment
  // this client makes about it. **Not which way it moved**: reading one cost as less than
  // another means valuing every symbol in both, and valuing a cost is computing one. The two
  // numbers are on screen side by side and labelled, which is what says the direction (§6.7).
  const changed =
    cost !== undefined && (cost.printed ?? '') !== '' && cost.printed !== cost.modified
  const showCost = (!asking && paying !== undefined) || changed
  // The cost still owed, and the question this client is posing itself about a source that can
  // pay the pip being filled more than one way.
  const owed = remainingCost(slots)
  const pending = interaction.asking
  const pendingSlot = pending && slots.find((slot) => slot.slot === pending.slot)
  const askingWhich =
    pending && pendingSlot
      ? { slot: pending.slot, ways: waysToPay(pendingSlot, pending.source) }
      : undefined

  // A stepper stands somewhere from the moment it is drawn, and where it stands is the answer.
  // The server's own first value is where it starts (`stepperAt`), so the slot holds it without
  // the player having to press a control to say "yes, that one" — and the submission the bar
  // will build carries the value the player is looking at rather than nothing at all.
  const unanswered = slots.find(
    (slot) => slot.kind === 'number' && slot.values !== undefined && slot.chosen.length === 0,
  )
  const opening = unanswered && stepperAt(unanswered)
  useEffect(() => {
    if (!unanswered || !opening) return
    update(answer(interaction, unanswered.slot, [String(opening.value)]))
  })

  return (
    <div className={`action-bar action-${tone}`} role="region" aria-label="Actions">
      {/* What the settle did, drawn *over* the board's bottom edge rather than in a row of its
          own (§6.9). Height is the scarce axis here — every seat, both field rows, the hand and
          the chrome are on screen at once at every supported size (§3) — so a band that took a
          grid row would take it from the board, and one that appeared and vanished would resize
          the field under the player's pointer. It costs no layout at all. */}
      <SettleBand {...(settle ? { settle } : {})} />
      <div className="action-text">
        <span className="action-prompt">
          <Symbols text={asking ? action.label : paying ? `Pay for ${paying.name}` : prompt} />
        </span>
        <span className="action-phase">{where}</span>
      </div>

      {/* Paying, said the only way a cost can be said: what the cast costs, and what is
          floating so far. Neither is compared against the other — that is arithmetic about a
          rule, and the server answers it by offering the cast or not offering it. Confirm goes
          live the moment it does.

          Where the game has **changed** what a cast costs (§6.7), the number a player acts on is
          the modified one and the printed one stays on screen beside it, marked and labelled, so
          the difference is legible without the mark. The mark says only *that* it changed: which
          way is carried by the two numbers themselves, and green and red are already spoken for
          by the bar's own tones (§6.5). The modified cost is what a screen reader is given
          first, and the mark carries no fact alone (§5.5). */}
      {showCost && (
        <div
          className="action-pay"
          role="group"
          aria-label={paying ? `Paying for ${paying.name}` : 'Cost'}
        >
          <span className={`pay-part${changed ? ' pay-changed' : ''}`}>
            <span className="pay-label">{changed ? 'costs now' : 'Cost'}</span>
            <Cost cost={charged} />
          </span>
          {changed && (
            <span className="pay-part pay-printed">
              <span className="pay-label">card says</span>
              <Cost cost={printed} />
            </span>
          )}
          {!asking && paying && (
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
          )}
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
                : dockCandidates(slot, drawn, badged).map((id) => ({ id, label: labelFor(id) }))
            // A pip is answered by clicking a source on the board, and the line above already
            // says which pips are still owed. Its own row would be the third copy — and the one
            // that grows with the board, since every untapped land can pay a generic pip.
            if (slot.kind === 'mana' && candidates.length === 0) return null
            const at = slot.kind === 'number' ? stepperAt(slot) : undefined
            return (
              <span
                key={slot.slot}
                className={`slot${slot.numbered ? ' slot-rows' : ''}`}
                role="group"
                aria-label={slot.prompt}
              >
                {/* The mode, as full-width rows the numeral on each one selects (§6.7). One row
                    per mode the server offered, carrying the mode's own generated sentence; the
                    bound is three and the catalog validator is what keeps it there, so there is
                    no ladder here for a fourth. Choosing one sends nothing — it fills a slot, and
                    the target slots that mode owes appear because `requires` named them. */}
                {slot.numbered ? (
                  <span className="action-modes">
                    {slot.options.map((option, index) => {
                      const chosen = slot.chosen.includes(option.id)
                      return (
                        <button
                          key={option.id}
                          className={`mode-row${chosen ? ' mode-chosen' : ''}`}
                          aria-pressed={chosen}
                          aria-keyshortcuts={`${index + 1}`}
                          onClick={() => update(answer(interaction, slot.slot, [option.id]))}
                        >
                          <span className="mode-num" aria-hidden="true">
                            {index + 1}
                          </span>
                          <ModeLabel text={option.label} />
                        </button>
                      )
                    })}
                  </span>
                ) : at ? (
                  /* X, as a stepper over the values the server enumerated (§6.7). The controls
                     walk that list and stop at its ends; the cost beside the value is the one
                     the server stated for it. Nothing here adds, multiplies, or compares a
                     number — a client that worked out what `{X}{R}` costs would be deciding
                     what a spell costs. */
                  <span className="slot-step">
                    <span className="slot-ask">
                      <Symbols text={slot.prompt} />
                    </span>
                    <button
                      className="step-btn"
                      aria-label="Lower"
                      disabled={stepTo(slot, -1) === undefined}
                      onClick={() => {
                        const next = stepTo(slot, -1)
                        if (next !== undefined) update(answer(interaction, slot.slot, [next]))
                      }}
                    >
                      −
                    </button>
                    <span className="step-value" role="status">
                      {at.value}
                    </span>
                    <button
                      className="step-btn"
                      aria-label="Higher"
                      disabled={stepTo(slot, 1) === undefined}
                      onClick={() => {
                        const next = stepTo(slot, 1)
                        if (next !== undefined) update(answer(interaction, slot.slot, [next]))
                      }}
                    >
                      +
                    </button>
                    {at.cost !== undefined && (
                      <span className="pay-part">
                        <span className="pay-label">costs</span>
                        <Cost cost={at.cost} />
                      </span>
                    )}
                  </span>
                ) : slot.kind === 'number' ? (
                  /* A number that costs nothing — how many counters, how much of a divided
                     effect — is a value in a range the server stated and no list of stops. */
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
            {/* Two ways out, and they are different questions rather than two depths of one.
                *Start again* empties the answers and stays in the question — a combat
                declaration aimed at the wrong things is undone in a click without also undoing
                "I am declaring attackers" — and it is offered only while there is something to
                empty. *Cancel* leaves outright, every time: a player who has decided not to
                cast this spell should not have to press one button twice to stop being asked
                about it, and since nothing was sent, nothing is undone. */}
            {restart && (
              <button className="action-done action-alt" onClick={restart}>
                Start again
              </button>
            )}
            <button className="action-done action-alt" onClick={cancel}>
              Cancel
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
