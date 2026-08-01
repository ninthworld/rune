/**
 * The turn, drawn as the sequence it is — and the same row is where stops are set.
 *
 * Every step of a turn, in order, with the one the game is in marked. A single step name in a
 * header tells a player where they are; the row tells them what is behind and what is ahead,
 * which is the question actually being asked when someone looks at the phase.
 *
 * The stops live here rather than in a settings panel because they are a statement about *these*
 * steps: "stop me here next time" is the same row as "we are here now", and a preference divorced
 * from the strip it applies to is a preference nobody edits. Each step cycles off → your turn →
 * every turn, which is the exact shape of the two lists the server keeps (a step is never on
 * both), and every change is sent as the whole preference because that is what `set_stops`
 * replaces.
 *
 * A step the server passed on the player's behalf **this turn** is marked too, so the settle's
 * path is visible on the same row that explains how to stop it happening again.
 *
 * Three layouts, and the scene picks between them (`scene.ts`, §3 step 7 and §4's table): a rail
 * down the left edge where there is width for one, a horizontal band under the header where the
 * viewport is square, and — collapsed — the current step alone. Collapsing is a real loss and it
 * is the last of them for a reason: the row is what says what is *behind and ahead*, and a chip
 * says only where the game is. What the chip keeps is the one thing §3 lists as never degrading,
 * and the eleven steps it drops are still in the accessibility tree, so nothing about setting a
 * stop becomes unreachable — it becomes unseen.
 */
import type { Phase } from './../../protocol'
import { nextScope, scopeWording, type Step, type StopScope } from './../../turn'

/** How much of the turn there is room to draw (`scene.ts`'s `ladder.rails` and the band). */
export type TurnLayout = 'rail' | 'strip' | 'chip'

export function TurnStrip({
  steps,
  layout = 'rail',
  onStop,
}: {
  steps: readonly Step[]
  layout?: TurnLayout
  onStop(phase: Phase, scope: StopScope): void
}) {
  // Collapsed, the row is the current step and nothing else. The eleven it drops go entirely
  // rather than being hidden in place: a control with no box is a control a pointer cannot reach
  // and a screen reader announces as if it could, which is worse than not offering it. What is
  // lost is per-step stops, and the pace presets behind the gear still set them in bulk.
  const drawn = layout === 'chip' ? steps.filter((step) => step.current) : steps

  return (
    <ol className={`strip strip--${layout}`} aria-label="Turn steps">
      {drawn.map((step) => (
        <li key={step.phase}>
          <button
            type="button"
            className={[
              'strip__step',
              step.current && 'strip__step--now',
              step.passed && 'strip__step--passed',
              step.stop !== 'none' && `strip__step--stop-${step.stop}`,
            ]
              .filter(Boolean)
              .join(' ')}
            aria-current={step.current ? 'step' : undefined}
            // The button's own name is the whole state, because a marker on a twelve-step row is
            // not readable on its own and is invisible to a screen reader.
            aria-label={`${step.label} — ${scopeWording(step.stop)}${
              step.passed ? ', passed for you this turn' : ''
            }`}
            onClick={() => onStop(step.phase, nextScope(step.stop))}
          >
            <span className="strip__name">{layout === 'chip' ? step.label : step.short}</span>
            <span aria-hidden="true" className="strip__mark">
              {step.stop === 'always' ? '●' : step.stop === 'own' ? '◐' : '·'}
            </span>
          </button>
        </li>
      ))}
    </ol>
  )
}
