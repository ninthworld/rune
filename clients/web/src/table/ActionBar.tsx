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
}

export function ActionBar({
  globalActions,
  selectedActions,
  selectedName,
  onChoose,
  onCancelTargeting,
}: Props) {
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
