/**
 * What the controls are currently for — in one word, in how much room, and in what they carry.
 *
 * The dock is where a player's eyes rest between actions, so it is the right place to say what
 * the game wants — and saying it in *colour* is what makes it answerable from peripheral vision,
 * without reading. XMage does this and it is the single most-copied thing about its table: the
 * band under the board changes colour, and a player learns in one game that a red band means
 * stop and look.
 *
 * Six tones, and the order they are decided in is the whole of the rule — a finished game
 * outranks everything, a destructive question outranks a submission in flight, and a submission
 * in flight outranks an action list, because the player has already answered and what they need
 * to know is that their click is out there.
 *
 * **Every tone carries wording.** A colour a player has not learnt yet says nothing, a colour
 * nobody can distinguish says nothing, and a colour is invisible to a screen reader. The band
 * says it in words and draws it in colour; neither is the only copy.
 *
 * Nothing here reads the game. The inputs are the action list the server sent, the interaction
 * this client is holding, and whether a result arrived — the same three things the dock already
 * renders from.
 *
 * The rest of this module is `docs/client-design.md` §6.5, which gave the dock's *contents* the
 * ladder every region got in §3 — and it is the same shape as `fit.ts`: pure arithmetic over the
 * box a region was handed, so what a question costs in height is testable without a browser.
 *
 * - `dockNarrates` is §6.5 rule 2. A band that is asking looks like it is asking; when the
 *   controls below it already state the question, the sentence above them is the same fact
 *   written twice (§2.1 rule 5) and it is what pushed the buttons out of the band.
 * - `dockCandidates` is §6.5's "the board answers". A question about objects on the screen is
 *   answered on the screen, so the dock carries only the subjects no surface drew — which is the
 *   whole of why a question with twenty answers is the size of a question with two.
 * - `dockDensity` is "scale first, remove last" (§3): the dock's band is `scene()`'s and can be
 *   as little as 44px, so the contents scale into it rather than anything being dropped from it.
 */
import {
  focus,
  globalActions,
  needsConfirmation,
  owedActions,
  type Interaction,
  type Slot,
} from './interaction'
import type { GameResult, ValidAction } from './protocol'

/**
 * The tone the action bar is tinted in: **where in the turn you are**, never how urgent the ask
 * is (`docs/client-design.md` §6.5).
 *
 * Green for the turn's bookends, blue while you may cast at will, red once combat is live and the
 * choice costs something. A step this build does not recognise is green, because the bookends are
 * where an unknown step most likely belongs and a wrong red is the expensive direction to be
 * wrong in.
 *
 * The distinction the tone used to carry — what the controls are currently *for* — is still drawn
 * and still `dockTone` below; it is drawn in the **words**. What colour is for is telling a player
 * at a glance that the game has moved somewhere different, and tying it to the turn means the bar
 * changes when the situation does rather than flickering between two shades of "asking" inside one
 * step.
 */
export type BarTone = 'green' | 'blue' | 'red'

const BAR_TONES: Record<string, BarTone> = {
  precombat_main: 'blue',
  postcombat_main: 'blue',
  begin_combat: 'red',
  declare_attackers: 'red',
  declare_blockers: 'red',
  combat_damage: 'red',
  end_combat: 'red',
}

export const barTone = (phase: string | undefined): BarTone =>
  (phase === undefined ? undefined : BAR_TONES[phase]) ?? 'green'

export type DockTone = 'over' | 'confirm' | 'sent' | 'asking' | 'yours' | 'waiting'

export function dockTone(
  actions: readonly ValidAction[],
  interaction: Interaction,
  result: GameResult | undefined,
): DockTone {
  if (result) return 'over'
  // Asked before it is sent, and it ends the match — the one state that should stop a player
  // who is clicking through by reflex.
  if (interaction.confirming) return 'confirm'
  if (interaction.pending) return 'sent'
  // Either the player armed something that asks questions, or the server is offering no pass,
  // which is its way of saying play does not continue until this seat answers (`interaction.ts`).
  if (focus(actions, interaction).action || owedActions(actions).length > 0) return 'asking'
  return actions.length > 0 ? 'yours' : 'waiting'
}

/**
 * The tone as a sentence.
 *
 * Deliberately about *the player's obligation* rather than about the game's state: "waiting for
 * the other seat" and "your move" are the two things a player checks for, and a band that said
 * "precombat main" instead would be repeating the step that is already printed beside it.
 */
export function dockWording(tone: DockTone): string {
  switch (tone) {
    case 'over':
      return 'The game is over'
    case 'confirm':
      return 'Confirm below — this cannot be undone'
    case 'sent':
      return 'Sent — waiting for the server'
    case 'asking':
      return 'The game is waiting on your answer'
    case 'yours':
      return 'Your move'
    case 'waiting':
      return 'Waiting for the other seat'
  }
}

/**
 * Whether the band still has to say what the game wants, or the controls already do.
 *
 * `docs/client-design.md` §6.5 rule 2 — *the tone is shown, not written* — and §2.1 rule 5.
 * Asking a player to keep or mulligan used to state the same fact four times: this sentence, a
 * heading, a legend, and the two buttons. Only the buttons are acted on, and together the other
 * three pushed the controls out of a band that is 44px on half the viewports this client
 * supports.
 *
 * So the sentence is drawn exactly where nothing else is stating the question: while an action
 * is being drafted its own controls are the question, and a confirmation asks in as many words.
 * Every other state — your move, waiting, sent, over — has no controls that say what the game
 * wants, so the band says it.
 *
 * A finished game keeps its sentence whatever else is open, for the same reason `dockTone` ranks
 * it first: nobody is coming, and a screen that will never change has to say so.
 */
export function dockNarrates(
  actions: readonly ValidAction[],
  interaction: Interaction,
  result: GameResult | undefined,
): boolean {
  if (result) return true
  if (interaction.confirming !== undefined) return false
  return focus(actions, interaction).action === undefined
}

/**
 * The buttons the bar carries, in the order it draws them.
 *
 * Two kinds, and the second is the one that was missing. **Global actions** own no object and
 * have nowhere on the table to be clicked — passing, a mulligan decision — so the bar is where
 * they live. **Owed actions** do own an object and still belong here: `owedActions` is the
 * server declining to offer this seat a `pass_priority`, which is its way of saying play does
 * not continue until this seat answers. A triggered ability waiting to be aimed is exactly that
 * shape — it is bound to its own stack entry and to the permanent whose ability it is — so it
 * drew no button, and the band said *the game is waiting on your answer* above nothing that
 * would answer it. The only way in was to guess which card to click.
 *
 * Owed actions come last, so the one the game is actually waiting for is the primary control at
 * the end of the row. Anything that is both is drawn once. An action that ends the match is not
 * here at all: it lives in the side panel, away from the button a player presses by reflex.
 *
 * Nothing here reads the game. Which actions are global, which are owed, and which end a match
 * are all the server's own statements (`interaction.ts`).
 */
export function barActions(actions: readonly ValidAction[]): readonly ValidAction[] {
  const ordinary = (action: ValidAction) => !needsConfirmation(action)
  const globals = globalActions(actions).filter(ordinary)
  const owed = owedActions(actions).filter(
    (action) => ordinary(action) && !globals.includes(action),
  )
  return [...globals, ...owed]
}

/**
 * The subjects of one slot the dock still has to carry, because no surface drew them.
 *
 * §6.5: *a question about objects on the screen is answered on the screen.* The objects a slot
 * will accept are highlighted where they lie and clicking one answers it (`interaction.ts`), so
 * listing them in the dock as well is the second copy of a control — and it is the copy that
 * grows without bound, because a board with twenty legal blockers has twenty of them.
 *
 * **The fallback is not optional.** A card in a face-down pile, a card in no rendered zone at
 * all, an ability with no permanent: those have nothing to click, and they stay here as controls
 * so that no action is ever reachable only by finding its object. `drawn` is a fact about this
 * client's own rendering and nothing else — which ids it put a box on — never about the game.
 *
 * An ordering is the exception and it is not a policy choice: a permutation's answer is *where*
 * each item sits in it, only a control can carry that, and a board that showed the items without
 * their positions would be showing half an answer. So an `order` slot keeps every one of its
 * items here, and the count it can reach is the count the server asked to be ordered.
 */
export function dockCandidates(slot: Slot, drawn: ReadonlySet<string>): readonly string[] {
  if (!slot.byEntity) return []
  if (slot.kind === 'order') return slot.candidates
  return slot.candidates.filter((id) => !drawn.has(id))
}

/**
 * How much of its designed drawing the dock's own band can afford.
 *
 * `docs/client-design.md` §3, "Scale first. Remove last." — the first answer to *it does not fit*
 * is always to make it smaller. The dock's box is `scene()`'s and is a function of the viewport
 * alone; on half the supported viewports it is the 44px floor even while the game is asking, and
 * a question drawn at desktop type in 44px is a question with its buttons cut off.
 *
 * One scale for everything in the band (§7): type, the padding on a control, and the space
 * between controls all move together, so no element can clip independently of its neighbours.
 * The type floor is §7's 11px for chrome and is never crossed.
 *
 * Monotone by construction — every field is non-decreasing in `height`, and clamped at both ends
 * — because the recurring defect in this client is a bigger screen drawing a worse board (§3,
 * "More screen is never a worse board"). Nothing here reads the content: a band that responded to
 * how much there is to ask about is the defect §5 forbids, wearing a smaller hat.
 */
export interface DockDensity {
  /** Type size for everything the dock draws, in px. */
  text: number
  /** Vertical padding on one control. */
  padY: number
  /** Horizontal padding on one control. */
  padX: number
  /** Space between two controls sharing a row. */
  gap: number
  /** Space between two rows, and the dock's own block padding. */
  rowGap: number
}

/**
 * The two ends of the ladder.
 *
 * `TIGHT` is what `scene.ts` hands the dock at its floor — `DOCK.min`, 44px — and every number in
 * it is the smallest the spec allows: 11px is §7's chrome floor, and the padding is what keeps a
 * button a button. `ROOMY` is the drawing at `DOCK_ASKING`, 160px, which is what a wide screen
 * gives a question.
 */
const TIGHT: DockDensity = { text: 11, padY: 1, padX: 6, gap: 7, rowGap: 2 }
const ROOMY: DockDensity = { text: 14, padY: 4, padX: 13, gap: 18, rowGap: 5 }

/** The band's floor and the band it grows to when the game is asking — `scene.ts`'s two numbers. */
const DOCK_FLOOR = 44
const DOCK_ASKING = 160

export function dockDensity(height: number): DockDensity {
  const t = Math.max(0, Math.min(1, (height - DOCK_FLOOR) / (DOCK_ASKING - DOCK_FLOOR)))
  const at = (from: number, to: number) => Math.round((from + (to - from) * t) * 100) / 100
  return {
    text: at(TIGHT.text, ROOMY.text),
    padY: at(TIGHT.padY, ROOMY.padY),
    padX: at(TIGHT.padX, ROOMY.padX),
    gap: at(TIGHT.gap, ROOMY.gap),
    rowGap: at(TIGHT.rowGap, ROOMY.rowGap),
  }
}
