/**
 * The prompt/phase banner (React DOM, ADR 0003).
 *
 * Shows the current step, who holds priority, and — when the server has issued
 * `valid_actions` — that a decision is pending (with the countdown the server
 * provides, displayed verbatim). When nothing is offered, input is gated: the
 * banner reads "Waiting", entities carry no hotspots, and the bar is empty.
 */
import { useEffect, useState } from 'react';
import type { GameView, Phase } from '../protocol';
import type { PendingPrompt } from '../store';
import {
  banner,
  bannerAccent,
  bannerOptions,
  bannerTargeting,
  deadlineCountdown,
  deadlineCountdownLow,
  optionButton,
} from './styles';

/** Below this many seconds the countdown enters its low-time warning state. */
const LOW_TIME_SECONDS = 10;

/**
 * A live decision countdown (issue #263). Seeds from the server-sent seconds
 * remaining ({@link GameView.action_deadline}) and ticks down locally once per
 * second; a fresh view re-seeds it (the server re-sends the real remaining time,
 * so nothing here is load-bearing across messages — a reconnect shows the right
 * value). Enters a warning state under {@link LOW_TIME_SECONDS}. The server, not
 * the client, enforces the deadline — this is display only.
 */
function DeadlineCountdown({ seconds }: { seconds: number }) {
  const [remaining, setRemaining] = useState(seconds);
  useEffect(() => {
    setRemaining(seconds);
    const id = setInterval(() => setRemaining((value) => Math.max(0, value - 1)), 1000);
    return () => clearInterval(id);
  }, [seconds]);
  const display = Math.max(0, Math.ceil(remaining));
  const low = display <= LOW_TIME_SECONDS;
  return (
    <span
      data-testid="deadline-countdown"
      data-low={low || undefined}
      style={low ? deadlineCountdownLow : deadlineCountdown}
    >
      {' '}
      — {display}s
    </span>
  );
}

/** One named choice offered in the banner's modal option picker (issue #157). */
export interface PromptOptionControl {
  /** Opaque option id echoed as the decision slot's chosen value. */
  id: string;
  /** Human-readable label for the button. */
  label: string;
}

/** The active targeting step, for the "Choose target" banner (ADR 0009 §Client). */
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
 * slot (bottoming/discard) — how many are required. Drives the "Select" banner.
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
   * The active walked slot's kind, so the banner shows a running count only for a
   * selection (`subset`/`count`) and not for an `order` slot (where every item is
   * always included). Absent when there is no walked slot (a pure option decision).
   */
  slotKind?: 'subset' | 'count' | 'order';
  /**
   * A modal option picker (issue #157): the named choices rendered as buttons in
   * the banner itself, so a keep/mulligan-style decision reads as a modal choice
   * rather than action-bar chrome. Empty/absent when the action poses no `option`.
   */
  options?: PromptOptionControl[];
  /** The option decision's own prompt, shown above the option buttons. */
  optionPrompt?: string;
  /** Whether the option buttons may submit now (false while a count slot is partial). */
  optionsEnabled?: boolean;
}

interface Props {
  view: GameView;
  prompt: PendingPrompt | null;
  /** Present only while picking targets; drives the targeting-mode banner. */
  targeting?: TargetingBanner | null;
  /** Present only while building a multi-select; drives the "Select" banner. */
  multiSelect?: MultiSelectBanner | null;
  /** Submit an option decision by its chosen id (the modal option picker). */
  onOption?: (optionId: string) => void;
}

/** Display-format a phase id, e.g. `precombat_main` → `Precombat Main`. */
function formatPhase(phase: Phase): string {
  return phase
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

export function PromptBanner({ view, prompt, targeting, multiSelect, onOption }: Props) {
  // Multi-select mode owns the banner: it announces the declaration, the server's
  // slot prompt, the running count of chosen candidates (for a selection slot, with
  // the required count when fixed), a per-slot step counter when there are several,
  // and — for an `option` decision — the named choices as a modal picker.
  if (multiSelect) {
    // A count/subset slot shows a running selection count; an `order` slot (every
    // item always included) and a pure option decision (no walked slot) show none.
    const showCount = multiSelect.slotKind === 'count' || multiSelect.slotKind === 'subset';
    const count =
      multiSelect.required !== undefined
        ? `${multiSelect.chosen} of ${multiSelect.required} selected`
        : `${multiSelect.chosen} selected`;
    const options = multiSelect.options ?? [];
    return (
      <div data-testid="prompt-banner" role="status" style={banner}>
        <span style={bannerTargeting} data-testid="multiselect-prompt">
          Select: {multiSelect.prompt}
        </span>
        <span>{multiSelect.label}</span>
        {showCount && <span data-testid="multiselect-count">{count}</span>}
        {multiSelect.total > 1 && (
          <span data-testid="multiselect-step">
            Step {multiSelect.step} of {multiSelect.total}
          </span>
        )}
        {options.length > 0 && (
          <div style={bannerOptions} data-testid="multiselect-options">
            {multiSelect.optionPrompt !== undefined && <span>{multiSelect.optionPrompt}</span>}
            {options.map((option) => (
              <button
                key={option.id}
                type="button"
                onClick={() => onOption?.(option.id)}
                disabled={multiSelect.optionsEnabled === false}
                data-testid={`multiselect-option-${option.id}`}
                style={optionButton}
              >
                {option.label}
              </button>
            ))}
          </div>
        )}
      </div>
    );
  }

  // Targeting mode owns the banner: it announces the decision kind ("Choose
  // target"), the server's slot prompt, and a multi-target counter when relevant.
  if (targeting) {
    return (
      <div data-testid="prompt-banner" role="status" style={banner}>
        <span style={bannerTargeting} data-testid="targeting-prompt">
          Choose target: {targeting.prompt}
        </span>
        <span>{targeting.label}</span>
        {targeting.total > 1 && (
          <span data-testid="targeting-counter">
            Target {targeting.step} of {targeting.total}
          </span>
        )}
      </div>
    );
  }

  return (
    <div data-testid="prompt-banner" role="status" style={banner}>
      <span>
        Phase: <span style={bannerAccent}>{formatPhase(view.phase)}</span>
      </span>
      {view.priority_player !== undefined && <span>Priority: {view.priority_player}</span>}
      {prompt ? (
        <span style={bannerAccent}>
          Your move
          {prompt.deadline !== undefined && <DeadlineCountdown seconds={prompt.deadline} />}
        </span>
      ) : (
        <span>Waiting…</span>
      )}
    </div>
  );
}
