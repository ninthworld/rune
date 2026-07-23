/**
 * The decision sheet (issues #157/#009): a viewport-clamped layer — one of the only
 * layers permitted to cover the shell (ADR 0023) — that hosts a multi-select answer
 * given in the DOM rather than on the canvas. Two kinds render here:
 *
 * - A **sheet-mode slot**: an `order` list (items in current order) or a non-canvas
 *   `select_from_zone` (graveyard/library candidates with the chosen ones marked),
 *   shown through {@link PromptSurface}.
 * - An **option picker**: the named choices of an option slot (e.g. a mulligan's
 *   keep / take-another), each disabled while a paired count slot is not yet
 *   submittable (e.g. bottoming incomplete).
 *
 * Pure render of the session already assembled by the caller — it submits nothing
 * itself, only routing toggles/reorders/option-picks back through the handlers.
 */
import type { GameView } from '../protocol';
import { PromptSurface } from './PromptSurface';
import { activeChosen as msActiveChosen, hasOptions, optionsSubmittable } from './multiSelect';
import type { MultiSelectSession, MultiSelectSlot } from './multiSelect';
import { cardNameOf } from './tableView';
import s from './chrome.module.css';

interface Props {
  view: GameView;
  multiSelect: MultiSelectSession | null;
  /** Whether the active slot is answered in this sheet rather than on the canvas. */
  sheetMode: boolean;
  /** The active multi-select slot, when one is being resolved. */
  msSlot: MultiSelectSlot | null;
  onToggle: (entityId: string) => void;
  onMove: (entityId: string, direction: -1 | 1) => void;
  onChooseOption: (optionId: string) => void;
}

export function DecisionSheet({
  view,
  multiSelect,
  sheetMode,
  msSlot,
  onToggle,
  onMove,
  onChooseOption,
}: Props) {
  // The option picker's named choices (issue #157), if the active session poses one.
  const optionControls = multiSelect && hasOptions(multiSelect) ? multiSelect.options[0] : null;

  if (!((sheetMode && msSlot) || optionControls)) return null;

  // The sheet rows for a sheet-mode slot: an `order` list (items in current order)
  // or a non-canvas `select_from_zone` (candidates with chosen).
  const surfaceChosen = multiSelect ? msActiveChosen(multiSelect) : [];
  const surfaceItems =
    sheetMode && msSlot
      ? (msSlot.kind === 'order' ? surfaceChosen : msSlot.candidates).map((id) => ({
          id,
          label: cardNameOf(view, id),
          chosen: surfaceChosen.includes(id),
        }))
      : [];

  return (
    <div className={s.sheetBackdrop} data-testid="decision-sheet">
      <div className={s.sheetPanel}>
        {sheetMode && msSlot && (
          <PromptSurface
            mode={msSlot.kind === 'order' ? 'order' : 'select'}
            prompt={msSlot.prompt}
            zone={msSlot.zone}
            items={surfaceItems}
            onToggle={onToggle}
            onMove={onMove}
          />
        )}
        {multiSelect && optionControls && (
          <div className={s.sheetOptions} data-testid="multiselect-options">
            {optionControls.prompt !== undefined && <span>{optionControls.prompt}</span>}
            {(optionControls.options ?? []).map((option) => (
              <button
                key={option.id}
                type="button"
                onClick={() => onChooseOption(option.id)}
                disabled={!optionsSubmittable(multiSelect)}
                data-testid={`multiselect-option-${option.id}`}
                className={s.optionButton}
              >
                {option.label}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
