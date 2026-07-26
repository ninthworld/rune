/**
 * The **turn trail** — which steps the current turn has already passed through
 * (issue #455, `docs/design/visual-system.md` §8 "Phase / step change":
 * *"skipped phases compress into one wipe showing the path taken"*).
 *
 * ## Why this exists
 *
 * The room settles every auto-passable seat before it broadcasts
 * ([ADR 0020](../../../../../docs/decisions/0020-priority-automation.md)), so a
 * single `GameView` can be several steps — or a whole turn — further on than the
 * one before it. The plaque draws `view.phase` and nothing else, which answers
 * *"where am I"* but never *"what did I just pass through"*. That is the
 * playtest complaint #455 records verbatim: the player believed they were still
 * in turn 1 while the game was at turn 2.
 *
 * ## It is a function of ONE view
 *
 * The trail is read off `view.log` — the server's own bounded window of
 * `step_changed` entries (ADR 0021), which rides *inside* the view. Nothing here
 * remembers a previous message, diffs two views, or starts a clock, so the whole
 * plaque still rebuilds from one `GameView` + prompt: a hard reload mid-turn
 * reproduces exactly the same trail, and a reconnect whose window opens
 * mid-turn simply reports the shorter path it can actually see.
 *
 * ## It states nothing the server did not
 *
 * Every entry is a `step_changed` the server recorded, compared on its own
 * `turn` field. No legality, no priority reasoning, and — deliberately — **no
 * claim about which steps the seat was auto-passed at**. The wire carries one
 * `auto_passed` boolean for the whole settle (ADR 0020) and never names the
 * steps it covered, so this module says only *"the turn has been here"*. The
 * plaque phrases it that way too; over-claiming would be inventing game
 * information, which is exactly what #455 is trying to stop losing.
 *
 * Consumed by {@link ../PhasePlaque.PhasePlaque}, its only production caller.
 */
import type { GameView, Phase } from '../../protocol';

/**
 * The steps the **current** turn has already passed through, in the order the
 * server recorded them, with the step the view is on now excluded.
 *
 * - Entries from an earlier turn contribute nothing: the window may hold several
 *   turns and only this one is the path being drawn.
 * - Consecutive duplicates collapse, so a step re-broadcast inside one turn
 *   appears once.
 * - The current step is dropped from the tail — the trail is where the turn has
 *   *been*, and the plaque already draws where it *is*.
 * - Empty whenever the log window carries no `step_changed` for this turn: a
 *   fresh mount, a reconnect that opened mid-turn, or a server that trimmed the
 *   window. An empty trail draws nothing rather than guessing a path.
 */
export function turnTrail(view: GameView): Phase[] {
  const steps: Phase[] = [];
  for (const { event } of view.log ?? []) {
    if (event.type !== 'step_changed') continue;
    // A different turn belongs to the path *before* this one. The log window is
    // sequence-ordered, so this turn's entries are a single contiguous run and
    // skipping the others is all that is needed — no reset can ever be reached.
    if (event.turn !== view.turn) continue;
    if (steps[steps.length - 1] !== event.phase) steps.push(event.phase);
  }
  while (steps.length > 0 && steps[steps.length - 1] === view.phase) steps.pop();
  return steps;
}
