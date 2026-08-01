/**
 * Wording for the game log.
 *
 * Log events carry typed references and data, never pre-rendered prose, so every sentence here
 * is the client's. That is deliberate — the server states what happened, and how it reads is a
 * presentation decision — but it means an event kind this build has never heard of has no
 * wording at all. Such an entry says so, rather than being dropped or guessed at: a log with a
 * silent hole in it is worse than one that admits to a gap.
 */
import type { GameLogEvent } from './protocol'
import { phaseLabel } from './table'

export function describe(event: GameLogEvent, playerName: (id: string) => string): string {
  switch (event.type) {
    case 'spell_cast':
      return `${playerName(event.player)} casts ${event.card.name}`
    case 'spell_resolved':
      return `${event.card.name} resolves`
    case 'spell_countered':
      return `${event.card.name} is countered`
    case 'spell_fizzled':
      return `${event.card.name} fizzles — no legal target`
    case 'attackers_declared':
      return `${playerName(event.player)} attacks with ${event.attackers.map((a) => a.name).join(', ') || 'nobody'}`
    case 'blockers_declared':
      return `${playerName(event.player)} blocks ${event.blocks.map((b) => `${b.attacker.name} with ${b.blocker.name}`).join(', ') || 'nothing'}`
    case 'mulligan':
      return `${playerName(event.player)} mulligans`
    case 'hand_kept':
      return `${playerName(event.player)} keeps their hand`
    case 'life_changed':
      return `${playerName(event.player)} ${event.amount >= 0 ? 'gains' : 'loses'} ${Math.abs(event.amount)} life`
    case 'damage_dealt':
      return event.target.kind === 'player'
        ? `${playerName(event.target.player)} takes ${event.amount} damage`
        : `${event.target.permanent.name} takes ${event.amount} damage`
    case 'cards_drawn':
      return `${playerName(event.player)} draws ${event.count}`
    case 'cards_milled':
      return `${playerName(event.player)} mills ${event.count}`
    case 'cards_discarded':
      return `${playerName(event.player)} discards ${event.count}`
    case 'library_searched':
      return `${playerName(event.player)} searches their library and shuffles`
    case 'optional_applied':
      return `${playerName(event.player)} takes an optional effect`
    case 'optional_declined':
      return `${playerName(event.player)} declines an optional effect`
    case 'permanent_died':
      return `${event.permanent.name} dies`
    case 'step_changed':
      return `— Turn ${event.turn}, ${phaseLabel(event.phase)} (${playerName(event.active_player)})`
    case 'player_eliminated':
      return `${playerName(event.player)} is eliminated (${event.reason})`
    case 'commander_returned_to_command_zone':
      return `${event.card.name} returns to the command zone`
    case 'game_over':
      return `Game over — ${event.result.winner ? `${playerName(event.result.winner)} wins` : 'no winner'} (${event.result.reason})`
    default:
      // A newer server may log something this client has no wording for.
      return '(unrecognized log event)'
  }
}
