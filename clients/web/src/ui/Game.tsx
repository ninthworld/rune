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
import { useState } from 'react'

import type { ClientMessage, GameLogEvent, GameView } from './../protocol'
import { controlledBy, list, playerLabel, seatSummary } from './../normalize'
import {
  cardFace,
  emblemFace,
  permanentFace,
  stackFace,
  type CardFace,
  type CardFaceState,
} from './../card-face'
import { ActionPanel } from './ActionPanel'
import { Card } from './Card'
import { CardInspector } from './CardInspector'

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

  // The inspector remembers an **id**, never a face. Faces are rebuilt from whatever view
  // arrived last, so an open inspector shows the object as it is now — and an object that has
  // left the view closes it rather than pinning a card that no longer exists. Nothing here is
  // load-bearing across messages: a refresh reproduces the board, minus an open panel.
  const [inspecting, setInspecting] = useState<string | undefined>(undefined)

  // One face per card-shaped object on screen, built once and shared by every surface, so the
  // hand, the board, and the inspector cannot disagree about the same object.
  const handFaces = list(view.my_hand).map(cardFace)
  const revealedFaces = list(view.revealed).map(cardFace)
  // Paired with their source object where the surface renders something the face does not
  // carry — a controller, a target list, a combat relationship.
  const stackEntries = list(view.stack).map((item) => ({ item, face: stackFace(item) }))
  const emblemEntries = list(view.emblems).map((emblem) => ({ emblem, face: emblemFace(emblem) }))
  const permanentEntries = list(view.battlefield).map((permanent) => ({
    permanent,
    face: permanentFace(permanent),
    controller: permanent.controller,
  }))
  const faces = new Map<string, CardFace>()
  for (const face of [
    ...handFaces,
    ...revealedFaces,
    ...stackEntries.map((entry) => entry.face),
    ...emblemEntries.map((entry) => entry.face),
    ...permanentEntries.map((entry) => entry.face),
  ]) {
    faces.set(face.id, face)
  }
  const inspected = inspecting === undefined ? undefined : faces.get(inspecting)
  const inspect = (face: CardFace) => setInspecting(face.id)

  // Every object the server named in an offered action or one of its target slots. This is a
  // reading of `valid_actions`, not a judgement about it: the client marks what the server
  // pointed at and works out nothing about why. Acting on one of these is the next surface's
  // job (#626); showing which objects are in play at all is this one's.
  const named = new Set<string>()
  for (const action of list(view.valid_actions)) {
    for (const id of list(action.subject)) named.add(id)
    for (const requirement of list(action.requirements)) {
      for (const id of list(requirement.candidates)) named.add(id)
    }
  }
  const stateOf = (face: CardFace): CardFaceState => (named.has(face.id) ? 'candidate' : 'idle')

  // Names for entity ids the action panel offers. The server labels players; cards and
  // permanents are named from the view's own contents rather than resolved by the client.
  const names = new Map<string, string>()
  for (const card of list(view.my_hand)) names.set(card.id, card.name)
  // Cards from a hidden zone this seat is being shown right now (a search, a look at
  // the top, an opponent's hand). Without these the choice prompt would ask about ids
  // the client can put no name to.
  for (const card of list(view.revealed)) names.set(card.id, card.name)
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
          <ol className="cards cards--stack">
            {stackEntries.map(({ item, face }) => (
              <li key={item.id}>
                <Card face={face} variant="stack" state={stateOf(face)} onInspect={inspect} />
                <p className="cards__aside">
                  {/* The server composes a description for the stack object itself, which is
                      not always the card's name — "Counterspell targeting Twin Bolt" says
                      something the face does not. Kept only when it adds something: for many
                      spells it is just the name, or verbatim the rules text already above. */}
                  {item.description !== face.name && item.description !== face.rulesText && (
                    <>{item.description} — </>
                  )}
                  {label(item.controller)}
                  {list(item.targets).length > 0 && (
                    <>
                      {' '}
                      →{' '}
                      {list(item.targets)
                        .map((t) => ('id' in t ? labelFor(t.id) : label(t.player)))
                        .join(', ')}
                    </>
                  )}
                </p>
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
            {controlledBy(permanentEntries, seat).length === 0 ? (
              <p>No permanents.</p>
            ) : (
              <ul className="cards cards--battlefield">
                {controlledBy(permanentEntries, seat).map(({ permanent, face }) => (
                  <li key={permanent.id}>
                    <Card
                      face={face}
                      variant="battlefield"
                      state={stateOf(face)}
                      onInspect={inspect}
                    />
                    {/* Combat and attachment are relationships *between* objects rather than
                        facts about one, so they stay beside the face as text until the table
                        can draw them. */}
                    {(permanent.attacking || permanent.blocking || permanent.attached_to) && (
                      <p className="cards__aside">
                        {permanent.attacking &&
                          (permanent.attacking_planeswalker !== undefined
                            ? `attacking ${labelFor(permanent.attacking_planeswalker)}`
                            : 'attacking')}
                        {permanent.blocking && ` blocking ${labelFor(permanent.blocking)}`}
                        {permanent.attached_to && ` attached to ${labelFor(permanent.attached_to)}`}
                      </p>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>
        ))}
      </section>

      {list(view.emblems).length > 0 && (
        <section aria-labelledby="emblems-heading">
          <h2 id="emblems-heading">Emblems</h2>
          {/* An emblem (CR 114) is in no zone and is never removed, so it sits beside the
              battlefield rather than in it. Its abilities arrive as server-composed
              sentences; the client renders them and computes nothing. */}
          <ul className="cards cards--emblems">
            {emblemEntries.map(({ emblem, face }) => (
              <li key={emblem.id}>
                {/* An emblem has no cost, no type line, and no printed face, so there is no
                    smaller variant to clamp it into — it renders at full size or it renders
                    a truncated sentence nobody can act on. */}
                <Card face={face} variant="inspect" onInspect={inspect} />
                <p className="cards__aside">{label(emblem.controller)}</p>
              </li>
            ))}
          </ul>
        </section>
      )}

      {list(view.revealed).length > 0 && (
        <section aria-labelledby="revealed-heading">
          <h2 id="revealed-heading">Shown to you</h2>
          {/* Only this seat receives these; the server decides that, and sends them to
              nobody else. Rendered beside the hand so the choice prompt below has
              something legible to refer to. */}
          <ul className="cards cards--compact">
            {revealedFaces.map((face) => (
              <li key={face.id}>
                <Card face={face} variant="compact" state={stateOf(face)} onInspect={inspect} />
              </li>
            ))}
          </ul>
        </section>
      )}

      <section aria-labelledby="hand-heading">
        <h2 id="hand-heading">Your hand</h2>
        {list(view.my_hand).length === 0 ? (
          <p>Empty.</p>
        ) : (
          <ul className="cards cards--hand">
            {handFaces.map((face) => (
              <li key={face.id}>
                <Card face={face} variant="hand" state={stateOf(face)} onInspect={inspect} />
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

      {/* Last in the tree so it layers over the board without any surface below it needing to
          know it exists. `inspected` is looked up in this frame's faces, so an object that has
          left the view simply stops resolving and the panel closes itself. */}
      {inspected && <CardInspector face={inspected} onClose={() => setInspecting(undefined)} />}
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
      // A newer server may log something this client has no wording for. Say so, rather than
      // dropping the entry or inventing a description for it.
      return '(unrecognized log event)'
  }
}
