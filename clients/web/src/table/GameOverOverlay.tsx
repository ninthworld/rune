/**
 * The game-over overlay (React DOM, ADR 0003 — text a user reads is DOM, not
 * canvas).
 *
 * A terminal {@link GameView} carries a {@link GameResult}; this renders it as a
 * modal over the final board, naming the verdict from *your* seat (win / loss /
 * draw), who won, and why the game ended. It is pure render output of the latest
 * view's `result` + `you`: no client state is load-bearing across messages, so a
 * refresh + reconnect that replays the terminal view shows the exact same screen.
 * The client never decides a winner or terminality — it only formats the server's
 * already-decided result (zero game logic, AGENTS.md hard rule).
 *
 * It also carries the way out (issue #452). Every terminal state reaches this
 * overlay — win, loss, draw, a conceded game, and a reconnect into a finished one —
 * and until it offered an exit there was none anywhere on the in-game path, so the
 * verdict was a dead screen. Leaving is a client-session action (like disconnect),
 * never a game action: no `valid_actions` entry is invented for it.
 */
import type { GameResult, GameOverReason, PlayerId } from '../protocol';
import { cx } from '../chrome/cx';
import s from './chrome.module.css';

interface Props {
  /** The server-decided terminal result. */
  result: GameResult;
  /** The receiver's own seat id (`GameView.you`), used to phrase the verdict. */
  you: PlayerId;
  /**
   * Public display names keyed by player id (`GameView.player_names`, issue #294),
   * used to name the winner. A player with no entry falls back to their raw id.
   */
  names: Record<PlayerId, string>;
  /**
   * Leave the finished game and return to the lobby (issue #452). Absent only where
   * there is no session to leave (a preview/embedded render), which then shows the
   * verdict alone.
   */
  onLeave?: () => void;
}

/** The three outcomes the overlay phrases, from the receiving player's seat. */
type Outcome = 'win' | 'loss' | 'draw';

/**
 * Classify the result from the receiver's seat. A draw has no winner (CR 104.4a);
 * otherwise the receiver won iff they are the named winner. A non-winner who is
 * not among the losers (a spectator) is phrased as a loss-of-focus "loss" only
 * when they actually lost — otherwise the winner is simply named (see below).
 */
function outcomeFor(result: GameResult, you: PlayerId): Outcome {
  if (result.winner === undefined) return 'draw';
  return result.winner === you ? 'win' : 'loss';
}

/** The headline verdict text for an outcome. */
function headlineText(outcome: Outcome): string {
  switch (outcome) {
    case 'win':
      return 'Victory';
    case 'loss':
      return 'Defeat';
    case 'draw':
      return 'Draw';
  }
}

/** A human sentence naming who won (or that the game was drawn). */
function winnerText(result: GameResult, names: Record<PlayerId, string>): string {
  if (result.winner === undefined) return 'The game is a draw.';
  const winner = names[result.winner] ?? result.winner;
  return `${winner} wins the game.`;
}

/**
 * A human sentence for why the game ended. An unrecognized future reason (the
 * server's enum grew) is handled generically rather than crashing — the overlay
 * still shows game over.
 */
function reasonText(reason: GameOverReason): string {
  switch (reason) {
    case 'life_zero':
      return 'A player’s life total reached zero.';
    case 'decked':
      return 'A player drew from an empty library.';
    case 'concede':
      return 'A player conceded.';
    case 'commander_damage':
      return 'A player took 21 combat damage from a single commander.';
    default:
      return 'The game has ended.';
  }
}

export function GameOverOverlay({ result, you, names, onLeave }: Props) {
  const outcome = outcomeFor(result, you);
  const headlineTint =
    outcome === 'win' ? s.gameOverWin : outcome === 'loss' ? s.gameOverLoss : s.gameOverNeutral;

  return (
    <div className={s.gameOverBackdrop} data-testid="game-over-overlay">
      <div
        role="alertdialog"
        aria-label="Game over"
        aria-describedby="game-over-winner"
        className={s.gameOverPanel}
      >
        <h2 className={cx(s.gameOverHeadline, headlineTint)} data-testid="game-over-headline">
          {headlineText(outcome)}
        </h2>
        <p id="game-over-winner" className={s.gameOverWinner} data-testid="game-over-winner">
          {winnerText(result, names)}
        </p>
        <p className={s.gameOverReason} data-testid="game-over-reason">
          {reasonText(result.reason)}
        </p>
        {onLeave && (
          <button
            type="button"
            className={s.gameOverExit}
            data-testid="game-over-leave"
            onClick={onLeave}
            autoFocus
          >
            Return to lobby
          </button>
        )}
      </div>
    </div>
  );
}
