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
 * stop set**, whose sole source of truth is `view.stops` (and, since issue #455,
 * `view.own_turn_stops`) — nothing is stored client-side, so a reconnect
 * rebuilds the toggles from one message.
 *
 * ## A stop has three settings, because the server has three answers (#455)
 *
 * The server keeps two stop lists per seat: one that fires on any turn, and one
 * that fires only while the seat is the active player — and it seeds a *human
 * default* into the second, so a turn never fast-forwards past its owner's own
 * main phases. A two-state toggle could not draw a seat that already has stops
 * it never set, so each step cycles **Auto → Your turn → Always → Auto** and
 * every click sends both whole lists. Nothing here decides where a stop
 * *should* be: the drawn state is whatever the two lists on the view say.
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
import { turnTrail } from './turnTrail';
import s from './plaque.module.css';

export interface PhasePlaqueProps {
  /** The one authoritative view. Every line below is read straight off it. */
  view: GameView;
  /**
   * Set the receiver's priority-stop preferences (ADR 0020, issues #264 and
   * #455). When absent — the read-only game-over board, or a spectator — the step
   * list still discloses, but carries no toggles. The answer is the full new
   * preference: both lists, every time, so clearing a seeded default sticks.
   */
  onSetStops?: (stops: Phase[], ownTurn: Phase[]) => void;
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

/**
 * The plaque's one accessible sentence (issue #455). §5 fixes the two drawn
 * lines; this is what assistive technology hears, and it carries the **turn
 * ordinal** in every form — including the compact one, which drops the drawn
 * ownership line. The playtest #455 records lost exactly this fact ("the player
 * believes they're still in turn 1; the game is at turn 2"), and an ordinal that
 * only survives in the wide layout would lose it again on a phone.
 */
function plaqueSentence(view: GameView, title: string, ownership: string): string {
  return `Turn ${view.turn}. ${title}. ${ownership}.`;
}

/** How a step's stop is set: not at all, on your own turn only, or on every turn. */
type StopMode = 'auto' | 'own' | 'any';

/** The three settings in cycle order, so one click advances to the next. */
const STOP_CYCLE: Record<StopMode, StopMode> = { auto: 'own', own: 'any', any: 'auto' };

/** The word drawn on the toggle, and the phrase the accessible name ends with. */
const STOP_LABEL: Record<StopMode, { text: string; described: string }> = {
  auto: { text: 'Auto', described: 'passed automatically' },
  own: { text: 'Your turn', described: 'stop on your turn' },
  any: { text: 'Always', described: 'stop on every turn' },
};

/**
 * The accessible sentence for the auto-passed steps of the settle that produced
 * this view (issue #455) — the words behind the badge that until now could only
 * say "Auto-passed".
 *
 * `auto_passed_steps` is a *path*, so a settle that crossed a turn boundary can
 * name a step twice; the sentence de-duplicates for reading while the wire value
 * is left alone. Empty when the settle did not act for this receiver, which is
 * also when the badge does not render.
 */
function autoPassedSentence(view: GameView): string {
  const steps = view.auto_passed_steps ?? [];
  if (steps.length === 0) return '';
  const names = [...new Set(steps)].map((phase) => STEP_NAME[phase]);
  const list =
    names.length === 1
      ? names[0]
      : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
  return `Auto-passed for you at ${list}.`;
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
  const ownTurnStops = view.own_turn_stops ?? [];
  const states = pipStates(view.phase);
  // The path this turn has already taken — read off `view.log`, so the marks below
  // rebuild from the same single message the rest of the plaque does. It says
  // "the turn has been here", which is a weaker claim than the one beside it.
  const passed = new Set<Phase>(turnTrail(view));
  // Where the settle acted *for this seat* (issue #455). This is the stronger
  // claim the wire could not make before — ADR 0020 shipped one boolean for a
  // whole settle — and it is the server's own statement, not an inference from
  // the trail: a step the turn merely went through is not a step you were skipped
  // at (another seat may have held priority, or you may have acted there yourself).
  const skipped = new Set<Phase>(view.auto_passed_steps ?? []);
  const skippedSentence = autoPassedSentence(view);

  const stopMode = (phase: Phase): StopMode =>
    stops.includes(phase) ? 'any' : ownTurnStops.includes(phase) ? 'own' : 'auto';

  // The full new preference on every toggle — both lists, since the server replaces
  // the whole thing and `view.stops`/`view.own_turn_stops` are the toggles' only
  // source of truth. A step never rides both lists: `any` is the wider claim.
  const cycleStop = (phase: Phase): void => {
    if (!onSetStops) return;
    const next = STOP_CYCLE[stopMode(phase)];
    const withoutPhase = (list: Phase[]): Phase[] => list.filter((p) => p !== phase);
    onSetStops(
      next === 'any' ? [...withoutPhase(stops), phase] : withoutPhase(stops),
      next === 'own' ? [...withoutPhase(ownTurnStops), phase] : withoutPhase(ownTurnStops),
    );
  };

  return (
    <div
      className={cx(s.plaque, compact && s.compact)}
      data-testid="phase-plaque"
      data-compact={compact || undefined}
      // The turn ordinal as data, so it is assertable and available to CSS in
      // both forms even where §5's drawn ownership line is dropped.
      data-turn={view.turn}
    >
      {/* The transient "Auto-passed" badge (shipped behaviour, ADR 0020). It sits
          above the plate rather than inside it: the plate's two text lines are
          the drawn form, and 6b has only one.

          The drawn word is unchanged — the plate has no room for a step list and
          the badge is a glance, not a report — but issue #455's `auto_passed_steps`
          now supplies the *accessible* name, so what a screen reader is told is the
          same fact the step list marks below rather than a bare "it happened".
          There is no animation here in either path, so the reduced-motion reading
          is identical to the default one. */}
      {view.auto_passed && (
        <span
          className={s.autoPassed}
          role="status"
          data-testid="plaque-auto-passed"
          aria-label={skippedSentence || undefined}
        >
          Auto-passed
        </span>
      )}

      <div className={s.frame}>
        <div className={s.face}>
          <div className={s.text} role="status" aria-label={plaqueSentence(view, title, ownership)}>
            <span className={s.title} data-testid="plaque-step">
              {title}
            </span>
            {/* §5's second line: the ownership sentence with the pip row beside
                it, exactly as the plaque is drawn. 6b drops the sentence and
                keeps the pips, which is what makes it one line. */}
            <span className={s.subline}>
              {!compact && (
                <>
                  {/* Issue #455: the turn ordinal, drawn beside §5's ownership
                      sentence rather than folded into it, so the sentence the
                      spec fixes is unchanged and the number the settle loop
                      makes easy to lose is on the plate. The compact row keeps
                      its one-line form (it drops the ownership line too) and
                      answers through the accessible sentence above. */}
                  <span className={s.turn} data-testid="plaque-turn">
                    T{view.turn}
                  </span>
                  <span className={s.ownership} data-testid="plaque-ownership">
                    {ownership}
                  </span>
                </>
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
            const mode = stopMode(phase);
            const wentThrough = passed.has(phase);
            const wasSkipped = skipped.has(phase);
            return (
              <li
                key={phase}
                className={cx(s.step, current && s.stepCurrent, wentThrough && s.stepPassed)}
                data-testid={`plaque-step-${phase}`}
                data-phase={phase}
                data-current={current || undefined}
                data-passed={wentThrough || undefined}
                data-skipped={wasSkipped || undefined}
                data-stop={mode === 'auto' ? undefined : mode}
                aria-current={current ? 'step' : undefined}
              >
                <span className={s.stepLead}>
                  <span className={s.stepName}>{STEP_NAME[phase]}</span>
                  {/* The §8 "path taken" mark (issue #455): a glyph, not a hue,
                      so the trail reads on a colour-blind path and under
                      reduced motion alike — there is no animation to remove.

                      Two marks, two different claims. The check is the turn's
                      path (from `view.log`); the arrow is the stronger one the
                      server now makes for this seat alone (`auto_passed_steps`),
                      and it replaces the check where both are true so a step
                      never carries two glyphs saying overlapping things. */}
                  {wasSkipped ? (
                    <span
                      className={s.stepPassedMark}
                      data-testid={`plaque-skipped-${phase}`}
                      role="img"
                      aria-label="passed for you here"
                    >
                      ↷
                    </span>
                  ) : (
                    wentThrough && (
                      <span
                        className={s.stepPassedMark}
                        data-testid={`plaque-passed-${phase}`}
                        role="img"
                        aria-label="already passed this turn"
                      >
                        ✓
                      </span>
                    )
                  )}
                </span>
                {onSetStops && (
                  <button
                    type="button"
                    className={s.stopToggle}
                    data-testid={`plaque-stop-${phase}`}
                    data-stop={mode === 'auto' ? undefined : mode}
                    // Three settings, so `aria-pressed` (a binary) would have to
                    // lie about one of them; the accessible name states the
                    // current setting outright instead.
                    aria-label={`Stop at ${STEP_NAME[phase]} — ${STOP_LABEL[mode].described}`}
                    onClick={() => cycleStop(phase)}
                  >
                    {STOP_LABEL[mode].text}
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
