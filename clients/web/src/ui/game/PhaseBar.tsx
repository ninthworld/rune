/**
 * The turn, as a strip that is always drawn (§4.1).
 *
 * Not a control that expands: the whole turn is on screen, the step the game is in is the lit
 * pane in a row of unlit ones (§5.5), and each step is also **the control that sets a stop
 * there**. A preference divorced from the strip it applies to is one nobody edits.
 *
 * Clicking a step cycles what it is worth stopping for — never, your turns, every turn — and
 * sends the whole preference, because `set_stops` replaces it and is never a delta. Nothing
 * about a stop is stored in this client.
 *
 * Where the strip sits is the stylesheet's answer, not this component's: under the topbar on a
 * phone, and across the middle of the table where there is room for it there.
 *
 * **A watcher gets the strip and not the control.** A stop is a preference belonging to a seat,
 * and a spectator has none — so with no `onStop` the steps are drawn as what they are, the turn,
 * and there is no button to press. Not a disabled one: a disabled control says *not now*, and the
 * honest thing to say to somebody who is not at the table is nothing.
 */
import type { Phase } from './../../protocol'
import { nextScope, scopeWording, type Step, type StopScope } from './../../turn'

export function PhaseBar({
  className,
  turn,
  active,
  steps,
  onStop,
}: {
  className: string
  turn: number
  /** Whose turn it is, in the words the view named them by. */
  active: string
  steps: readonly Step[]
  /** Absent for a reader with no seat to hold a preference: the strip becomes a read-out. */
  onStop?(phase: Phase, scope: StopScope): void
}) {
  return (
    <div className={`phase-bar ${className}`}>
      <h2 className="turn-info">
        Turn {turn} — {active}
      </h2>
      <ul className="phase-pills" aria-label="Turn steps">
        {steps.map((step) => {
          const pill = [
            'phase',
            step.current ? 'phase-current' : '',
            onStop && step.stop !== 'none' ? `phase-stop-${step.stop}` : '',
            step.passed ? 'phase-passed' : '',
          ]
            .filter(Boolean)
            .join(' ')
          return (
            <li key={step.phase}>
              {onStop ? (
                <button
                  className={pill}
                  title={`${step.label} — ${scopeWording(step.stop)}`}
                  aria-label={`${step.label}, ${scopeWording(step.stop)}`}
                  aria-current={step.current ? 'step' : undefined}
                  onClick={() => onStop(step.phase, nextScope(step.stop))}
                >
                  {step.short}
                </button>
              ) : (
                <span
                  className={pill}
                  title={step.label}
                  aria-label={step.label}
                  aria-current={step.current ? 'step' : undefined}
                >
                  {step.short}
                </span>
              )}
            </li>
          )
        })}
      </ul>
    </div>
  )
}
