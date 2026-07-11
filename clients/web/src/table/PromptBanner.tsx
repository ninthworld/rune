/**
 * The prompt/phase banner (React DOM, ADR 0003).
 *
 * Shows the current step, who holds priority, and — when the server has issued
 * `valid_actions` — that a decision is pending (with the countdown the server
 * provides, displayed verbatim). When nothing is offered, input is gated: the
 * banner reads "Waiting", entities carry no hotspots, and the bar is empty.
 */
import type { GameView, Phase } from '../protocol';
import type { PendingPrompt } from '../store';
import { banner, bannerAccent, bannerTargeting } from './styles';

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
}

interface Props {
  view: GameView;
  prompt: PendingPrompt | null;
  /** Present only while picking targets; drives the targeting-mode banner. */
  targeting?: TargetingBanner | null;
  /** Present only while building a multi-select; drives the "Select" banner. */
  multiSelect?: MultiSelectBanner | null;
}

/** Display-format a phase id, e.g. `precombat_main` → `Precombat Main`. */
function formatPhase(phase: Phase): string {
  return phase
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

export function PromptBanner({ view, prompt, targeting, multiSelect }: Props) {
  // Multi-select mode owns the banner: it announces the declaration, the server's
  // slot prompt, the running count of chosen candidates (with the required count
  // for a fixed-count slot), and a per-slot step counter when there are several.
  if (multiSelect) {
    const count =
      multiSelect.required !== undefined
        ? `${multiSelect.chosen} of ${multiSelect.required} selected`
        : `${multiSelect.chosen} selected`;
    return (
      <div data-testid="prompt-banner" role="status" style={banner}>
        <span style={bannerTargeting} data-testid="multiselect-prompt">
          Select: {multiSelect.prompt}
        </span>
        <span>{multiSelect.label}</span>
        <span data-testid="multiselect-count">{count}</span>
        {multiSelect.total > 1 && (
          <span data-testid="multiselect-step">
            Step {multiSelect.step} of {multiSelect.total}
          </span>
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
          {prompt.deadline !== undefined ? ` — ${prompt.deadline}s` : ''}
        </span>
      ) : (
        <span>Waiting…</span>
      )}
    </div>
  );
}
