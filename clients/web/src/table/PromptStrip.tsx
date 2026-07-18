/**
 * The prompt strip (ADR 0023; blueprint §Interaction model): the fixed home that
 * states the pending question **in words**, riding the top edge of the hand
 * panel. Together with the action dock it carries every decision's question,
 * progress, and controls — replacing the anchored prompt overlay that floated
 * over the board (which could collide with controls; in the fixed shell it
 * cannot exist).
 *
 * Every string derives from the current view + the in-progress selection the
 * table passes down; nothing here is load-bearing across messages.
 */
import type { GameView, Phase } from '../protocol';
import type { PendingPrompt } from '../store';
import { DeadlineCountdown } from './DeadlineCountdown';
import { playerName } from '../playerNames';
import s from './chrome.module.css';

/** The active targeting step, for the "Choose target" strip (ADR 0009 §Client). */
export interface TargetingBanner {
  /** The action being targeted, e.g. `"Cast Lightning Bolt"`. */
  label: string;
  /** The active slot's human-readable spec, e.g. `"target creature"`. */
  prompt: string;
  /** 1-based index of the slot being filled. */
  step: number;
  /** Total number of target slots this action requires. */
  total: number;
}

/**
 * The active multi-select step (issue #143): the action's label, the active slot's
 * server prompt, how many candidates are chosen so far, and — for a fixed-count
 * slot (bottoming/discard) — how many are required. Drives the "Select" strip.
 */
export interface MultiSelectBanner {
  /** The declaration being built, e.g. `"Declare attackers"`. */
  label: string;
  /** The active slot's human-readable prompt, e.g. `"Choose attackers"`. */
  prompt: string;
  /** 1-based index of the walked slot being filled. */
  step: number;
  /** Total number of walked slots this action poses. */
  total: number;
  /** How many candidates are currently chosen in the active slot. */
  chosen: number;
  /** The exact count required for a fixed-count slot; absent for a free subset. */
  required?: number;
  /**
   * The active walked slot's kind, so the strip shows a running count only for a
   * selection (`subset`/`count`) and not for an `order` slot (where every item is
   * always included) or a `defender` slot (a single per-attacker target pick, whose
   * prompt already names the attacker — issue #347). Absent when there is no walked
   * slot (a pure option decision).
   */
  slotKind?: 'subset' | 'count' | 'order' | 'defender';
}

interface Props {
  view: GameView;
  prompt: PendingPrompt | null;
  /** Present only while picking targets; drives the targeting-mode strip. */
  targeting?: TargetingBanner | null;
  /** Present only while building a multi-select; drives the "Select" strip. */
  multiSelect?: MultiSelectBanner | null;
}

/** Display-format a phase id, e.g. `precombat_main` → `Precombat Main`. */
function formatPhase(phase: Phase): string {
  return phase
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

export function PromptStrip({ view, prompt, targeting, multiSelect }: Props) {
  // Multi-select mode owns the strip: the declaration, the active slot's server
  // prompt, the running count (for a selection slot), and the step counter.
  if (multiSelect) {
    const showCount = multiSelect.slotKind === 'count' || multiSelect.slotKind === 'subset';
    const count =
      multiSelect.required !== undefined
        ? `${multiSelect.chosen} of ${multiSelect.required} selected`
        : `${multiSelect.chosen} selected`;
    return (
      <div data-testid="prompt-banner" role="status" className={s.promptStrip}>
        <span className={s.promptTag}>Select</span>
        <span className={s.promptText} data-testid="multiselect-prompt">
          {multiSelect.prompt}
        </span>
        <span className={s.promptMeta}>{multiSelect.label}</span>
        {showCount && (
          <span className={s.promptMeta} data-testid="multiselect-count">
            {count}
          </span>
        )}
        {multiSelect.total > 1 && (
          <span className={s.promptMeta} data-testid="multiselect-step">
            Step {multiSelect.step} of {multiSelect.total}
          </span>
        )}
        {/* The server deadline countdown rides the staged decision (issue #263). */}
        {prompt?.deadline !== undefined && <DeadlineCountdown seconds={prompt.deadline} />}
      </div>
    );
  }

  // Targeting mode owns the strip: the decision kind, the server's slot prompt,
  // and a multi-target counter when relevant.
  if (targeting) {
    return (
      <div data-testid="prompt-banner" role="status" className={s.promptStrip}>
        <span className={s.promptTagTargeting}>Target</span>
        <span className={s.promptText} data-testid="targeting-prompt">
          Choose target: {targeting.prompt}
        </span>
        <span className={s.promptMeta}>{targeting.label}</span>
        {targeting.total > 1 && (
          <span className={s.promptMeta} data-testid="targeting-counter">
            Target {targeting.step} of {targeting.total}
          </span>
        )}
        {prompt?.deadline !== undefined && <DeadlineCountdown seconds={prompt.deadline} />}
      </div>
    );
  }

  // Neutral: the current step and whether the receiver may act. The top of the
  // stack is named when a response window is open, so "respond or pass" reads as
  // a sentence rather than a mystery.
  const stackTop = view.stack.length > 0 ? view.stack[view.stack.length - 1] : undefined;
  return (
    <div data-testid="prompt-banner" role="status" className={s.promptStrip}>
      <span className={s.promptTag}>{formatPhase(view.phase)}</span>
      {prompt ? (
        <span className={s.promptText}>
          {stackTop ? (
            <>
              <b>{stackTop.description}</b> is on the stack — respond, or pass to let it resolve.
            </>
          ) : (
            <>You have priority — select a card, or pass.</>
          )}
          {prompt.deadline !== undefined && <DeadlineCountdown seconds={prompt.deadline} />}
        </span>
      ) : (
        <span className={s.promptTextMuted}>
          Waiting
          {view.priority_player !== undefined && view.priority_player !== view.you
            ? ` for ${playerName(view, view.priority_player)}`
            : ''}
          …
        </span>
      )}
    </div>
  );
}
