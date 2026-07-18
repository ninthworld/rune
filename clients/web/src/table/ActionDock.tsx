/**
 * The action dock (ADR 0023 commitment 2: **one action home**; ADR 0004
 * preserved, reinterpreted).
 *
 * Every server-offered action renders HERE — a fixed location beside the hand —
 * and nowhere else. Selecting a card (hand or battlefield) routes its offered
 * actions to the dock; per-card popup menus are abolished (a popup under a
 * bottom-edge card is guaranteed to clip; in this anatomy it cannot exist). The
 * dock stays O(1) in board size: it holds the global, subject-less actions
 * (pass/resolve — the primary action, always the same button in the same place)
 * plus the current selection's server-labeled actions. The dock computes no
 * legality; every button corresponds to an entry in `valid_actions[]`.
 *
 * The dock also hosts the decision controls — multi-select confirm/advance/
 * cancel and the targeting cancel — so every decision's controls live in the one
 * home. When nothing is offered it reads a quiet "Waiting".
 *
 * Keeps `data-testid="action-bar"` for continuity with the interaction suite.
 */
import type { ValidAction } from '../protocol';
import { cx } from '../chrome/cx';
import { DeadlineCountdown } from './DeadlineCountdown';
import s from './chrome.module.css';

/**
 * Whether an offered global action is the dock's **primary** affordance — the one
 * the player presses most (pass/resolve priority). Primary is a *treatment*
 * (weight + the `P` keybind hint), never a client-invented action: the button
 * exists only because the server offered it (ADR 0004 unchanged).
 */
function isPrimary(action: ValidAction): boolean {
  return action.type === 'pass_priority';
}

/**
 * The multi-select decision's controls (issues #143/#157). While building a
 * declaration the dock is dedicated to it: advancing between slots, confirming the
 * whole selection, and cancelling. An `option` decision's named choices render in
 * the option sheet (#157), not here. Enablement is computed by the caller from the
 * session — the dock renders it and never derives legality.
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
  /** The selected entity's actions, routed here (the one action home). */
  selectedActions: ValidAction[];
  /** Name of the selected entity, shown as the routed actions' heading. */
  selectedName?: string;
  /** Echo back the chosen action (the store reads its id + content-binding token). */
  onChoose: (action: ValidAction) => void;
  /** Clear the current selection (the Esc affordance, as a button). */
  onClearSelection?: () => void;
  /** Present in targeting mode: abandon the in-progress target selection. */
  onCancelTargeting?: () => void;
  /** Present in multi-select mode: the confirm/advance/cancel controls. */
  multiSelect?: MultiSelectControls;
  /**
   * True when the server is offering the receiver nothing to do (no `valid_actions`).
   * The dock then reads a quiet "Waiting" instead of an empty surface.
   */
  waiting?: boolean;
  /**
   * Seconds remaining on the server clock for the bare priority window, if any.
   * During an active decision the countdown rides the prompt strip instead.
   */
  deadline?: number;
}

export function ActionDock({
  globalActions,
  selectedActions,
  selectedName,
  onChoose,
  onClearSelection,
  onCancelTargeting,
  multiSelect,
  waiting,
  deadline,
}: Props) {
  return (
    <div role="toolbar" aria-label="Actions" data-testid="action-bar" className={s.actionDock}>
      <span className={s.dockLabel} aria-hidden="true">
        Actions
      </span>

      {/* A multi-select owns the dock: step through slots, confirm the whole
          selection atomically, or cancel. No other action is offered until the
          declaration is confirmed or abandoned. */}
      {multiSelect ? (
        <>
          {multiSelect.canAdvance && (
            <button type="button" onClick={multiSelect.onAdvance} className={s.dockButton}>
              Next
            </button>
          )}
          {multiSelect.confirm && (
            <button
              type="button"
              onClick={multiSelect.confirm.onConfirm}
              disabled={!multiSelect.confirm.enabled}
              data-testid="multiselect-confirm"
              className={cx(s.dockButton, s.dockButtonPrimary)}
            >
              {multiSelect.confirm.label}
            </button>
          )}
          <button
            type="button"
            onClick={multiSelect.onCancel}
            data-testid="multiselect-cancel"
            className={cx(s.dockButton, s.dockButtonGhost)}
          >
            Cancel
          </button>
        </>
      ) : onCancelTargeting ? (
        // Targeting mode: the dock is dedicated to cancelling the in-progress pick.
        <button
          type="button"
          onClick={onCancelTargeting}
          className={cx(s.dockButton, s.dockButtonGhost)}
        >
          Cancel targeting
        </button>
      ) : (
        <>
          {/* The selected entity's actions, routed to the one action home (ADR 0023
              commitment 2). Server-labeled, O(1) — never a per-board enumeration.
              While a selection is active its actions are the player's declared
              intent, so they render FIRST and carry the primary weight — and the
              global pass demotes to the quiet treatment (below), so the brightest
              control is never one slip away from the wrong verb. */}
          {selectedActions.length > 0 && selectedName !== undefined && (
            <div data-testid="selection-echo" className={s.dockSelection}>
              <span className={s.dockSelectionName}>{selectedName}</span>
              {selectedActions.map((action) => (
                <button
                  key={action.id}
                  type="button"
                  onClick={() => onChoose(action)}
                  data-primary="true"
                  className={cx(s.dockButton, s.dockButtonPrimary)}
                >
                  {action.label}
                </button>
              ))}
              {onClearSelection && (
                <button
                  type="button"
                  onClick={onClearSelection}
                  data-testid="clear-selection"
                  className={cx(s.dockButton, s.dockButtonGhost)}
                >
                  Cancel selection
                  <kbd className={s.keyHint} aria-hidden="true">
                    Esc
                  </kbd>
                </button>
              )}
            </div>
          )}

          {globalActions.map((action) => {
            // Pass keeps its fixed slot and keybind, but yields the primary
            // treatment whenever a selection's actions are routed to the dock.
            const primary = isPrimary(action) && selectedActions.length === 0;
            return (
              <button
                key={action.id}
                type="button"
                onClick={() => onChoose(action)}
                data-primary={primary || undefined}
                className={cx(s.dockButton, primary && s.dockButtonPrimary)}
              >
                {action.label}
                {/* The pass shortcut hint rides its button (issue #266 binding). */}
                {isPrimary(action) && (
                  <kbd className={s.keyHint} aria-hidden="true">
                    P
                  </kbd>
                )}
              </button>
            );
          })}

          {/* Bare priority window: show the server clock quietly under the actions. */}
          {!waiting && deadline !== undefined && (
            <span className={s.trayClock}>
              Priority
              <DeadlineCountdown seconds={deadline} />
            </span>
          )}

          {/* Nothing to do: read "Waiting" quietly rather than an empty surface. */}
          {waiting && (
            <span className={s.trayWaiting} data-testid="tray-waiting">
              Waiting…
            </span>
          )}
        </>
      )}
    </div>
  );
}
