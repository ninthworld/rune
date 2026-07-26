/**
 * The **decision area** — the one surface that presents and answers a decision
 * (issue #567, under
 * [ADR 0032](../../../../docs/decisions/0032-contextual-shell-anatomy.md);
 * `docs/design/control-language.md` §3, §7, §8, §10).
 *
 * ## What it replaces, and why one surface
 *
 * Three surfaces used to draw one question. `PromptStrip` carried the sentence,
 * `DecisionSheet` re-drew the same sentence over the option buttons, and
 * `DecisionPlaque` floated a third copy of the title beside them — control-less
 * for a forced mulligan, because with named options in play its confirm was
 * undefined and, being forced, so was its cancel. The player was asked once and
 * told three times, from a surface that could not answer.
 *
 * #567 makes the lower-right corner **one action area**: the primary, the
 * utilities, the mana reservoir, and the phase plaque in the control cluster, and
 * this surface stacked directly above them whenever the server is waiting on an
 * answer. The question, its progress, its rows, its named choices, and its
 * controls are all here, drawn from one derivation
 * ({@link deriveDecision}) so they cannot disagree.
 *
 * This supersedes §10.1's split of "the strip carries the sentence, the plaque
 * carries the controls" and its §10.1/D17 anchoring walk. That design placed the
 * controls next to their subject and left the sentence at a fixed home; #567's
 * goal statement — "one coherent lower-right action area for authoritative
 * decisions" and "one 2.5D-native choice surface **adjacent to the primary
 * action**" — is the later instruction, and the anchoring it made moot is removed
 * rather than left half-wired (see this directory's `index.ts`).
 *
 * ## The layer, and why it is a sibling of the shell's regions
 *
 * The area sits on `--rune-z-decision`. ADR 0032's binding rule is that *a layer
 * may only be covered by a layer the player explicitly invoked and can dismiss
 * without answering it*, and a pending decision is neither — issue #528 shipped
 * the proof, a mulligan painted over by chrome and left unanswerable. A region
 * carrying a `z-index` creates a stacking context, so a surface rendered inside
 * `.scene` / `.hand` / `.cluster` is trapped at that region's rung whatever it
 * declares. `LiveMatchTable` therefore mounts this at the shell root, and
 * `LiveMatchTable.occlusion.test.tsx` asserts it.
 *
 * ## Mulligan bottoming still happens on the cards
 *
 * The area never covers the receiver's hand. On the full composition it stands in
 * the cluster's column, whose 268 px the hand band already yields; on the compact
 * composition it rests its bottom edge on the hand band's top edge. Both are
 * expressed in the `--shell-*` geometry `shellLayout.ts` publishes, so the
 * clearance is the shipped geometry rather than a guess (`decision.module.css`).
 * A slot whose candidates are on the board or in the hand lights them there and
 * draws no rows here — {@link deriveDecision} decides which, once.
 *
 * ## What it refuses to do
 *
 * It renders the server's words and echoes back an `action_id` plus the picks the
 * session recorded. It computes no legality, no cardinality, and no enablement:
 * every `disabledReason` arrives from the derivation, which reads the server's
 * own `count` and `requires`. Every control is a real `<button>`, so click,
 * `Enter`/`Space`, and tap are one path and the focus ring is never suppressed
 * (§7, D15); each is floored at 44 px by `ControlButton`. It installs no key
 * handler — `useTableKeyboard` owns `Escape` for the whole shell, so the two
 * cannot disagree about what one press cancels.
 *
 * A decision the view forces offers no cancel (§8/D19, #451): there is no neutral
 * state to return to, and a control that instantly re-opens what it closed is
 * worse than no control. The derivation says so; this component only obeys.
 */
import { SymbolText, symbolNotationText } from '../../chrome/symbols';
import type { EntityId } from '../../protocol';
import { ControlButton } from '../controls';
import { DeadlineCountdown } from '../DeadlineCountdown';
import { NumberPromptSurface } from '../NumberPromptSurface';
import { PromptSurface } from '../PromptSurface';
import type { DecisionSurface } from './decisionSurface';
import s from './decision.module.css';

export interface DecisionAreaProps {
  /** The one derived model. The component draws it and nothing else. */
  surface: DecisionSurface;
  /** Submit the assembled multi-select answer. */
  onConfirm: () => void;
  /** Walk to the next in-play slot. */
  onAdvance: () => void;
  /** §8's local retract-one-pick. Never a takeback — no such message exists (GAP-1). */
  onUndo: () => void;
  /** Abandon the session locally. Nothing is sent; nothing was. */
  onCancel: () => void;
  /** Toggle a candidate of a list-answered slot. */
  onToggleRow: (id: EntityId) => void;
  /** Move a row one step earlier (`-1`) or later (`+1`) in an `order` slot. */
  onMoveRow: (id: EntityId, direction: -1 | 1) => void;
  /** Answer an `option` prompt with the chosen option id. */
  onChooseOption: (optionId: string) => void;
  /** Set the active `number` slot's value (issue #554). The caller clamps it. */
  onNumber: (value: number) => void;
  testId?: string;
}

export function DecisionArea({
  surface,
  onConfirm,
  onAdvance,
  onUndo,
  onCancel,
  onToggleRow,
  onMoveRow,
  onChooseOption,
  onNumber,
  testId = 'decision-area',
}: DecisionAreaProps) {
  const { title, prompt, progress, count, deadline, rows, number, choices } = surface;

  return (
    <section
      className={s.area}
      data-testid={testId}
      data-kind={surface.kind}
      // The area IS the pending decision, so it announces itself as one and the
      // title names what the sentence below is about.
      role="group"
      aria-label={symbolNotationText(title)}
      // The region's own box takes no pointer events; the plate inside it does.
      // Nothing underneath is ever swallowed by empty surface (#451).
      data-pointer-through="true"
    >
      <div className={s.frame}>
        <div className={s.plate}>
          <header className={s.head} role="status">
            {/* The heading is dropped when it would repeat the phase plaque's
                current step name (#586): during Declare Attackers both surfaces
                drew "DECLARE ATTACKERS" a few hundred pixels apart in two
                treatments, and one of the two has to say something the other
                does not. The plaque is the phase surface; this one is the
                question. The `aria-label` on the section still carries the
                title, so nothing is lost to a reader who cannot see the plaque
                beside it, and the words are never rewritten — only not printed
                twice. */}
            {!surface.titleEchoesPhase && (
              <span className={s.title} data-testid={`${testId}-title`}>
                <SymbolText text={title} />
              </span>
            )}
            {/* THE sentence. It is drawn here and nowhere else — the whole point
                of #567's first bullet, and what `decisionSurface.test.ts` and the
                shell's "asks once" test pin. */}
            <span className={s.prompt} data-testid="decision-prompt" data-decision-prompt="true">
              <SymbolText text={prompt} />
            </span>
            {(progress !== undefined || count !== undefined || deadline !== undefined) && (
              <span className={s.meta}>
                {progress !== undefined && <span data-testid="decision-progress">{progress}</span>}
                {count !== undefined && <span data-testid="decision-count">{count}</span>}
                {deadline !== undefined && <DeadlineCountdown seconds={deadline} />}
              </span>
            )}
          </header>

          {/* A slot answered in a list rather than on the board: an `order`
              arrangement, or a zone the board does not show (graveyard, library).
              The rows scroll inside the plate rather than growing the surface. */}
          {rows && (
            <div className={s.rows}>
              <PromptSurface
                mode={rows.mode}
                prompt={prompt}
                zone={rows.zone}
                items={rows.items}
                onToggle={onToggleRow}
                onMove={onMoveRow}
              />
            </div>
          )}

          {/* A `number` slot (issue #554) brings its own control: it has no
              candidates, so neither the board nor a row list can answer it. The
              surface offers exactly the server's `min`..`max` — the same range
              `setActiveNumber` clamps to — and the Confirm below submits it, which
              is why the slot opens pre-filled at the minimum. This is the surface
              #554 wired into the retired `DecisionSheet`; #567 moved it here with
              the rest of the question rather than leaving it with the deletion. */}
          {number && (
            <div className={s.rows}>
              <NumberPromptSurface
                prompt={number.prompt}
                min={number.min}
                max={number.max}
                value={number.value}
                onChange={onNumber}
              />
            </div>
          )}

          {/* The named choices, in the control language rather than the retired
              sheet's bespoke buttons (#567: "named choices use the new visual
              language beside the primary action"). Pressing one IS the answer,
              which is why a decision with choices carries no separate confirm.
              They render as equal-weight secondaries: several options offered on
              equal terms is §4.2 rules 4/7's tie, and promoting one of them
              would be the client ranking the server's choices — the same refusal
              `controlPrimary.ts` makes for the cluster's blue slot (#586). */}
          {choices && choices.length > 0 && (
            <div className={s.choices} data-testid="multiselect-options">
              {choices.map((choice) => (
                <ControlButton
                  key={choice.id}
                  variant="secondary"
                  label={choice.label}
                  onPress={() => onChooseOption(choice.id)}
                  disabledReason={choice.disabledReason}
                  testId={`multiselect-option-${choice.id}`}
                />
              ))}
            </div>
          )}

          {(surface.confirm || surface.advance || surface.undo || surface.cancel) && (
            <div className={s.controls}>
              {/* §11: confirm always LEADS and cancel always TRAILS. The order is
                  the pair's non-colour channel — the cue that survives a
                  colour-blind path — so it is not a layout preference.

                  Confirm wears the BLUE primary, not a second primary hue
                  (#586). §4.2 rule 1 already says this control "carries the
                  advance" while a decision is open, and the cluster's blue slot
                  is deliberately empty for exactly that reason — so this is the
                  one primary on screen, and the product has one primary colour
                  from CONNECT through SUBMIT DECK to here. The compact form is
                  the pair width the row is built for. */}
              {surface.confirm && (
                <ControlButton
                  variant="primaryCompact"
                  label="Confirm"
                  onPress={onConfirm}
                  disabledReason={surface.confirmDisabledReason}
                  testId={`${testId}-confirm`}
                />
              )}
              {surface.advance && (
                <ControlButton
                  variant="secondary"
                  label="Next"
                  onPress={onAdvance}
                  accessibleName="Next slot"
                  testId={`${testId}-advance`}
                />
              )}
              {surface.undo && (
                <ControlButton
                  variant="utility"
                  label="Undo"
                  onPress={onUndo}
                  // The drawn word is "UNDO"; what it does is retract one pick,
                  // and §8 says that is all it ever does. The accessible name
                  // says so, because "undo" promises a takeback that does not
                  // exist (GAP-1).
                  accessibleName="Retract the last pick"
                  testId={`${testId}-undo`}
                />
              )}
              {surface.cancel && (
                <ControlButton
                  variant="cancel"
                  label="Cancel"
                  onPress={onCancel}
                  testId={`${testId}-cancel`}
                />
              )}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
