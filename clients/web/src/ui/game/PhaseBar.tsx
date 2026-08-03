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
  onStop(phase: Phase, scope: StopScope): void
}) {
  return (
    <div className={`phase-bar ${className}`}>
      <h2 className="turn-info">
        Turn {turn} — {active}
      </h2>
      <ul className="phase-pills" aria-label="Turn steps">
        {steps.map((step) => (
          <li key={step.phase}>
            <button
              className={[
                'phase',
                step.current ? 'phase-current' : '',
                step.stop !== 'none' ? `phase-stop-${step.stop}` : '',
                step.passed ? 'phase-passed' : '',
              ]
                .filter(Boolean)
                .join(' ')}
              title={`${step.label} — ${scopeWording(step.stop)}`}
              aria-label={`${step.label}, ${scopeWording(step.stop)}`}
              aria-current={step.current ? 'step' : undefined}
              onClick={() => onStop(step.phase, nextScope(step.stop))}
            >
              {step.short}
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}
