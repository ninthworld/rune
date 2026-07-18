/**
 * The top status bar (ADR 0023): brand · turn/phase strip · combat status ·
 * (compact: stack/log chips) · menu. One fixed home for session-level status;
 * the phase strip's expanded step list opens as a viewport-clamped overlay below
 * the bar (the only layer permitted to cover the shell).
 *
 * On the compact composition the full phase strip condenses to the turn pill +
 * phase-progress dots, and the rail's stack/log collapse to chips here that open
 * sheets (blueprint §Phone portrait). Everything renders straight from the view.
 */
import type { GameView, Phase, PlayerId, ValidAction } from '../protocol';
import { PHASES } from '../protocol';
import { GameMenu } from './GameMenu';
import { PhaseIndicator, type TableMode } from './PhaseIndicator';
import { RuneMark } from '../chrome/RuneMark';
import s from './chrome.module.css';

/** Which sheet a compact top-bar chip opens (the rail's content, as sheets). */
export type RailSheet = 'stack' | 'log';

interface Props {
  view: GameView;
  mode: TableMode;
  localId?: PlayerId;
  /** Compact composition: condensed strip + stack/log chips opening sheets. */
  compact?: boolean;
  /** Set the receiver's priority-stop preferences (issue #264); absent read-only. */
  onSetStops?: (stops: Phase[]) => void;
  /** Open a rail sheet (compact composition only). */
  onOpenSheet?: (sheet: RailSheet) => void;
  /** The concede action, when the server offers it (routed to the game menu). */
  concede?: ValidAction;
  /** Echo a chosen menu action to the store. */
  onChoose?: (action: ValidAction) => void;
  /** Open the keyboard shortcut reference. */
  onShowShortcuts?: () => void;
}

/** How many attackers currently attack the receiver, verbatim from the view. */
function attackersOnMe(view: GameView, localId: PlayerId | undefined): number {
  if (localId === undefined) return 0;
  return view.battlefield.filter((perm) => perm.attacking_player === localId).length;
}

export function TopBar({
  view,
  mode,
  localId,
  compact,
  onSetStops,
  onOpenSheet,
  concede,
  onChoose,
  onShowShortcuts,
}: Props) {
  const attacked = attackersOnMe(view, localId);
  return (
    <div className={s.topBar} data-testid="top-bar">
      <span className={s.brand} aria-hidden="true">
        <RuneMark size={18} />
        {!compact && <span className={s.brandWordmark}>RUNE</span>}
      </span>
      {compact ? (
        <CompactStatus view={view} localId={localId} />
      ) : (
        <PhaseIndicator view={view} mode={mode} localId={localId} onSetStops={onSetStops} />
      )}
      <span className={s.topBarRight}>
        {/* "You are attacked ×N" (issue #347): combat aimed at the receiver reads
            from the fixed status home. Verbatim view data; no combat derived. */}
        {attacked > 0 && (
          <span className={s.underAttack} data-testid="topbar-attacked">
            ▸ You are attacked ×{attacked}
          </span>
        )}
        {compact && onOpenSheet && (
          <>
            <button
              type="button"
              className={s.chipButton}
              data-testid="stack-chip"
              onClick={() => onOpenSheet('stack')}
            >
              Stack <span className={s.chipCount}>{view.stack.length}</span>
            </button>
            <button
              type="button"
              className={s.chipButton}
              data-testid="log-chip"
              onClick={() => onOpenSheet('log')}
            >
              Log
            </button>
          </>
        )}
        {onChoose && onShowShortcuts && (
          <GameMenu concede={concede} onChoose={onChoose} onShowShortcuts={onShowShortcuts} />
        )}
      </span>
    </div>
  );
}

/**
 * The compact composition's condensed status: a turn pill plus phase-progress
 * dots (blueprint §Phone portrait — the full step strip lives behind the pill's
 * semantics, carried by the pill text). Pure render of the single view value.
 */
function CompactStatus({ view, localId }: { view: GameView; localId?: PlayerId }) {
  const isLocalTurn = view.active_player !== '' && view.active_player === localId;
  const turnLabel = view.turn > 0 ? `T${view.turn}` : 'T—';
  const who =
    view.active_player === ''
      ? '—'
      : isLocalTurn
        ? 'Your turn'
        : `${view.player_names[view.active_player] ?? view.active_player}'s turn`;
  const current = PHASES.indexOf(view.phase);
  return (
    <>
      <span className={s.turnPill} data-testid="turn-pill">
        {turnLabel} · <b>{who}</b>
      </span>
      <span className={s.phaseDots} aria-hidden="true" data-testid="phase-dots">
        {PHASES.map((phase, index) => (
          <i
            key={phase}
            data-current={index === current || undefined}
            data-passed={index < current || undefined}
          />
        ))}
      </span>
    </>
  );
}
