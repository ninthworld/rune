/**
 * The last-match ribbon (issue #506; `front-door-and-lobby.md` §5.5, beat two).
 *
 * After a finished game the verdict stays in the match — it belongs to the
 * terminal `GameView`, so it survives a reconnect for free — and the *landing*
 * is the Lobby place the player already understands, plus this quiet,
 * explicitly-ephemeral ribbon above the directory.
 *
 * - **One line** — the outcome word in its §2 hue family (Victory gold, Defeat
 *   in the red loss-moment family, Draw neutral) plus the opponents and the
 *   setup label. The **word** carries the meaning, so the ribbon is never a
 *   color-only signal (§5.9).
 * - **One action** — *Play again*, which opens the Start-a-game card pre-filled
 *   with the finished room's `game_setup` and seat count and moves focus there.
 *   It is honestly a **new room**: the finished room has been reclaimed
 *   server-side (`lobby/registry.rs`), and nothing here pretends otherwise. A
 *   true rematch is a protocol change and a follow-up (§9.3).
 * - **One dismissal** — Play again, joining or creating any room, a reload, or
 *   the explicit Dismiss.
 *
 * The record is presentation-only ephemeral store state in the `lobbyError`
 * idiom: **the lobby renders identically and stays fully functional with it
 * absent**, no control's availability depends on it, and a reload loses the
 * ribbon and nothing else. #506 ships the rendering; #452/#509 produce the data.
 */
import { cx } from '../chrome/cx';
import type { LastMatchSummary } from '../store';
import { ControlButton } from '../table/controls';
import { setupLabel } from './gameSetups';
import p from './styles';

/** The outcome word — the meaning channel the hue family only tints. */
const OUTCOME_WORD: Record<LastMatchSummary['outcome'], string> = {
  victory: 'Victory',
  defeat: 'Defeat',
  draw: 'Draw',
};

export function LastMatchRibbon({
  summary,
  onPlayAgain,
  onDismiss,
}: {
  summary: LastMatchSummary;
  /** Open the Start-a-game card pre-filled with the finished configuration. */
  onPlayAgain: () => void;
  /** Drop the ribbon (it is ephemeral; nothing else changes). */
  onDismiss: () => void;
}) {
  const outcomeClass =
    summary.outcome === 'victory'
      ? p.outcomeVictory
      : summary.outcome === 'defeat'
        ? p.outcomeDefeat
        : p.outcomeDraw;

  const against =
    summary.opponents.length > 0 ? `against ${summary.opponents.join(', ')}` : undefined;
  const setup = summary.gameSetup !== undefined ? setupLabel(summary.gameSetup) : undefined;

  return (
    <section className={p.ribbon} aria-label="Your last match" data-testid="last-match-ribbon">
      <span
        className={cx(p.ribbonOutcome, outcomeClass)}
        data-testid="last-match-outcome"
        data-outcome={summary.outcome}
      >
        {OUTCOME_WORD[summary.outcome]}
      </span>
      <span className={p.muted}>
        {[against, setup].filter((part) => part !== undefined).join(' · ')}
      </span>
      <span className={p.fit}>
        <ControlButton
          variant="utility"
          label="Play again"
          onPress={onPlayAgain}
          testId="last-match-play-again"
        />
      </span>
      <span className={p.fit}>
        <ControlButton
          variant="utility"
          label="Dismiss"
          accessibleName="Dismiss the last-match ribbon"
          onPress={onDismiss}
          testId="last-match-dismiss"
        />
      </span>
    </section>
  );
}
