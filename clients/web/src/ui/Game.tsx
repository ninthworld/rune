/**
 * The game screen: one `GameView`, rendered.
 *
 * Everything below is read straight off the view. Nothing is remembered between messages, so a
 * refresh mid-game produces this same screen from the next frame the server sends.
 *
 * The settle notice is the point of the exercise, not decoration. The server advances the game
 * on your behalf through steps where you had nothing to do, which means one message can cover a
 * whole turn; without saying where it acted, a player watches the game move and cannot tell
 * what they missed.
 */
import type { ClientMessage, GameLogEvent, GameView } from './../protocol'
import { controlledBy, list, playerLabel, powerToughness, seatSummary } from './../normalize'
import { ActionPanel } from './ActionPanel'

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

// An unknown classifier is rendered generically rather than guessed at (`docs/protocol.md`).
const phaseLabel = (phase: string): string => PHASE_LABELS[phase] ?? phase

export function Game({ view, send }: { view: GameView; send(message: ClientMessage): void }) {
  const you = view.you ?? ''
  const label = (id: string) => playerLabel(view, id)

  // Names for entity ids the action panel offers. The server labels players; cards and
  // permanents are named from the view's own contents rather than resolved by the client.
  const names = new Map<string, string>()
  for (const card of list(view.my_hand)) names.set(card.id, card.name)
  for (const permanent of list(view.battlefield)) names.set(permanent.id, permanent.card.name)
  for (const item of list(view.stack)) names.set(item.id, item.description)
  for (const pile of [...list(view.graveyards), ...list(view.exile), ...list(view.command)]) {
    for (const card of pile.cards) names.set(card.id, card.name)
  }
  for (const seat of list(view.seat_order)) names.set(seat, label(seat))
  const labelFor = (id: string) => names.get(id) ?? id

  return (
    <div className="game">
      <header>
        <h1>
          Turn {view.turn ?? 0} — {phaseLabel(view.phase)}
        </h1>
        <p>
          Active: {label(view.active_player ?? '')}
          {view.priority_player !== undefined && <> · Priority: {label(view.priority_player)}</>}
          {view.action_deadline !== undefined && <> · {view.action_deadline}s to decide</>}
        </p>
        {view.format?.commander && <p>Commander game</p>}
      </header>

      {view.action_rejected && (
        <p role="status" className="notice">
          That action could not be taken. This is the current state.
        </p>
      )}

      {list(view.auto_passed_steps).length > 0 && (
        <section className="notice" aria-labelledby="settle-heading">
          <h2 id="settle-heading">Passed for you</h2>
          {/* A path, not a set: a genuinely revisited position appears twice, and each entry
              carries its own turn because an extra combat or cleanup phase revisits a step
              within one turn. Collapsing either would assert game structure the server did
              not state. */}
          <ol>
            {list(view.auto_passed_steps).map((step, index) => (
              <li key={`${step.turn}-${step.phase}-${index}`}>
                Turn {step.turn} — {phaseLabel(step.phase)}
              </li>
            ))}
          </ol>
        </section>
      )}

      {view.result && (
        <section className="notice" aria-labelledby="result-heading">
          <h2 id="result-heading">Game over</h2>
          <p>
            {view.result.winner ? `${label(view.result.winner)} wins` : 'No winner'} ·{' '}
            {view.result.reason}
          </p>
        </section>
      )}

      <section aria-labelledby="seats-heading">
        <h2 id="seats-heading">Seats</h2>
        <ul>
          {view.me && (
            <li>
              <strong>{label(you)} (you)</strong> — {seatSummary(view.me)}
              {list(view.mana_pool).length > 0 && <> · pool {list(view.mana_pool).join(' ')}</>}
            </li>
          )}
          {list(view.opponents).map((opponent) => (
            <li key={opponent.player_id}>
              {label(opponent.player_id)} — {seatSummary(opponent)}
            </li>
          ))}
        </ul>
      </section>

      <section aria-labelledby="stack-heading">
        <h2 id="stack-heading">Stack</h2>
        {list(view.stack).length === 0 ? (
          <p>Empty.</p>
        ) : (
          // Bottom first on the wire; the top of the stack resolves first, so it reads last.
          <ol>
            {list(view.stack).map((item) => (
              <li key={item.id}>
                {item.description} — {label(item.controller)}
                {item.kind && <> ({item.kind})</>}
                {list(item.targets).length > 0 && (
                  <>
                    {' '}
                    →{' '}
                    {list(item.targets)
                      .map((t) => ('id' in t ? labelFor(t.id) : label(t.player)))
                      .join(', ')}
                  </>
                )}
              </li>
            ))}
          </ol>
        )}
      </section>

      <section aria-labelledby="battlefield-heading">
        <h2 id="battlefield-heading">Battlefield</h2>
        {list(view.seat_order).length === 0 && list(view.battlefield).length === 0 && <p>Empty.</p>}
        {(list(view.seat_order).length > 0 ? list(view.seat_order) : [you]).map((seat) => (
          <div key={seat}>
            <h3>{label(seat)}</h3>
            {controlledBy(view, seat).length === 0 ? (
              <p>No permanents.</p>
            ) : (
              <ul>
                {controlledBy(view, seat).map((permanent) => (
                  <li key={permanent.id}>
                    {permanent.card.name}
                    {powerToughness(permanent.card) && <> {powerToughness(permanent.card)}</>}
                    {permanent.tapped && ' · tapped'}
                    {permanent.attacking && ' · attacking'}
                    {permanent.blocking && ` · blocking ${labelFor(permanent.blocking)}`}
                    {permanent.damage !== undefined && permanent.damage > 0 && (
                      <> · {permanent.damage} damage</>
                    )}
                    {list(permanent.counters).map((counter) => (
                      <span key={counter.kind}>
                        {' '}
                        · {counter.count}× {counter.kind}
                      </span>
                    ))}
                    {permanent.is_commander && ' · commander'}
                  </li>
                ))}
              </ul>
            )}
          </div>
        ))}
      </section>

      <section aria-labelledby="hand-heading">
        <h2 id="hand-heading">Your hand</h2>
        {list(view.my_hand).length === 0 ? (
          <p>Empty.</p>
        ) : (
          <ul>
            {list(view.my_hand).map((card) => (
              <li key={card.id}>
                {card.name} — {card.type_line}
                {card.mana_cost && <> {card.mana_cost}</>}
                {powerToughness(card) && <> {powerToughness(card)}</>}
                {card.rules_text && <> — {card.rules_text}</>}
              </li>
            ))}
          </ul>
        )}
      </section>

      {list(view.valid_actions).length > 0 ? (
        <ActionPanel actions={list(view.valid_actions)} labelFor={labelFor} send={send} />
      ) : (
        <section aria-labelledby="waiting-heading">
          <h2 id="waiting-heading">Actions</h2>
          {/* A finished game is not a game that is waiting: nobody is coming, and saying
              otherwise leaves a player watching a screen that will never change. */}
          <p>{view.result ? 'Nothing to do — the game is over.' : 'Waiting for the other seat.'}</p>
        </section>
      )}

      <section aria-labelledby="log-heading">
        <h2 id="log-heading">Log</h2>
        {list(view.log).length === 0 ? (
          <p>Nothing yet.</p>
        ) : (
          <ol className="log">
            {/* Newest last, as the server ordered it; the window is bounded server-side. */}
            {list(view.log).map((entry) => (
              <li key={entry.sequence}>{describe(entry.event, label)}</li>
            ))}
          </ol>
        )}
      </section>
    </div>
  )
}

/**
 * Compose readable text for one log event.
 *
 * Events carry typed references and data, never pre-rendered prose, so the wording is the
 * client's — which is why an unrecognized event kind renders generically rather than being
 * dropped or guessed at.
 */
function describe(event: GameLogEvent, playerName: (id: string) => string): string {
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
      // A newer server may log something this client has no wording for. Say so, rather than
      // dropping the entry or inventing a description for it.
      return '(unrecognized log event)'
  }
}
