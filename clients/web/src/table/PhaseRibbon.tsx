/**
 * The phase/turn ribbon (React DOM, ADR 0003 — a persistent HUD a user reads).
 *
 * A always-visible indicator of where the game is in the turn: the turn number,
 * whose turn it is (the active player, highlighted — and named "Your turn" when
 * that is the receiver), and the full phase/step sequence with the current step
 * marked. Every value is rendered straight from the {@link GameView}; the client
 * counts nothing itself (issue #267). This is the foundation the stops/automation
 * UI (#264) will later attach to.
 *
 * The ribbon also reflects the table's **mode** (issue #267): `overview` for
 * scanning the whole table, `focus` when a decision is pending. The mode is derived
 * by {@link Table} purely from the current view + pending prompt (never history),
 * so a fresh mount lands in the right treatment; here it only drives presentation.
 */
import type { GameView, Phase, PlayerId } from '../protocol';
import { PHASES } from '../protocol';
import { cx } from '../chrome/cx';
import { playerName } from '../playerNames';
import s from './chrome.module.css';

/** The table's presentation mode (issue #267). */
export type TableMode = 'overview' | 'focus';

interface Props {
  view: GameView;
  /** The derived presentation mode; drives the ribbon's focus emphasis only. */
  mode: TableMode;
  /** The receiver's own seat id, to phrase the active player as "Your turn". */
  localId?: PlayerId;
}

/** Compact step labels for the ribbon strip (the full name rides in `title`/aria). */
const STEP_LABEL: Record<Phase, string> = {
  untap: 'Untap',
  upkeep: 'Upkeep',
  draw: 'Draw',
  precombat_main: 'Main 1',
  begin_combat: 'Combat',
  declare_attackers: 'Attack',
  declare_blockers: 'Block',
  combat_damage: 'Damage',
  end_combat: 'End Cbt',
  postcombat_main: 'Main 2',
  end: 'End',
  cleanup: 'Cleanup',
};

/** Full display name of a phase, e.g. `precombat_main` → `Precombat Main`. */
function fullPhase(phase: Phase): string {
  return phase
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

export function PhaseRibbon({ view, mode, localId }: Props) {
  const isLocalTurn = view.active_player !== '' && view.active_player === localId;
  const activeLabel =
    view.active_player === ''
      ? 'Active player —'
      : isLocalTurn
        ? 'Your turn'
        : `${playerName(view, view.active_player)}'s turn`;

  return (
    <div
      data-testid="phase-ribbon"
      data-mode={mode}
      className={s.ribbon}
      role="status"
      aria-label="Turn"
    >
      <span className={s.ribbonTurn} data-testid="ribbon-turn">
        Turn {view.turn > 0 ? view.turn : '—'}
      </span>
      <span className={s.ribbonActive} data-testid="ribbon-active">
        {activeLabel}
      </span>
      <ol className={s.ribbonSteps} data-testid="ribbon-steps">
        {PHASES.map((phase) => {
          const current = phase === view.phase;
          return (
            <li
              key={phase}
              data-testid={`ribbon-step-${phase}`}
              data-current={current || undefined}
              aria-current={current ? 'step' : undefined}
              title={fullPhase(phase)}
              className={current ? cx(s.ribbonStep, s.ribbonStepCurrent) : s.ribbonStep}
            >
              {STEP_LABEL[phase]}
            </li>
          );
        })}
      </ol>
      {mode === 'focus' && (
        <span className={s.ribbonFocusBadge} data-testid="ribbon-focus">
          Decision
        </span>
      )}
    </div>
  );
}
