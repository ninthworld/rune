/**
 * The game screen: one `GameView`, arranged as a table.
 *
 * This file composes and derives; it draws almost nothing. Everything visible is a surface in
 * `./game/`, and everything those surfaces need is worked out once here — the faces, what the
 * server named, what an id is called — so no surface holds a second reading of the view.
 *
 * The composition is fixed and two-player by design: opponent across from you, your side
 * nearest you, the stack between them, your hand along the bottom edge, and the controls that
 * move the game pinned below that. A permanent's controller is answered by *where the card is*
 * rather than by a heading above a list, which is the difference between a table and a state
 * dump. Three to six seats are a different composition and are not this one.
 *
 * It also owns the one piece of interaction state, and routes every click through
 * `interaction.ts` so the rule is written once. What it holds is presentation and nothing else:
 * what is selected, what is being drafted, and which submission is still unanswered. A new view
 * throws all of it away — `settle` — because a draft assembled against the previous view names
 * slots and ids this one may no longer offer. The submission is the single thing that crosses
 * that boundary, and it is the server that closes it, by echoing the id back in `action_ack`.
 *
 * A refresh mid-game therefore produces this same screen from the next frame the server sends,
 * minus an open inspector and minus a click in flight.
 *
 * The transport is the second thing that crosses a message boundary, and it crosses in one
 * direction only: a new socket ends every correlation this screen was holding, because the server
 * drops a seat's `action_ack` when it reconnects. That is the whole of what reconnection means
 * here — the view is replaced by the server's, and the wait for an answer that is no longer
 * coming is dropped.
 */
import { useState } from 'react'

import type { ClientMessage, GameView, Phase, ValidAction } from './../protocol'
import { list, playerLabel } from './../normalize'
import { seats, type SeatPile } from './../table'
import { withStop, type StopScope } from './../turn'
import type { ConnectionStatus } from './../socket'
import {
  cardFace,
  emblemFace,
  permanentFace,
  stackFace,
  type CardFace,
  type CardFaceLink,
} from './../card-face'
import { relationLines, relations } from './../relations'
import {
  IDLE,
  arm,
  ask,
  fill,
  focus,
  gestureFor,
  highlightFor,
  needsChoices,
  needsConfirmation,
  release,
  select,
  settle,
  submitted,
  type Interaction,
} from './../interaction'
import { buildChooseAction, type Draft } from './../submission'
import { CardInspector } from './CardInspector'
import { ActionDock } from './game/ActionDock'
import { Battlefield, type FieldEntry } from './game/Battlefield'
import { Hand } from './game/Hand'
import { MatchHeader } from './game/MatchHeader'
import { MatchResult } from './game/MatchResult'
import { PlayerPanel } from './game/PlayerPanel'
import { SidePanel, type OpenZone } from './game/SidePanel'
import { StackRail } from './game/StackRail'
import type { Surface } from './game/surface'

// Opaque and client-generated: the server echoes it back verbatim and derives nothing from it.
let submissionCounter = 0
const nextSubmissionId = (): string => `s:${++submissionCounter}`

export function Game({
  view,
  connection,
  epoch,
  send,
  leave,
}: {
  view: GameView
  connection: ConnectionStatus
  /** How many sockets this tab has opened; a change means everything in flight was lost. */
  epoch: number
  send(message: ClientMessage): void
  leave(): void
}) {
  const label = (id: string) => playerLabel(view, id)

  // The inspector remembers an **id**, never a face. Faces are rebuilt from whatever view
  // arrived last, so an open inspector shows the object as it is now — and an object that has
  // left the view closes it rather than pinning a card that no longer exists.
  const [inspecting, setInspecting] = useState<string | undefined>(undefined)
  // The pile the player is looking through, held the same way and for the same reason: a seat
  // and a zone, never the cards. A pile that empties, or a seat that leaves, stops resolving
  // and the browser closes rather than showing a graveyard the game no longer has.
  const [browsing, setBrowsing] = useState<{ seat: string; zone: SeatPile['zone'] } | undefined>(
    undefined,
  )
  // The object the player is looking at. Transient to the point of being disposable — it is not
  // settled against a new view, because whatever the pointer is over is still under the pointer
  // when the next frame paints, and an id that has left the view simply relates to nothing.
  const [hovering, setHovering] = useState<string | undefined>(undefined)
  const [interaction, setInteraction] = useState<Interaction>(IDLE)
  // Whether the player has pushed the result panel aside to read the final board. Purely
  // presentation, and safe to hold across views: a game ends once, and the header keeps saying
  // so for as long as the view does.
  const [dismissed, setDismissed] = useState(false)

  // Settled during render rather than in an effect, so the frame that carries a new view is
  // never painted with the previous view's draft still in the dock.
  const [seen, setSeen] = useState(view)
  if (seen !== view) {
    setSeen(view)
    // Updated from the current value rather than the one this render closed over, so a view and
    // a reconnect landing together compose instead of one overwriting the other.
    setInteraction((current) => settle(current, view.action_ack))
  }

  // A new socket is the end of every correlation this client was holding: the server drops a
  // seat's `action_ack` when it reconnects (`docs/protocol.md`), so an ack that would have
  // answered the click in flight is never coming. Waiting on it forever would leave the dock
  // blocked on a reply that no longer exists, so the wait is released and the fresh view the
  // reconnect brings is the answer to what actually happened.
  const [seenEpoch, setSeenEpoch] = useState(epoch)
  if (seenEpoch !== epoch) {
    setSeenEpoch(epoch)
    setInteraction(release)
  }

  const actions = list(view.valid_actions)
  const table = seats(view)
  // Combat, attachments, targets, and sources, joined once from the identifiers the server
  // stated. Both the trails under the cards and the emphasis on a focused subset read from this
  // one graph, so the board and the stack cannot describe the same edge differently.
  const related = relations(view)
  const handFaces = list(view.my_hand).map(cardFace)
  const revealedFaces = list(view.revealed).map(cardFace)
  const stackEntries = list(view.stack).map((item) => ({
    item,
    face: stackFace(item),
    lines: relationLines(related, item.id),
  }))
  const emblemEntries = list(view.emblems).map((emblem) => ({ emblem, face: emblemFace(emblem) }))
  const fieldEntries: readonly FieldEntry[] = list(view.battlefield).map((permanent) => ({
    permanent,
    face: permanentFace(permanent),
    lines: relationLines(related, permanent.id),
  }))

  // One face per card-shaped object on screen, so the hand, the board, the piles, and the
  // inspector cannot disagree about the same object.
  const faces = new Map<string, CardFace>()
  for (const face of [
    ...handFaces,
    ...revealedFaces,
    ...stackEntries.map((entry) => entry.face),
    ...emblemEntries.map((entry) => entry.face),
    ...fieldEntries.map((entry) => entry.face),
    ...table.flatMap((seat) => seat.piles.flatMap((pile) => pile.faces)),
  ]) {
    faces.set(face.id, face)
  }
  const inspected = inspecting === undefined ? undefined : faces.get(inspecting)

  // Names for entity ids the surfaces and the dock mention. The server labels players; cards and
  // permanents are named from the view's own contents, never resolved client-side.
  const names = new Map<string, string>()
  for (const face of faces.values()) names.set(face.id, face.name)
  // The stack's own description, not its card's name: a target on the stack reads better as
  // "Counterspell targeting Twin Bolt" than as "Counterspell".
  for (const item of list(view.stack)) names.set(item.id, item.description)
  for (const seat of table) names.set(seat.id, seat.name)

  /** Send one action, and start waiting on the answer. */
  const dispatch = (action: ValidAction, draft: Draft) => {
    const submission = nextSubmissionId()
    send(buildChooseAction(action, draft, submission))
    // The draft is spent. What is kept is the selection, so the table does not jump, and the
    // submission, so a second click cannot send the same thing twice.
    setInteraction(
      submitted(
        { draft: {}, selected: interaction.selected },
        { submission, actionId: action.id, label: action.label },
      ),
    )
  }

  /**
   * Start an action: one click when it asks nothing, a draft when it does, two when it ends the
   * game. Reaching an action that wants confirming *while it is the one being confirmed* is the
   * second click, which is why the same handler both asks and answers.
   */
  const take = (action: ValidAction) => {
    if (interaction.pending) return
    if (needsConfirmation(action) && interaction.confirming !== action.id) {
      setInteraction(ask(interaction, action))
      return
    }
    if (needsChoices(action)) {
      setInteraction(arm(interaction, action))
      return
    }
    dispatch(action, {})
  }

  /** Set where the game will hand this seat priority. Server-stored, so nothing is kept here. */
  const setStop = (phase: Phase, scope: StopScope) => send(withStop(view, phase, scope))

  const confirm = () => {
    const current = focus(actions, interaction)
    if (!current.action || !current.ready || interaction.pending) return
    dispatch(current.action, interaction.draft)
  }

  // Tracing follows the look, and falls back to the click. Hovering or tabbing to an object
  // emphasises everything the server related it to, which costs nothing and reaches the objects
  // that most need it — a blocker or an enchanted creature usually owns no action, so a click on
  // it opens the inspector over the very board the relationship crosses. Selection keeps the
  // trace alive once the player's pointer has moved off to the dock.
  //
  // An object with no relationships traces nothing at all, because emphasising one card and no
  // others would promise a connection that is not there.
  const traced = hovering ?? interaction.selected
  const linked = traced === undefined ? new Set<string>() : related.linked(traced)
  const linkOf = (id: string): CardFaceLink =>
    linked.size === 0 ? undefined : id === traced ? 'focus' : linked.has(id) ? 'linked' : undefined

  const surface: Surface = {
    stateOf: (id: string) => highlightFor(actions, interaction, id),
    linkOf,
    trace: setHovering,
    activate: (id: string) => {
      const gesture = gestureFor(actions, interaction, id)
      if (gesture.kind === 'inspect') {
        setInspecting(id)
        return
      }
      if (gesture.kind === 'select') {
        setInteraction(select(interaction, id))
        return
      }
      const slot = focus(actions, interaction).slots.find((each) => each.slot === gesture.slot)
      if (slot) setInteraction(fill(interaction, slot, id))
    },
    labelFor: (id: string) => names.get(id) ?? id,
  }

  // Opponents across the table, you nearest. With no seat the server named as yours, everyone
  // renders as an opponent rather than one of them being promoted into your chair.
  const opponents = table.filter((seat) => !seat.isYou)
  const local = table.find((seat) => seat.isYou)
  const fieldFor = (id: string) => fieldEntries.filter((entry) => entry.permanent.controller === id)

  // Resolved out of this frame's seats rather than remembered as cards, so an opened pile shows
  // what is in it now. A pile the view no longer carries simply stops resolving, which closes
  // the browser — the same rule the inspector follows.
  const openSeat = table.find((seat) => seat.id === browsing?.seat)
  const openPile = openSeat?.piles.find((pile) => pile.zone === browsing?.zone)
  const openZone: OpenZone | undefined =
    openSeat && openPile
      ? { label: openPile.label, note: openSeat.name, faces: openPile.faces }
      : undefined
  const browse = (seat: string) => (zone: SeatPile['zone']) =>
    setBrowsing((current) =>
      // Clicking the open pile's own button closes it, so the control that opened it is also
      // the way out and the column does not need a second one.
      current?.seat === seat && current.zone === zone ? undefined : { seat, zone },
    )

  return (
    <div className="screen">
      <MatchHeader
        view={view}
        label={label}
        sent={interaction.pending?.label}
        eliminated={local?.eliminated === true}
        connection={connection}
        onStop={setStop}
      />

      <div className="table">
        <div className="table__side table__side--opponent">
          {opponents.map((seat) => (
            <div key={seat.id} className="table__seat">
              <PlayerPanel
                seat={seat}
                lines={relationLines(related, seat.id)}
                open={browsing?.seat === seat.id ? browsing.zone : undefined}
                onOpen={browse(seat.id)}
                surface={surface}
              />
              <Battlefield
                entries={fieldFor(seat.id)}
                name={seat.name}
                isYou={false}
                surface={surface}
              />
            </div>
          ))}
        </div>

        <StackRail stack={stackEntries} emblems={emblemEntries} label={label} surface={surface} />

        <div className="table__side table__side--you">
          {local && (
            <div className="table__seat">
              <Battlefield entries={fieldFor(local.id)} name={local.name} isYou surface={surface} />
              <PlayerPanel
                seat={local}
                lines={relationLines(related, local.id)}
                open={browsing?.seat === local.id ? browsing.zone : undefined}
                onOpen={browse(local.id)}
                surface={surface}
              />
            </div>
          )}
          {!local && <p className="field__empty">You are watching this table.</p>}
        </div>
      </div>

      <Hand faces={handFaces} surface={surface} />

      <ActionDock
        actions={actions}
        interaction={interaction}
        result={view.result}
        labelFor={surface.labelFor}
        take={take}
        update={setInteraction}
        confirm={confirm}
        inspect={setInspecting}
      />

      <SidePanel
        zone={openZone}
        closeZone={() => setBrowsing(undefined)}
        revealed={revealedFaces}
        settled={list(view.auto_passed_steps)}
        log={list(view.log)}
        label={label}
        surface={surface}
      />

      {/* Last in the tree so they layer over the table without any surface below them needing to
          know they exist. `inspected` is looked up in this frame's faces, so an object that has
          left the view simply stops resolving and the panel closes itself. */}
      {inspected && <CardInspector face={inspected} onClose={() => setInspecting(undefined)} />}

      {view.result && !dismissed && (
        <MatchResult
          result={view.result}
          label={label}
          you={view.you}
          onLeave={leave}
          onDismiss={() => setDismissed(true)}
        />
      )}
    </div>
  )
}
