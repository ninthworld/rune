/**
 * The compact turn/phase indicator (React DOM, ADR 0003 — a persistent HUD a user
 * reads). Top-center of the tabletop shell.
 *
 * Compact by default (issue #297): the turn number, whose turn it is (the active
 * player, highlighted — and named "Your turn" when that is the receiver), the
 * current step's name, and a lightweight phase-group progress treatment
 * (beginning / main / combat / main / ending). The full twelve-step sequence is
 * available on demand — expanding the indicator reveals it with the current step
 * marked, the same information the retired twelve-pill ribbon showed at all times.
 * Every value is rendered straight from the {@link GameView}; the client counts
 * nothing itself. Mapping the current phase to its display label and group is pure
 * presentation classification of the single view value, not turn/step counting.
 *
 * The expanded surface is where the per-step stop toggles (#264) will later attach,
 * so every step stays individually addressable in the expanded state (a stable
 * per-step element with a `data-phase` handle and testid). This issue implements no
 * stops.
 *
 * Expansion is ephemeral presentation state, not game state: a fresh mount from one
 * {@link GameView} renders collapsed, so nothing here is load-bearing across
 * messages (the reconnect/replay invariant).
 *
 * The indicator also reflects the table's **mode** (issue #267): `overview` for
 * scanning the whole table, `focus` when a decision is pending. The mode is derived
 * by {@link Table} purely from the current view + pending prompt (never history),
 * so a fresh mount lands in the right treatment; here it only drives presentation.
 */
import { useId, useState } from 'react';
import type { GameView, Phase, PlayerId } from '../protocol';
import { PHASES } from '../protocol';
import { cx } from '../chrome/cx';
import { playerName } from '../playerNames';
import s from './chrome.module.css';

/** The table's presentation mode (issue #267). */
export type TableMode = 'overview' | 'focus';

interface Props {
  view: GameView;
  /** The derived presentation mode; drives the indicator's focus emphasis only. */
  mode: TableMode;
  /** The receiver's own seat id, to phrase the active player as "Your turn". */
  localId?: PlayerId;
  /**
   * Set the receiver's priority-stop preferences (issue #264). When provided, the
   * expanded step list renders a per-step stop toggle; the answer is the full new set,
   * which the server stores and echoes back in `view.stops` (the toggles' only source
   * of truth — nothing is kept client-side). Omitted on the read-only game-over board.
   */
  onSetStops?: (stops: Phase[]) => void;
}

/** Readable name of each step, shown compact (current step) and expanded. */
const STEP_NAME: Record<Phase, string> = {
  untap: 'Untap',
  upkeep: 'Upkeep',
  draw: 'Draw',
  precombat_main: 'Main Phase 1',
  begin_combat: 'Begin Combat',
  declare_attackers: 'Declare Attackers',
  declare_blockers: 'Declare Blockers',
  combat_damage: 'Combat Damage',
  end_combat: 'End of Combat',
  postcombat_main: 'Main Phase 2',
  end: 'End Step',
  cleanup: 'Cleanup',
};

/**
 * The five phase groups shown as the compact progress treatment, in turn order.
 * `main` deliberately appears twice (pre- and post-combat). A group's membership is
 * a fixed classification of the phase sequence — it derives nothing from history and
 * counts nothing; it only maps the single `view.phase` to where it sits.
 */
const PHASE_GROUPS: ReadonlyArray<{ id: string; label: string; phases: readonly Phase[] }> = [
  { id: 'beginning', label: 'Beginning', phases: ['untap', 'upkeep', 'draw'] },
  { id: 'main-1', label: 'Main', phases: ['precombat_main'] },
  {
    id: 'combat',
    label: 'Combat',
    phases: [
      'begin_combat',
      'declare_attackers',
      'declare_blockers',
      'combat_damage',
      'end_combat',
    ],
  },
  { id: 'main-2', label: 'Main', phases: ['postcombat_main'] },
  { id: 'ending', label: 'Ending', phases: ['end', 'cleanup'] },
];

export function PhaseIndicator({ view, mode, localId, onSetStops }: Props) {
  // Expansion is ephemeral presentation, defaulting collapsed — a fresh mount from
  // one GameView renders compact (nothing load-bearing across messages).
  const [expanded, setExpanded] = useState(false);
  const listId = useId();

  // The current stop set is the server's echo (issue #264) — the only source of the
  // toggles' state; the client stores nothing. Toggling a step sends the full new set.
  const stops = view.stops ?? [];
  const toggleStop = (phase: Phase): void => {
    if (!onSetStops) return;
    onSetStops(stops.includes(phase) ? stops.filter((p) => p !== phase) : [...stops, phase]);
  };

  const isLocalTurn = view.active_player !== '' && view.active_player === localId;
  const activeLabel =
    view.active_player === ''
      ? 'Active player —'
      : isLocalTurn
        ? 'Your turn'
        : `${playerName(view, view.active_player)}'s turn`;

  const currentGroup = PHASE_GROUPS.findIndex((group) => group.phases.includes(view.phase));

  return (
    <div
      data-testid="phase-indicator"
      data-mode={mode}
      className={s.indicator}
      role="status"
      aria-label="Turn and phase"
    >
      <button
        type="button"
        className={s.indicatorSummary}
        aria-expanded={expanded}
        aria-controls={listId}
        onClick={() => setExpanded((open) => !open)}
        data-testid="indicator-toggle"
      >
        <span className={s.indicatorTurn} data-testid="indicator-turn">
          Turn {view.turn > 0 ? view.turn : '—'}
        </span>
        <span className={s.indicatorActive} data-testid="indicator-active">
          {activeLabel}
        </span>
        <span className={s.indicatorStep} data-testid="indicator-step">
          {STEP_NAME[view.phase]}
        </span>
        {/* Lightweight phase-group progress. Decorative: the semantic step list is
            the expanded <ol> below, so this row is hidden from assistive tech. */}
        <span className={s.indicatorGroups} aria-hidden="true" data-testid="indicator-groups">
          {PHASE_GROUPS.map((group, index) => (
            <span
              key={group.id}
              data-testid={`indicator-group-${group.id}`}
              data-current={index === currentGroup || undefined}
              data-passed={currentGroup >= 0 && index < currentGroup ? true : undefined}
              className={s.indicatorGroup}
            >
              {group.label}
            </span>
          ))}
        </span>
        <span
          className={s.indicatorChevron}
          aria-hidden="true"
          data-expanded={expanded || undefined}
        >
          ▾
        </span>
      </button>
      {mode === 'focus' && (
        <span className={s.indicatorDecision} data-testid="indicator-decision">
          Decision
        </span>
      )}
      {/* Auto-pass indicator (issue #264): reaching this state passed priority on the
          receiver's behalf. Display-only and transient — a plain status badge. */}
      {view.auto_passed && (
        <span className={s.indicatorAutoPassed} data-testid="auto-passed-indicator" role="status">
          Auto-passed
        </span>
      )}
      {expanded && (
        <ol id={listId} className={s.indicatorSteps} data-testid="indicator-steps">
          {PHASES.map((phase) => {
            const current = phase === view.phase;
            const stopped = stops.includes(phase);
            return (
              <li
                key={phase}
                data-testid={`indicator-step-${phase}`}
                data-phase={phase}
                data-current={current || undefined}
                data-stop={stopped || undefined}
                aria-current={current ? 'step' : undefined}
                className={
                  current
                    ? cx(s.indicatorFullStep, s.indicatorFullStepCurrent)
                    : s.indicatorFullStep
                }
              >
                <span className={s.indicatorFullStepName}>{STEP_NAME[phase]}</span>
                {/* Per-step stop toggle (issue #264): opt into stopping here even when
                    idle. Rendered only when a setter is wired (never on the read-only
                    game-over board). `aria-pressed` reflects the server's current set. */}
                {onSetStops && (
                  <button
                    type="button"
                    className={s.stopToggle}
                    data-testid={`stop-toggle-${phase}`}
                    data-stop={stopped || undefined}
                    aria-pressed={stopped}
                    aria-label={`Stop at ${STEP_NAME[phase]}`}
                    onClick={() => toggleStop(phase)}
                  >
                    {stopped ? 'Stop' : 'Auto'}
                  </button>
                )}
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
}
