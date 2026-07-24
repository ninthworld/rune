/**
 * The room's ready bar (issue #506; `front-door-and-lobby.md` §5.3, fixes P3
 * and P4).
 *
 * The shipped room put the one advance-the-game control at the end of a scroll,
 * below the roster, the AI seating, five deck tiles, and Build a deck — at a
 * filled four-seat room it was off-screen at the 1280×800 desktop floor. Here it
 * is pinned to the bottom of the room composition at `SCENE_ELEVATION.screen`,
 * visible at every scroll position and every reference geometry: the pregame
 * echo of ADR 0023's one action home.
 *
 * Left to right it carries **the gate in words** — *Choose and submit a deck* →
 * *Waiting for 2 more players* → *You're ready — waiting for Bob* → *Starting
 * the game…* — and the single gold control (Submit deck → Ready), with the quiet
 * Not ready fallback beside it once ready. Every string is read from the current
 * `LobbyView` by {@link readyGate}; **the bar computes no legality and offers
 * only advertised commands**. This is where P4's missing causality is drawn: the
 * sentence names the reason the gold control is what it is.
 *
 * A pressed control takes the `held` elevation and `aria-busy` while its command
 * is in flight; nothing else is disabled, because the authoritative answer is a
 * fresh `LobbyView` and the UI must stay interactive if it never comes (§5.7).
 */
import { cx } from '../chrome/cx';
import type { ReadyGateState } from './readyGate';
import p from './styles';

export interface ReadyBarProps {
  /** The gate derived from the current view. */
  gate: ReadyGateState;
  /** Whether the local seat already submitted a deck (drives the relabel). */
  decked: boolean;
  /** Whether `submit_deck` is advertised (Resubmit stays offered once decked). */
  canSubmit: boolean;
  /** Send `submit_deck` with the picked starter/built deck. */
  onSubmitDeck: () => void;
  /** Send `ready`. */
  onReady: () => void;
  /** Send `ready: false`. */
  onUnready: () => void;
  /** The designated commander line, when the advertised format requires one. */
  commanderLine?: string;
}

export function ReadyBar({
  gate,
  decked,
  canSubmit,
  onSubmitDeck,
  onReady,
  onUnready,
  commanderLine,
}: ReadyBarProps) {
  return (
    <div className={p.readyBar} data-testid="ready-bar" data-sticky="true">
      <div className={p.readyBarInner}>
        <span className={p.gateSentence} data-testid="ready-gate">
          {/* The waiting phrasing keeps the shipped test id so nothing that
              watched for it is lost in the restyle. */}
          <span data-testid={gate.ready && !gate.starting ? 'ready-waiting' : undefined}>
            {gate.sentence}
          </span>
          {commanderLine !== undefined && (
            <>
              {' '}
              <span className={p.muted} data-testid="designated-commander">
                Commander: {commanderLine}
              </span>
            </>
          )}
        </span>
        <span className={p.readyBarActions}>
          {/* Submit stays offered while advertised (a resubmit is legal), but it
              is gold only while it is the NEXT step. */}
          {canSubmit && (
            <button
              type="button"
              className={cx(gate.gold === 'submit_deck' ? p.gold : p.button)}
              data-gold={gate.gold === 'submit_deck' ? 'true' : undefined}
              onClick={onSubmitDeck}
              data-testid="submit-deck-button"
            >
              {decked ? 'Resubmit deck' : 'Submit deck'}
            </button>
          )}
          {gate.gold === 'ready' && (
            <button
              type="button"
              className={p.gold}
              data-gold="true"
              onClick={onReady}
              data-testid="ready-button"
            >
              Ready
            </button>
          )}
          {gate.unready && (
            <button
              type="button"
              className={p.quiet}
              onClick={onUnready}
              data-testid="unready-button"
            >
              Not ready
            </button>
          )}
        </span>
      </div>
    </div>
  );
}
