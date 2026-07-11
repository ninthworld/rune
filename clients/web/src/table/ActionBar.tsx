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
import { bar, button, echo, echoLabel, muted } from './styles';

/** One option submit trigger in a multi-select (mulligan keep/take-another). */
export interface MultiSelectOptionControl {
  /** Opaque option id echoed as the decision slot's chosen value. */
  id: string;
  /** Human-readable label for the button. */
  label: string;
}

/**
 * The multi-select toolbar's controls (issue #143). While building a declaration
 * the bar is dedicated to it: advancing between slots, confirming the whole
 * selection (or choosing an option), and cancelling. Enablement is computed by the
 * caller from the session — the bar renders it and never derives legality.
 */
export interface MultiSelectControls {
  /** Whether a "Next" advance to the following slot is offered. */
  canAdvance: boolean;
  /** Advance to the next walked slot. */
  onAdvance: () => void;
  /**
   * The confirm control, present when the action poses no option prompt. Disabled
   * until every slot's constraint is met (e.g. exactly N bottomed).
   */
  confirm?: { label: string; enabled: boolean; onConfirm: () => void };
  /**
   * Option submit triggers (mulligan keep/take-another). Each submits its decision
   * plus the current selection; `enabled` is false while a count slot is partial.
   */
  options?: MultiSelectOptionControl[];
  /** Whether the option buttons may submit right now (no partial count slot). */
  optionsEnabled?: boolean;
  /** Submit the action with the chosen option id (and the current selection). */
  onOption?: (optionId: string) => void;
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
  // confirm the whole selection (or pick an option), and cancel. No other global
  // action is offered until the selection is confirmed or abandoned.
  if (multiSelect) {
    return (
      <div role="toolbar" aria-label="Actions" data-testid="action-bar" style={bar}>
        {multiSelect.canAdvance && (
          <button type="button" onClick={multiSelect.onAdvance} style={button}>
            Next
          </button>
        )}
        {multiSelect.confirm && (
          <button
            type="button"
            onClick={multiSelect.confirm.onConfirm}
            disabled={!multiSelect.confirm.enabled}
            data-testid="multiselect-confirm"
            style={button}
          >
            {multiSelect.confirm.label}
          </button>
        )}
        {(multiSelect.options ?? []).map((option) => (
          <button
            key={option.id}
            type="button"
            onClick={() => multiSelect.onOption?.(option.id)}
            disabled={multiSelect.optionsEnabled === false}
            data-testid={`multiselect-option-${option.id}`}
            style={button}
          >
            {option.label}
          </button>
        ))}
        <button
          type="button"
          onClick={multiSelect.onCancel}
          data-testid="multiselect-cancel"
          style={button}
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
      <div role="toolbar" aria-label="Actions" data-testid="action-bar" style={bar}>
        <button type="button" onClick={onCancelTargeting} style={button}>
          Cancel targeting
        </button>
      </div>
    );
  }

  const hasEcho = selectedActions.length > 0 && selectedName !== undefined;
  const empty = globalActions.length === 0 && !hasEcho;

  return (
    <div role="toolbar" aria-label="Actions" data-testid="action-bar" style={bar}>
      {globalActions.map((action) => (
        <button key={action.id} type="button" onClick={() => onChoose(action)} style={button}>
          {action.label}
        </button>
      ))}

      {hasEcho && (
        <div data-testid="selection-echo" style={echo}>
          <span style={echoLabel}>{selectedName}</span>
          {selectedActions.map((action) => (
            <button key={action.id} type="button" onClick={() => onChoose(action)} style={button}>
              {action.label}
            </button>
          ))}
        </div>
      )}

      {empty && <span style={muted}>No actions available</span>}
    </div>
  );
}
