/**
 * The React DOM action bar (ADR 0004).
 *
 * The bar is O(1) in board size: it holds only global, subject-less actions
 * (pass, end turn, confirm) plus a contextual echo of the currently selected
 * entity's actions. Per-card actions are NEVER enumerated here — they render on
 * the entity (see `EntityOverlay`). Every button corresponds to an entry in
 * `valid_actions[]`; the bar computes no legality.
 */
import type { ValidAction } from '../protocol';
import s from './chrome.module.css';

/**
 * The multi-select toolbar's controls (issues #143/#157). While building a
 * declaration the bar is dedicated to it: advancing between slots, confirming the
 * whole selection, and cancelling. An `option` decision's named choices render in
 * the prompt banner (the modal picker, #157), not here. Enablement is computed by
 * the caller from the session — the bar renders it and never derives legality.
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
}

export function ActionBar({
  globalActions,
  selectedActions,
  selectedName,
  onChoose,
  onCancelTargeting,
  multiSelect,
}: Props) {
  // During a multi-select the bar drives the declaration: step through slots,
  // confirm the whole selection, and cancel. An option decision's choices render in
  // the banner (the modal picker). No other global action is offered until the
  // selection is confirmed or abandoned.
  if (multiSelect) {
    return (
      <div role="toolbar" aria-label="Actions" data-testid="action-bar" className={s.bar}>
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

  // During targeting the bar is dedicated to cancelling the in-progress pick;
  // no other global action is offered until the target is chosen or abandoned.
  if (onCancelTargeting) {
    return (
      <div role="toolbar" aria-label="Actions" data-testid="action-bar" className={s.bar}>
        <button type="button" onClick={onCancelTargeting} className={s.button}>
          Cancel targeting
        </button>
      </div>
    );
  }

  const hasEcho = selectedActions.length > 0 && selectedName !== undefined;
  const empty = globalActions.length === 0 && !hasEcho;

  return (
    <div role="toolbar" aria-label="Actions" data-testid="action-bar" className={s.bar}>
      {globalActions.map((action) => (
        <button key={action.id} type="button" onClick={() => onChoose(action)} className={s.button}>
          {action.label}
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

      {empty && <span className={s.muted}>No actions available</span>}
    </div>
  );
}
