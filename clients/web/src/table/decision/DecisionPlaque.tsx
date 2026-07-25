/**
 * The **decision plaque** (`docs/design/control-language.md` §10 and §3.1's last
 * row; control-ui baseline panel 7 — issue #534, under
 * [ADR 0032](../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * ADR 0032 removed the permanent bottom action dock. The plaque is what takes
 * over the dock's *role* — the home for a decision's confirm / cancel / advance —
 * without taking over its *permanence*: it exists only while a decision is open,
 * and it is placed next to that decision's subject by
 * {@link ./plaqueAnchor.placeDecisionPlaque}.
 *
 * ## The division of labour with the prompt strip
 *
 * §10.1 is explicit and this component holds the line: **the strip carries the
 * sentence, the plaque carries the controls.** The question, the "Target 2 of 3"
 * progress, the running count, and the deadline all stay in `PromptStrip` at its
 * fixed home on the hand panel's top edge. Nothing here restates them. Merging
 * the two is tempting — the plaque is next to the subject and the strip is not —
 * and it is exactly what §10.1 forbids, because a sentence that follows the
 * subject around is a sentence a player has to hunt for.
 *
 * ## What it refuses to decide
 *
 * The plaque renders the server's prompt and echoes back an `action_id` plus the
 * picks the session already recorded. It computes no legality, no cardinality,
 * and no enablement: `confirm.disabledReason` arrives from the caller, which
 * derives it from `multiSelect.allSlotsSatisfied` over the server's own `count`.
 * That reason is a **string**, never a boolean, because {@link ControlButton}
 * cannot render disabled without printing the server's stated reason (§3.2, D14,
 * GAP-4) — a greyed control with no reason is a bug, and the type makes it
 * unwritable.
 *
 * ## UNDO is not a takeback
 *
 * §8 and GAP-1: `undo` is **only** the local retract-one-pick control. There is
 * no post-submission undo — `ChooseAction` is final, no `undo` exists in
 * `valid_actions`, and the protocol carries no message for one. The prop is named
 * for the drawn pill and documented here so nobody wires it to a takeback that
 * does not exist. It renders only while there is a pick to retract; in the
 * neutral state the pill does not render at all (§8, C8).
 *
 * ## Cancel, and the decision that offers none
 *
 * §8's taxonomy is normative. `cancel` is optional because a decision the view
 * forces — a mulligan, a cleanup discard — offers **no cancel**: there is no
 * neutral state to return to, and a control that immediately re-opens what it
 * just closed is worse than no control (shipped behaviour, #451). §10.2 calls the
 * plaque "dismissible" at every geometry; §8 is the more specific rule and wins.
 * Dismissal *is* this control, plus the shell's own `Escape` binding — which
 * `useTableKeyboard` owns, so this component installs no key handler and cannot
 * disagree with it.
 *
 * ## Input parity (§7)
 *
 * Every control is a real `<button>`, so the click, `Enter`/`Space`, and tap
 * paths are the same path, and the focus ring `controls.module.css` draws is
 * never suppressed. Each is floored at 44 px by `ControlButton`'s hit box. There
 * is no hover-only and no drag-only affordance here.
 */
import { cx } from '../../chrome/cx';
import { ControlButton } from '../controls';
import type { PlaquePlacement } from './plaqueAnchor';
import s from './plaque.module.css';

/** The green CONFIRM. Absent while targeting: the last pick auto-submits (§4.2 rule 2). */
export interface PlaqueConfirm {
  /**
   * The drawn label — `action.label` verbatim, or the session's confirm label.
   * The client does not name the server's actions.
   */
  label: string;
  onConfirm: () => void;
  /**
   * The server's stated reason the confirm is not yet available, rendered
   * verbatim beside the label. Today the only case is an unsatisfied cardinality
   * — exactly `count` ids not yet chosen (§9 storyboard 9, the one server-stated
   * disablement). Undefined means enabled; there is no other way to grey it.
   */
  disabledReason?: string;
}

/** The red CANCEL. Absent for a view-forced decision (§8, D19). */
export interface PlaqueCancel {
  /** Defaults to "Cancel"; supply the server's word when it has one. */
  label?: string;
  onCancel: () => void;
}

export interface DecisionPlaqueProps {
  /**
   * The plaque's title — the action's server `label`, printed verbatim
   * ("CHOOSE ATTACKERS" in the baseline). Not a sentence the client composes.
   */
  title: string;
  /**
   * Where to draw it. From {@link placeDecisionPlaque}; the component applies it
   * as inline `left`/`top`/`width` and never chooses a position of its own.
   */
  placement: PlaquePlacement;
  /** The confirm control, when the decision has one. Always renders leading. */
  confirm?: PlaqueConfirm;
  /** Advance to the next walked slot ("Next"), when a later slot is in play. */
  onAdvance?: () => void;
  /**
   * Retract the last pick — the local one-step retract of §8, and nothing more.
   * See the module note: this is not, and may not become, a takeback (GAP-1).
   */
  onUndo?: () => void;
  /** The cancel control, when the decision may be abandoned. Always renders trailing. */
  cancel?: PlaqueCancel;
  testId?: string;
}

/**
 * The plaque. Rendered by `LiveMatchTable` as a **sibling of the shell's
 * regions** — see the stylesheet's note on stacking contexts; rendered inside a
 * region it would be trapped below that region's rung and could be covered by
 * the very chrome ADR 0032's ladder exists to keep off it.
 */
export function DecisionPlaque({
  title,
  placement,
  confirm,
  onAdvance,
  onUndo,
  cancel,
  testId = 'decision-plaque',
}: DecisionPlaqueProps) {
  const { rect, form, side } = placement;

  return (
    <div
      className={cx(s.plaque, form === 'sheet' && s.sheet)}
      data-testid={testId}
      data-form={form}
      data-side={side}
      // §10.2: pointer-transparent where it hosts no control, so the candidates
      // underneath stay tappable. The attribute mirrors the shipped
      // `data-pointer-through` contract (#451) so the behaviour is assertable.
      data-pointer-through="true"
      // The plaque IS the pending decision, so it announces itself as one: an
      // assistive reader reaches the controls without hunting, and the title
      // names the decision the strip is describing in words.
      role="group"
      aria-label={title}
      style={{
        left: rect.x,
        top: rect.y,
        width: rect.w,
        // The sheet's height is capped by the placement (40 % of the viewport)
        // and must be honoured; an anchored plaque grows to its content.
        height: form === 'sheet' ? rect.h : undefined,
      }}
    >
      <div className={s.plate}>
        <span className={s.title} data-testid={`${testId}-title`}>
          {title}
        </span>
        <div className={s.controls}>
          {/* §11: confirm always LEADS and cancel always TRAILS. The order is
              the non-colour channel for the pair — the one cue that survives a
              colour-blind path, so it is not a layout preference. */}
          {confirm && (
            <ControlButton
              variant="confirm"
              label={confirm.label}
              onPress={confirm.onConfirm}
              disabledReason={confirm.disabledReason}
              testId={`${testId}-confirm`}
            />
          )}
          {onAdvance && (
            <ControlButton
              variant="secondary"
              label="Next"
              onPress={onAdvance}
              accessibleName="Next slot"
              testId={`${testId}-advance`}
            />
          )}
          {onUndo && (
            <ControlButton
              variant="utility"
              label="Undo"
              onPress={onUndo}
              // The drawn word is "UNDO"; what it does is retract one pick, and
              // §8 says that is all it ever does. The accessible name says so,
              // because "undo" to a screen-reader user promises a takeback.
              accessibleName="Retract the last pick"
              testId={`${testId}-undo`}
            />
          )}
          {cancel && (
            <ControlButton
              variant="cancel"
              label={cancel.label ?? 'Cancel'}
              onPress={cancel.onCancel}
              testId={`${testId}-cancel`}
            />
          )}
        </div>
      </div>
    </div>
  );
}
