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
import { banner, bannerAccent } from './styles';

interface Props {
  view: GameView;
  prompt: PendingPrompt | null;
}

/** Display-format a phase id, e.g. `precombat_main` → `Precombat Main`. */
function formatPhase(phase: Phase): string {
  return phase
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

export function PromptBanner({ view, prompt }: Props) {
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
