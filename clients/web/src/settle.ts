/**
 * What the settle did while you were not being asked (`docs/client-design.md` §6.9, issue #709).
 *
 * The server acts for a seat that has nothing to answer — that is the *settle*, and it is the
 * product hypothesis this client exists to test (`docs/brief.md`). It is also the moment a player
 * is most easily lost: the board is suddenly different, a creature is gone, and nothing on screen
 * says why. A run of log lines is where that information lived, and reading a log is not the same
 * as watching a game.
 *
 * ## It is a function of one view, and deliberately not a queue
 *
 * The issue this answers asked for a presentation queue with bounded dwell. This does not have
 * one, and the reason is the hard rule it would break: **no client state is load-bearing across
 * messages**. A queue is exactly that — a thing the client holds which the next `GameView` cannot
 * reconstruct — and every failure mode the issue then has to defend against (a reconnect
 * replaying stale presentation, an interrupted settle, a queue outliving the game it describes)
 * is a consequence of holding it.
 *
 * The server already states what this receiver missed. `auto_passed_from` marks the log sequence
 * a settle began at *for this seat*, and `auto_passed_steps` is the path it took. So "what did I
 * miss" is answered by the view in front of you, and the answer is replaced wholesale by the next
 * one. Reconnect lands on the current state carrying no mark, and therefore shows nothing —
 * not because anything was cancelled, but because there is nothing to cancel.
 *
 * That is also why nothing here animates. Motion belongs to `motion.ts`, which plays the last
 * quarter-second of objects reaching where the view already puts them; this is the *words*, and
 * a player who has turned motion off reads exactly the same ones. The two are independent by
 * construction rather than by a flag.
 *
 * ## Bounded by construction
 *
 * A settle can cross a whole turn and the log window is finite, so the honest report is "here is
 * what I still have" and never a promise of completeness. The band draws at most [`SHOWN`]
 * entries and says how many more there were; the rest are in the log, which is where a player who
 * wants the whole sequence should be sent. Nothing here grows a region or scrolls (§3).
 */
import { describe, kindOf, type LogKind } from './game-log'
import { list } from './normalize'
import type { GameLogEntry, GameView } from './protocol'
import { passedEvents, passedRuns, phaseLabel } from './turn'

/**
 * The most missed events drawn at once.
 *
 * Three, because the band is one line of chrome above the hand and the point is legibility rather
 * than completeness: a player who sees "Shock resolved · Grizzly Bears died · +2 more" knows both
 * what happened and that there is more, in less time than it takes to read four. The log is the
 * complete record and is one click away.
 */
export const SHOWN = 3

/** One thing the settle did, in the words the log would use for it. */
export interface SettleEvent {
  /** The log's own phrasing, so the band and the log cannot describe one event two ways. */
  readonly text: string
  /** What sort of event it was, for the same tint the log line carries. */
  readonly kind: LogKind
  /** The log sequence it came from — a stable key, and the log's own ordering. */
  readonly sequence: number
}

/**
 * What a settle did, ready to draw: where the game went, and what happened on the way.
 *
 * `undefined` when the server marked no settle for this receiver, which is the ordinary case —
 * a player who is being asked something missed nothing.
 */
export interface Settle {
  /**
   * Where the game moved, as a sentence: *"through combat to your second main"*, or
   * *"to Ada's turn"* when it crossed a boundary.
   *
   * Built from the path the server sent rather than from the difference between two views, so a
   * settle that crossed several steps says all of them and one that returned to a step it had
   * already passed says it twice.
   */
  readonly path: string
  /** What happened while it moved, most recent last, at most [`SHOWN`] of them. */
  readonly events: readonly SettleEvent[]
  /** How many more there were than the band draws. Zero when it drew them all. */
  readonly more: number
  /** Whether the settle crossed into a different turn — the one jump worth its own word. */
  readonly crossedTurn: boolean
}

/**
 * The settle this view describes, or `undefined` when it describes none.
 *
 * Everything is read off the view: the path (`auto_passed_steps`), the mark (`auto_passed_from`),
 * and the log the mark points into. Nothing is remembered and nothing is inferred from a previous
 * view — which is what makes this correct across a reconnect, a refresh, and a missed frame.
 */
export function settleOf(view: GameView, playerName: (id: string) => string): Settle | undefined {
  const runs = passedRuns(list(view.auto_passed_steps))
  const missed = passedEvents(view)
  // The server said a settle happened if it sent either the path or the mark. Both empty is a
  // view a player was asked for, and there is nothing to report about it.
  if (runs.length === 0 && missed.length === 0) return undefined

  const events = missed.map(entryOf(playerName))
  // Step changes are the settle's own footprints: the path above already says where it went, and
  // repeating "the game moved to combat" beside it is the same fact twice. Everything else is
  // something a player would have watched happen.
  const worth = events.filter((event) => event.kind !== 'step')
  const shown = worth.slice(-SHOWN)

  return {
    path: pathOf(runs, view),
    events: shown,
    more: worth.length - shown.length,
    crossedTurn: runs.length > 1 || runs.some((run) => run.turn !== view.turn),
  }
}

/** One log entry as a settle event, phrased by the log's own describer. */
const entryOf =
  (playerName: (id: string) => string) =>
  (entry: GameLogEntry): SettleEvent => ({
    text: describe(entry.event, playerName),
    kind: kindOf(entry.event),
    sequence: entry.sequence,
  })

/**
 * The path, as words.
 *
 * Named by where it **ended**, because that is the question a player actually has — *where am I
 * now* — with the length of the run behind it as the answer to *how far did this go*. A run of
 * one names only the step; a longer one says how many, because "through four steps" is the whole
 * of what makes a jump not look like a glitch.
 */
function pathOf(
  runs: readonly { turn: number; steps: readonly { label: string }[] }[],
  view: GameView,
): string {
  const steps = runs.flatMap((run) => run.steps)
  const here = view.phase === undefined ? undefined : phaseLabel(view.phase)
  if (steps.length === 0) return here === undefined ? 'The game moved on' : `Now at ${here}`
  if (steps.length === 1) {
    const only = steps[0]?.label ?? ''
    return `Passed ${only}`
  }
  return `Passed ${steps.length} steps${here === undefined ? '' : `, now at ${here}`}`
}
