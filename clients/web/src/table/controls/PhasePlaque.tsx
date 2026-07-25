/**
 * The **phase plaque** — the foot of the lower-right control cluster
 * (`docs/design/control-language.md` §5, issue #534, under
 * [ADR 0032](../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * The hexagonal 268 × 68 plate the approved baselines draw: the current step,
 * whose turn it is, the five phase-group pips, and a forward chevron that
 * discloses the twelve-step list where each step carries its stop toggle.
 *
 * ## It replaces a presentation, not a behaviour
 *
 * `table/PhaseIndicator.tsx` shipped all of this top-centre (C6). The plaque
 * moves the *presentation* to the cluster and keeps the *semantics* exactly:
 * `STEP_NAME` is the same twelve labels, `PHASE_GROUPS` the same five groups in
 * the same order (both carried into `phaseSteps.ts`), the step list is the same
 * `PHASES` sequence, and each step's toggle still answers with the **full new
 * stop set**, whose sole source of truth is `view.stops` — nothing is stored
 * client-side, so a reconnect rebuilds the toggles from one message.
 *
 * ## The chevron is a door, not a verb (D4)
 *
 * It reads as "where the turn is going next", and that is exactly what the step
 * list shows. It must never be wired to a game action: no `pass_until`,
 * `advance_phase`, or hold-priority action exists (**GAP-3**), and the escape
 * hatch from an auto-pass chain is a *stop*, exactly as ADR 0020 specifies. The
 * only message this component can produce is `set_stops`.
 *
 * ## Expansion is ephemeral
 *
 * The list opens closed on every fresh mount, so nothing here is load-bearing
 * across messages — the same invariant the shipped indicator holds.
 */
import { useId, useState } from 'react';
import type { GameView, Phase } from '../../protocol';
import { PHASES } from '../../protocol';
import { cx } from '../../chrome/cx';
import { playerName } from '../../playerNames';
import { PHASE_GROUPS, STEP_NAME, pipStates } from './phaseSteps';
import s from './plaque.module.css';

export interface PhasePlaqueProps {
  /** The one authoritative view. Every line below is read straight off it. */
  view: GameView;
  /**
   * Set the receiver's priority-stop preferences (ADR 0020, issue #264). When
   * absent — the read-only game-over board, or a spectator — the step list still
   * discloses, but carries no toggles. The answer is the full new set.
   */
  onSetStops?: (stops: Phase[]) => void;
  /**
   * Panel 6b's one-line form, used by the compact cluster. The ownership line is
   * dropped; the title, the pips, and the chevron all stay, because those are the
   * two things §3.3 says are always available.
   */
  compact?: boolean;
  /**
   * Rule 8: the server offered nothing, so the title line reads "Waiting" instead
   * of the step name (§4.2). The step name still rides the accessible sentence —
   * the turn has not stopped, only the player's part in it.
   */
  waiting?: boolean;
}

/**
 * The ownership line (§5): `Your turn` / `Priority` / `<name>'s turn`.
 *
 * Own turn wins outright, as panel 6 draws it. Holding priority on someone
 * else's turn is the 2.5D baseline's `Priority`, and it is the more useful of the
 * two facts there — it is the line that says the game is waiting on *you*.
 */
function ownershipLine(view: GameView): string {
  if (view.active_player !== '' && view.active_player === view.you) return 'Your turn';
  if (view.priority_player !== undefined && view.priority_player === view.you) return 'Priority';
  if (view.active_player === '') return 'Active player —';
  return `${playerName(view, view.active_player)}'s turn`;
}

export function PhasePlaque({ view, onSetStops, compact, waiting }: PhasePlaqueProps) {
  // Ephemeral presentation, defaulting closed: a fresh mount from one GameView
  // renders the plaque alone (nothing load-bearing across messages).
  const [open, setOpen] = useState(false);
  const listId = useId();

  const stepName = STEP_NAME[view.phase];
  const title = waiting ? 'Waiting' : stepName;
  const ownership = ownershipLine(view);
  const stops = view.stops ?? [];
  const states = pipStates(view.phase);

  // The full new set on every toggle — the server stores it and echoes it back in
  // `view.stops`, which is the toggles' only source of truth.
  const toggleStop = (phase: Phase): void => {
    if (!onSetStops) return;
    onSetStops(stops.includes(phase) ? stops.filter((p) => p !== phase) : [...stops, phase]);
  };

  return (
    <div
      className={cx(s.plaque, compact && s.compact)}
      data-testid="phase-plaque"
      data-compact={compact || undefined}
    >
      {/* The transient "Auto-passed" badge (shipped behaviour, ADR 0020). It sits
          above the plate rather than inside it: the plate's two text lines are
          the drawn form, and 6b has only one. */}
      {view.auto_passed && (
        <span className={s.autoPassed} role="status" data-testid="plaque-auto-passed">
          Auto-passed
        </span>
      )}

      <div className={s.frame}>
        <div className={s.face}>
          <div className={s.text} role="status" aria-label="Turn and phase">
            <span className={s.title} data-testid="plaque-step">
              {title}
            </span>
            {/* §5's second line: the ownership sentence with the pip row beside
                it, exactly as the plaque is drawn. 6b drops the sentence and
                keeps the pips, which is what makes it one line. */}
            <span className={s.subline}>
              {!compact && (
                <span className={s.ownership} data-testid="plaque-ownership">
                  {ownership}
                </span>
              )}
              {/* Decorative: the semantic step sequence is the <ol> behind the
                  chevron, so the row is hidden from assistive tech (§5.1). */}
              <span className={s.pips} aria-hidden="true" data-testid="plaque-pips">
                {PHASE_GROUPS.map((group, index) => (
                  <span
                    key={group.id}
                    className={s.pip}
                    data-pip={group.id}
                    data-state={states[index]}
                  />
                ))}
              </span>
            </span>
          </div>

          {/* D4: a DISCLOSURE, never a game action. Its only output is set_stops.
              The gold cue fires while the view reports an auto-pass, pointing at
              the one real escape hatch (a stop) — display-only, and dropped by
              the next view exactly like the badge. */}
          <button
            type="button"
            className={s.chevron}
            aria-expanded={open}
            aria-controls={listId}
            aria-label="Turn steps and stops"
            title="Turn steps and stops"
            data-testid="plaque-chevron"
            data-cue={view.auto_passed || undefined}
            onClick={() => setOpen((wasOpen) => !wasOpen)}
          >
            {/* Two glyphs, one drawn: the default state rotates the forward
                chevron, and `prefers-reduced-motion` swaps to a static down
                glyph instead of tweening (§5.2, §12). */}
            <span className={s.glyphTurn} aria-hidden="true" data-open={open || undefined}>
              ›
            </span>
            <span className={s.glyphSwap} aria-hidden="true" data-open={open || undefined}>
              {open ? '⌄' : '›'}
            </span>
          </button>
        </div>
      </div>

      {/* The step list opens ABOVE the plaque — the cluster is bottom-anchored —
          and is viewport-clamped, per `ui-requirements.md` (a phase expansion
          renders entirely within the viewport). */}
      {open && (
        <ol id={listId} className={s.steps} data-testid="plaque-steps">
          {PHASES.map((phase) => {
            const current = phase === view.phase;
            const stopped = stops.includes(phase);
            return (
              <li
                key={phase}
                className={cx(s.step, current && s.stepCurrent)}
                data-testid={`plaque-step-${phase}`}
                data-phase={phase}
                data-current={current || undefined}
                data-stop={stopped || undefined}
                aria-current={current ? 'step' : undefined}
              >
                <span className={s.stepName}>{STEP_NAME[phase]}</span>
                {onSetStops && (
                  <button
                    type="button"
                    className={s.stopToggle}
                    data-testid={`plaque-stop-${phase}`}
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
