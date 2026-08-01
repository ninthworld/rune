/**
 * Turn flow: where the game is, who it is waiting for, and where it will stop next time.
 *
 * Four questions a player asks constantly, answered in one place so the header, the strip, and
 * the side column cannot answer them differently:
 *
 * 1. **What step is this?** The twelve steps of a turn are a fixed sequence (CR 500.1), and the
 *    server names the one the game is in. Drawing all of them rather than only the current one
 *    is what makes the position legible — a step is only meaningful next to the ones it is
 *    between.
 * 2. **Who is the game waiting for?** Read off the fields the server stated — a result, a
 *    submission in flight, an action list, a priority holder — and never off the phase name.
 *    "Nobody has priority in the untap step" is a rule, and rules are the server's.
 * 3. **Where will it stop for me?** `stops` and `own_turn_stops` are the *effective* preference
 *    the server is honouring, defaults included, so the controls read straight off the view and
 *    nothing is stored here. A change is sent as the whole preference, because that is what the
 *    message replaces (`docs/protocol.md`).
 * 4. **What did I miss?** `auto_passed_steps` is the path a settle took on this receiver's
 *    behalf. It is a path and not a set — a revisited position appears twice — so it is grouped
 *    into per-turn runs with every occurrence kept.
 *
 * Nothing here decides whether a step *should* stop, whether a seat *may* act, or what automation
 * will do next. Those are the server's, and this module has nothing to answer them with.
 */
import { list } from './normalize'
import { Phase, type ClientMessage, type AutoPassedStep, type GameView } from './protocol'

const PHASE_LABELS: Record<string, string> = {
  untap: 'Untap',
  upkeep: 'Upkeep',
  draw: 'Draw',
  precombat_main: 'Precombat main',
  begin_combat: 'Begin combat',
  declare_attackers: 'Declare attackers',
  declare_blockers: 'Declare blockers',
  combat_damage: 'Combat damage',
  end_combat: 'End combat',
  postcombat_main: 'Postcombat main',
  end: 'End',
  cleanup: 'Cleanup',
}

/** An unknown classifier is rendered generically rather than guessed at (`docs/protocol.md`). */
export const phaseLabel = (phase: string): string => PHASE_LABELS[phase] ?? phase

/** A shorter form for the strip, where twelve of them sit side by side. */
const SHORT_LABELS: Record<string, string> = {
  precombat_main: 'Main 1',
  begin_combat: 'Combat',
  declare_attackers: 'Attack',
  declare_blockers: 'Block',
  combat_damage: 'Damage',
  end_combat: 'End comb.',
  postcombat_main: 'Main 2',
}

export const shortPhaseLabel = (phase: string): string =>
  SHORT_LABELS[phase] ?? PHASE_LABELS[phase] ?? phase

/**
 * The turn's steps, in order, from the mirror's own enum.
 *
 * Taken from the schema rather than written out again, so the strip cannot list a step the wire
 * does not have or miss one it does.
 */
export const PHASES: readonly Phase[] = Phase.options

// ---------------------------------------------------------------------------
// Stops
// ---------------------------------------------------------------------------

/**
 * How far a stop preference reaches at one step.
 *
 * The server keeps two lists because a stop answers two different questions — "hand me priority
 * here whoever's turn it is" and "hand me priority here while the turn is mine" — and a step is
 * never on both (`docs/protocol.md`). One control per step with three positions is therefore the
 * exact shape of the preference, rather than two checkboxes that can disagree.
 */
export type StopScope = 'none' | 'own' | 'always'

/** The next position of the control, wrapping. Off, then your turn, then every turn. */
export const nextScope = (scope: StopScope): StopScope =>
  scope === 'none' ? 'own' : scope === 'own' ? 'always' : 'none'

export const scopeWording = (scope: StopScope): string =>
  scope === 'always'
    ? 'stops on every turn'
    : scope === 'own'
      ? 'stops on your turn'
      : 'never stops'

/** One step of the turn, as the strip draws it. */
export interface Step {
  phase: Phase
  label: string
  short: string
  /** The step the game is in right now. */
  current: boolean
  /** The stop the server is honouring here — the effective preference, defaults included. */
  stop: StopScope
  /** The settle acted here on this receiver's behalf, **this turn**. */
  passed: boolean
}

export function steps(view: GameView): readonly Step[] {
  const always = new Set<string>(list(view.stops))
  const own = new Set<string>(list(view.own_turn_stops))
  // Only entries belonging to the turn on screen may mark a step in it: a settle can cross a
  // turn boundary, and its path then carries the previous turn's positions too
  // (`docs/protocol.md`). With no turn stated there is nothing to match against, so nothing is
  // marked rather than everything.
  const passed = new Set<string>(
    list(view.auto_passed_steps)
      .filter((step) => view.turn !== undefined && step.turn === view.turn)
      .map((step) => step.phase),
  )

  return PHASES.map((phase) => ({
    phase,
    label: phaseLabel(phase),
    short: shortPhaseLabel(phase),
    current: phase === view.phase,
    stop: always.has(phase) ? 'always' : own.has(phase) ? 'own' : 'none',
    passed: passed.has(phase),
  }))
}

/**
 * The whole preference, with one step moved to `scope`.
 *
 * `set_stops` replaces both lists at once and is never a delta, which is what lets a player clear
 * the defaults the server seeds. So the message is rebuilt from the lists the view is currently
 * reflecting — the server's own effective answer — rather than from anything remembered here.
 * Empty lists are omitted, as the wire omits them, so "stop nowhere" is the minimal message.
 */
export function withStop(view: GameView, phase: Phase, scope: StopScope): ClientMessage {
  const keep = (phases: readonly Phase[]) => phases.filter((each) => each !== phase)
  const always = keep(list(view.stops))
  const own = keep(list(view.own_turn_stops))
  const withPhase = (phases: readonly Phase[]) => order([...phases, phase])

  const stops = scope === 'always' ? withPhase(always) : order(always)
  const ownTurn = scope === 'own' ? withPhase(own) : order(own)

  return {
    type: 'set_stops',
    ...(stops.length > 0 ? { stops } : {}),
    ...(ownTurn.length > 0 ? { own_turn: ownTurn } : {}),
  }
}

/** Turn order, so the same preference always serializes the same way. */
const order = (phases: readonly Phase[]): Phase[] =>
  [...phases].sort((a, b) => PHASES.indexOf(a) - PHASES.indexOf(b))

// ---------------------------------------------------------------------------
// Who the game is waiting for
// ---------------------------------------------------------------------------

/**
 * What the match is doing, in the order that matters.
 *
 * A finished game outranks everything: nobody is coming. A submission in flight outranks an
 * action list, because the player has already answered and what they need to know is that their
 * click is out there. Only then does the view's own answer — actions to take, or a named
 * priority holder — decide.
 */
export type MatchStatus =
  | { kind: 'over' }
  | { kind: 'sent'; label: string }
  | { kind: 'yours' }
  | { kind: 'waiting'; on?: string }

export function matchStatus(view: GameView, sent?: string): MatchStatus {
  if (view.result) return { kind: 'over' }
  if (sent !== undefined) return { kind: 'sent', label: sent }
  if (list(view.valid_actions).length > 0) return { kind: 'yours' }
  return { kind: 'waiting', on: view.priority_player }
}

/**
 * The status as a sentence.
 *
 * Deliberately says *who* rather than *why*: "waiting for Alice" is a fact the view stated, where
 * "Alice is choosing blockers" would be a guess about a seat whose actions this client cannot
 * see. A priority holder the view did not name leaves the game itself as the subject, which is
 * what a settle running between two broadcasts actually looks like.
 */
export function statusLine(status: MatchStatus, label: (id: string) => string): string {
  switch (status.kind) {
    case 'over':
      return 'The game is over.'
    case 'sent':
      return `Sent “${status.label}” — waiting for the server.`
    case 'yours':
      return 'Your move.'
    case 'waiting':
      return status.on === undefined
        ? 'Waiting — the game is moving on its own.'
        : `Waiting for ${label(status.on)}.`
  }
}

// ---------------------------------------------------------------------------
// What the settle did on your behalf
// ---------------------------------------------------------------------------

/** Consecutive steps a settle passed within one turn, in the order it acted. */
export interface PassedRun {
  turn: number
  steps: readonly { phase: Phase; label: string }[]
}

/**
 * The settle's path, grouped into per-turn runs.
 *
 * Grouped by *adjacency*, never by value: a turn genuinely returned to — a path that crosses a
 * boundary and comes back cannot happen, but two runs in one turn separated by a step the player
 * did act at can — stays two runs, and a position reached twice appears twice inside its run.
 * De-duplicating either would quietly shorten how far the game moved unasked.
 */
export function passedRuns(passed: readonly AutoPassedStep[]): readonly PassedRun[] {
  const runs: PassedRun[] = []
  for (const step of passed) {
    const current = runs.at(-1)
    const entry = { phase: step.phase, label: phaseLabel(step.phase) }
    if (current && current.turn === step.turn) current.steps = [...current.steps, entry]
    else runs.push({ turn: step.turn, steps: [entry] })
  }
  return runs
}
