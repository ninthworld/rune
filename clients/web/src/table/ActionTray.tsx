/**
 * The floating action tray (ADR 0004; issue #298).
 *
 * Restages the old detached action bar as a tray that floats just above the hand
 * (`regions.tray`), reading as part of the board rather than a document row. It stays
 * O(1) in board size: it holds only global, subject-less actions (pass, end turn)
 * plus a contextual echo of the currently selected entity's actions. Per-card actions
 * are NEVER enumerated here — they render on the entity (see `EntityOverlay`). Every
 * button corresponds to an entry in `valid_actions[]`; the tray computes no legality.
 *
 * The tray also hosts the decision controls that used to live in a separate bar —
 * multi-select confirm/advance/cancel and the targeting-cancel — so they sit adjacent
 * to the anchored prompt overlay (issue #298). When nothing is offered it communicates
 * "waiting" quietly rather than showing a placeholder empty bar.
 *
 * Keeps `data-testid="action-bar"` for continuity with the interaction suite.
 */
import type { ValidAction } from '../protocol';
import { cx } from '../chrome/cx';
import { DeadlineCountdown } from './DeadlineCountdown';
import s from './chrome.module.css';

/**
 * Whether an offered global action is the tray's **primary** affordance — the one
 * the player presses most (pass/resolve priority). Primary is a *treatment*
 * (weight + the `P` keybind hint), never a client-invented action: the button
 * exists only because the server offered it (ADR 0004 unchanged).
 */
function isPrimary(action: ValidAction): boolean {
  return action.type === 'pass_priority';
}

/**
 * The multi-select toolbar's controls (issues #143/#157). While building a
 * declaration the tray is dedicated to it: advancing between slots, confirming the
 * whole selection, and cancelling. An `option` decision's named choices render in
 * the anchored prompt overlay (the modal picker, #157), not here. Enablement is
 * computed by the caller from the session — the tray renders it and never derives
 * legality.
 */
export interface MultiSelectControls {
  /** Whether a "Next" advance to the following slot is offered. */
  canAdvance: boolean;
  /** Advance to the next walked slot. */
  onAdvance: () => void;
  /**
   * The confirm control, present when the action poses no option prompt. Disabled
   * until every slot's constraint is met (e.g. exactly N bottomed, or a full order).
   */
  confirm?: { label: string; enabled: boolean; onConfirm: () => void };
  /** Abandon the in-progress selection, restoring the neutral state. */
  onCancel: () => void;
}

interface Props {
  /** Subject-less actions offered right now. */
  globalActions: ValidAction[];
  /** The selected entity's actions, echoed for confirmation/accessibility. */
  selectedActions: ValidAction[];
  /** Name of the selected entity, shown as the echo's heading. */
  selectedName?: string;
  /** Echo back the chosen action (the store reads its id + content-binding token). */
  onChoose: (action: ValidAction) => void;
  /** Present in targeting mode: abandon the in-progress target selection. */
  onCancelTargeting?: () => void;
  /** Present in multi-select mode: the confirm/advance/option/cancel controls. */
  multiSelect?: MultiSelectControls;
  /**
   * True when the server is offering the receiver nothing to do (no `valid_actions`).
   * The tray then reads a quiet "Waiting" instead of an empty bar with placeholder
   * text (issue #298).
   */
  waiting?: boolean;
  /**
   * Seconds remaining on the server clock for the bare priority window, if any. Shown
   * only in the neutral state; during an active decision the countdown rides the
   * anchored prompt overlay instead (issue #298/#263).
   */
  deadline?: number;
}

export function ActionTray({
  globalActions,
  selectedActions,
  selectedName,
  onChoose,
  onCancelTargeting,
  multiSelect,
  waiting,
  deadline,
}: Props) {
  // During a multi-select the tray drives the declaration: step through slots,
  // confirm the whole selection, and cancel. An option decision's choices render in
  // the anchored overlay (the modal picker). No other global action is offered until
  // the selection is confirmed or abandoned.
  if (multiSelect) {
    return (
      <div role="toolbar" aria-label="Actions" data-testid="action-bar" className={s.tray}>
        {multiSelect.canAdvance && (
          <button type="button" onClick={multiSelect.onAdvance} className={s.button}>
            Next
          </button>
        )}
        {multiSelect.confirm && (
          <button
            type="button"
            onClick={multiSelect.confirm.onConfirm}
            disabled={!multiSelect.confirm.enabled}
            data-testid="multiselect-confirm"
            className={s.button}
          >
            {multiSelect.confirm.label}
          </button>
        )}
        <button
          type="button"
          onClick={multiSelect.onCancel}
          data-testid="multiselect-cancel"
          className={s.button}
        >
          Cancel
        </button>
      </div>
    );
  }

  // During targeting the tray is dedicated to cancelling the in-progress pick;
  // no other global action is offered until the target is chosen or abandoned.
  if (onCancelTargeting) {
    return (
      <div role="toolbar" aria-label="Actions" data-testid="action-bar" className={s.tray}>
        <button type="button" onClick={onCancelTargeting} className={s.button}>
          Cancel targeting
        </button>
      </div>
    );
  }

  const hasEcho = selectedActions.length > 0 && selectedName !== undefined;

  return (
    <div role="toolbar" aria-label="Actions" data-testid="action-bar" className={s.tray}>
      {globalActions.map((action) => (
        <button
          key={action.id}
          type="button"
          onClick={() => onChoose(action)}
          className={cx(s.button, isPrimary(action) && s.buttonPrimary)}
        >
          {action.label}
          {/* The pass shortcut hint rides its button (issue #266 binding). */}
          {isPrimary(action) && (
            <kbd className={s.keyHint} aria-hidden="true">
              P
            </kbd>
          )}
        </button>
      ))}

      {hasEcho && (
        <div data-testid="selection-echo" className={s.echo}>
          <span className={s.echoLabel}>{selectedName}</span>
          {selectedActions.map((action) => (
            <button
              key={action.id}
              type="button"
              onClick={() => onChoose(action)}
              className={s.button}
            >
              {action.label}
            </button>
          ))}
        </div>
      )}

      {/* Bare priority window: show the server clock quietly next to the offered
          actions; an active decision carries its own countdown on the overlay. */}
      {!waiting && deadline !== undefined && (
        <span className={s.trayClock}>
          Priority
          <DeadlineCountdown seconds={deadline} />
        </span>
      )}

      {/* Nothing to do: read "Waiting" quietly rather than an empty placeholder bar. */}
      {waiting && (
        <span className={s.trayWaiting} data-testid="tray-waiting">
          Waiting…
        </span>
      )}
    </div>
  );
}
