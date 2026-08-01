/**
 * What the controls are currently for, in one word.
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
 */
import { focus, owedActions, type Interaction } from './interaction'
import type { GameResult, ValidAction } from './protocol'

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
