/**
 * The game screen: one `GameView`, arranged as a table.
 *
 * This file composes and derives; it draws almost nothing. Everything visible is a surface in
 * `./game/`, and everything those surfaces need is worked out once here — the faces, what the
 * server named, what an id is called — so no surface holds a second reading of the view.
 *
 * The reading order is fixed and two-player by design: opponent across from you, your side
 * nearest you, the stack between them, your hand along the bottom edge, and the controls that
 * move the game pinned below that. A permanent's controller is answered by *where the card is*
 * rather than by a heading above a list, which is the difference between a table and a state
 * dump. That order does not rearrange for any viewport (§4): density changes, the spatial
 * metaphor does not, so what a player learns on a desktop still holds on a phone.
 *
 * **The arrangement is computed, not composed.** Every region below is placed at a box `scene()`
 * returned for this viewport — absolutely, in scene coordinates — rather than flowing after
 * whatever came before it. That is the whole of #659 and it is a change of substrate rather than
 * of style: flow layout answers "how big is this?" with "however big its contents are", which is
 * exactly the question §5 forbids asking, and every scrollbar the table used to grow was that
 * answer arriving. A region that cannot hold what is in it now tightens the ladder — cards toward
 * their floor, rows merged, faces to chips, rails collapsed — and there is no `overflow: auto`
 * anywhere inside the board for it to fall through to instead.
 *
 * Two departures from "the box never responds to the game", both §5's and both about *what is
 * happening* rather than *how much there is*: an empty stack has no box, because an event that is
 * not happening takes no room while a place at the table keeps its box whether or not anybody has
 * put anything on it; and the hand yields the bottom band to the controls while the game is
 * asking something, which is a change of mode. Neither reads a count.
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
import { useEffect, useState } from 'react'

import type { ClientMessage, GameView, Phase, ValidAction } from './../protocol'
import { list, playerLabel } from './../normalize'
import { seats, type Seat, type SeatPile } from './../table'
import {
  passedEvents,
  phaseLabel,
  presetOf,
  presetStops,
  steps,
  withStop,
  type StopPreset,
  type StopScope,
} from './../turn'
import { claims, intentFor, type KeyPress } from './../keys'
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
  clear,
  disarm,
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
  unask,
  type Interaction,
} from './../interaction'
import { objectMenu } from './../menu'
import { changes, NO_CHANGES } from './../motion'
import { dockTone } from './../dock'
import { boardRows } from './../board'
import { fieldSlots } from './../pack'
import { scene, type Rect } from './../scene'
import { buildChooseAction, type Draft } from './../submission'
import { CardInspector } from './CardInspector'
import { ActionDock } from './game/ActionDock'
import { Motion } from './game/Motion'
import { Settings } from './game/Settings'
import { TurnStrip, type TurnLayout } from './game/TurnStrip'
import { Battlefield, type FieldEntry } from './game/Battlefield'
import { CardPreview } from './game/CardPreview'
import { Region, share, useViewport } from './game/frame'
import { Hand, RaisedHand } from './game/Hand'
import { MatchHeader } from './game/MatchHeader'
import { MatchResult } from './game/MatchResult'
import { ObjectMenu } from './game/ObjectMenu'
import { PlayerPanel } from './game/PlayerPanel'
import { RelationOverlay } from './game/RelationOverlay'
import { SidePanel, type OpenZone } from './game/SidePanel'
import { StackRail } from './game/StackRail'
import { TooSmall } from './game/TooSmall'
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
  // The settings panel — pace, keys, card art. Everything in it is about this device rather
  // than about this game, which is why one new view has no opinion about any of it.
  const [settingsOpen, setSettingsOpen] = useState(false)
  // Whether the player has pulled open the drawer the side column becomes when there is no room
  // for a column (§3, step 8), and whether they have raised the hand over its peek strip (§2).
  // Both are about *this device's* current shape rather than about the game, which is why the
  // view has nothing to say about either and a new one changes neither.
  const [drawerOpen, setDrawerOpen] = useState(false)
  const [handRaised, setHandRaised] = useState(false)

  // What the last message changed, held for exactly as long as this view is the current one.
  // A transition between two reconstructable states and never a third: a refresh loses it and
  // shows the same board, and nothing anywhere reads it to decide anything (`motion.ts`).
  const [moved, setMoved] = useState(NO_CHANGES)

  // Whether the next view is one this player is *arriving at* rather than one they watched
  // arrive. Set by a reconnect and spent by the first view after it, because the two are
  // separate messages and usually separate renders — the socket comes back first, and the board
  // that may have moved a whole turn while it was down comes second.
  const [resuming, setResuming] = useState(false)
  let arriving = resuming

  // A new socket is the end of every correlation this client was holding: the server drops a
  // seat's `action_ack` when it reconnects (`docs/protocol.md`), so an ack that would have
  // answered the click in flight is never coming. Waiting on it forever would leave the dock
  // blocked on a reply that no longer exists, so the wait is released and the fresh view the
  // reconnect brings is the answer to what actually happened.
  //
  // Read before the view below rather than after it, so a reconnect and a view landing in the
  // same render are still a reconnect.
  const [seenEpoch, setSeenEpoch] = useState(epoch)
  if (seenEpoch !== epoch) {
    setSeenEpoch(epoch)
    setInteraction(release)
    // A reconnect is a state a player is arriving at, not a change they watched: the board may
    // have moved a whole turn while the socket was down, and a delta across that gap is real
    // arithmetic and a lie about what the player saw.
    setMoved(NO_CHANGES)
    arriving = true
    setResuming(true)
  }

  // Settled during render rather than in an effect, so the frame that carries a new view is
  // never painted with the previous view's draft still in the dock.
  const [seen, setSeen] = useState(view)
  if (seen !== view) {
    setMoved(changes(arriving ? undefined : seen, view))
    if (resuming) setResuming(false)
    setSeen(view)
    // Updated from the current value rather than the one this render closed over, so a view and
    // a reconnect landing together compose instead of one overwriting the other.
    setInteraction((current) => settle(current, view.action_ack))
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

  /**
   * Pass priority, if that is a thing the server is currently offering.
   *
   * Looked up by the server's own `type` — "a free-form category used for presentation and input
   * routing" (`docs/protocol.md`) — and taken through the same path as a click on the button.
   * A build that does not recognise the classifier simply finds nothing and the key does
   * nothing, which is the harmless direction to be wrong in.
   */
  const passPriority = () => {
    const pass = actions.find((action) => action.type === 'pass_priority')
    if (pass) take(pass)
  }

  /**
   * Change the pace, then hand the game back.
   *
   * Two messages and no loop: the preference the server will honour, and one pass. Everything
   * that happens after that is the server's settle acting on a stored preference (ADR 0010) —
   * this client never decides that a step was uninteresting.
   *
   * Only the skips pass. Asking to stop *everywhere* is the opposite request, and passing on top
   * of it would skip the very step the player just said they wanted.
   */
  const setPace = (preset: StopPreset) => {
    send(presetStops(preset))
    if (preset !== 'everywhere') passPriority()
  }

  /**
   * Back out of whatever is open, one layer at a time.
   *
   * Innermost first, so one key unwinds the screen in the order a player built it up. Nothing
   * here is sent and nothing is undone in the game — every layer this closes is client-side
   * presentation, which is exactly why it is safe to bind to a key pressed by reflex.
   */
  // An object's own actions, at the object. The same list the dock is drawing for the same
  // selection — this decides only *whether* it belongs beside the card too (`menu.ts`).
  const menu = objectMenu(actions, interaction)

  /** Something is layered over the board, so the keyboard belongs to it rather than to the game. */
  const layered = inspecting !== undefined || settingsOpen || menu !== undefined

  const back = () => {
    if (inspecting !== undefined) return setInspecting(undefined)
    if (settingsOpen) return setSettingsOpen(false)
    if (handRaised) return setHandRaised(false)
    if (browsing) return setBrowsing(undefined)
    if (drawerOpen) return setDrawerOpen(false)
    if (interaction.confirming) return setInteraction(unask(interaction))
    if (interaction.armed) return setInteraction(disarm(interaction))
    if (interaction.selected) return setInteraction(clear(interaction))
  }

  // Re-registered every render rather than memoised: the handler reads the current view, the
  // current action list, and the current draft, and a stale closure over any of them would act
  // on a game that has already moved.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target
      const press: KeyPress = {
        key: event.key,
        ctrl: event.ctrlKey,
        meta: event.metaKey,
        alt: event.altKey,
        shift: event.shiftKey,
        typing: isTyping(target),
        onControl: target instanceof HTMLElement && target.closest('button, a[href]') !== null,
      }
      // A panel over the board takes the keyboard with it. Only backing out still means
      // anything: a player reading a card, the key list, or the art settings has not asked to
      // pass priority, and the space that would have done it belongs to the Close button they
      // are focused on.
      const asked = intentFor(press)
      const intent = layered && asked?.kind !== 'cancel' ? undefined : asked
      if (claims(press, intent)) event.preventDefault()
      if (!intent) return

      switch (intent.kind) {
        case 'confirm':
          // One key for the whole of "go on": answer the question if there is one, pass if
          // there is not. A submission already in flight is neither, and a concede waiting for
          // its second click is deliberately not reachable from here — a match-ending button
          // asked twice must be answered by a deliberate click, not by the key a player is
          // already holding down.
          if (interaction.pending || interaction.confirming) return
          if (focus(actions, interaction).ready) confirm()
          else passPriority()
          return
        case 'cancel':
          back()
          return
        case 'stops':
          setPace(intent.preset)
          return
        case 'help':
          setSettingsOpen((open) => !open)
          return
      }
    }

    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

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
      switch (gesture.kind) {
        case 'inspect':
          setInspecting(id)
          return
        case 'select':
          setInteraction(select(interaction, id))
          return
        case 'take': {
          // The same path the dock's own button takes, so one click on a card and a click on
          // the button that names it cannot behave differently — including the draft an action
          // with questions opens, and the second click a concede asks for.
          const action = actions.find((candidate) => candidate.id === gesture.action)
          if (action) take(action)
          return
        }
        case 'fill': {
          const slot = focus(actions, interaction).slots.find((each) => each.slot === gesture.slot)
          if (slot) setInteraction(fill(interaction, slot, id))
          return
        }
      }
    },
    inspect: setInspecting,
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

  // The arrangement, for the window this tab currently is. Derived every render from the
  // viewport and two facts about the table, and stored nowhere: a refresh produces the same
  // boxes from the same window, and no message can leave a region remembering a size.
  //
  // The stack's box is decided by whether the rail has anything in it at all — an object on the
  // stack, or an emblem beside it — and never by how much: a seven-deep stack and a one-deep
  // stack get the same rail, and the depth is absorbed by the items in it exactly as a permanent
  // count is (§5).
  const viewport = useViewport()
  const tone = dockTone(actions, interaction, view.result)
  const asking = tone === 'asking' || tone === 'confirm'
  const { band, regions, ladder } = scene(viewport, {
    stackDepth: stackEntries.length + emblemEntries.length,
    asking,
  })

  // §1's one commitment for a screen below the floor: say so plainly, **in place of** a broken
  // board. Not layered over one — a notice over a table that is still being drawn costs the same
  // layout, leaves a player poking at something that half works, and makes "unsupported" a
  // decoration rather than a decision.
  if (band === 'unsupported') {
    return <TooSmall width={viewport.width} height={viewport.height} onLeave={leave} />
  }

  const collapsed = ladder.rails === 'collapsed'
  const turnLayout: TurnLayout = collapsed ? 'chip' : band === 'square' ? 'strip' : 'rail'
  const drawer = ladder.sidePanel === 'drawer'
  // A pile the player opened and cards the server put in front of them are both things somebody
  // asked for, so the drawer they live in is already open when they arrive.
  const sideOpen = !drawer || drawerOpen || openZone !== undefined || revealedFaces.length > 0
  // §2's trade, in the same terms `scene.ts` allocated the height by: the hand keeps the bottom
  // band while nothing is pending, and yields it the moment there is something to answer — always
  // at Short, where height is the scarce resource and the strip is the resting state.
  const peek = band === 'short' || (band === 'tall' && asking)
  // The look, at full size. Resolved out of this frame's faces like everything else, so an object
  // that leaves the view stops previewing rather than pinning a card that is gone.
  const preview = hovering === undefined ? undefined : faces.get(hovering)

  const seatPanel = (seat: Seat) => (
    <PlayerPanel
      seat={seat}
      lines={relationLines(related, seat.id)}
      life={moved.life.get(seat.id)}
      folded={collapsed}
      open={browsing?.seat === seat.id ? browsing.zone : undefined}
      onOpen={browse(seat.id)}
      surface={surface}
    />
  )

  // §3's step 5, decided for the *table* rather than per seat: how many rows a field divides into
  // is answered once out of both halves' groups, so a permanent is the same size at both ends of
  // the table and nobody's board is two thirds the size of the other's. Both fields have the same
  // box, so either of them states the room. The groups arrive with their sizes because the answer
  // is which arrangement draws the biggest card, and that is decided by the count in a row as much
  // as by the height of one — never by anything a card *says* (`pack.ts`).
  const rowSlots = fieldSlots(
    regions.yourField,
    table.map((seat) =>
      boardRows(fieldFor(seat.id), (entry) => entry.face.cardTypes).map(
        (group) => group.entries.length,
      ),
    ),
    ladder,
  )

  const field = (seat: Seat, box: Rect) => (
    <Battlefield
      entries={fieldFor(seat.id)}
      name={seat.name}
      isYou={seat.isYou}
      box={box}
      slots={rowSlots}
      cardTier={ladder.cardTier}
      surface={surface}
    />
  )

  const sidePanel = (
    <SidePanel
      zone={openZone}
      closeZone={() => setBrowsing(undefined)}
      // Over the board rather than in the column, when there is no column: the preview is the
      // one thing here a player did not ask for, so it cannot wait behind a gesture.
      preview={drawer ? undefined : preview}
      revealed={revealedFaces}
      settled={list(view.auto_passed_steps)}
      missed={passedEvents(view)}
      log={list(view.log)}
      label={label}
      onClose={
        drawer
          ? () => {
              setDrawerOpen(false)
              setBrowsing(undefined)
            }
          : undefined
      }
      surface={surface}
    />
  )

  return (
    <div className="screen" data-band={band}>
      <Region name="header" rect={regions.header}>
        <MatchHeader
          view={view}
          label={label}
          sent={interaction.pending?.label}
          eliminated={local?.eliminated === true}
          connection={connection}
          onHistory={drawer ? () => setDrawerOpen((open) => !open) : undefined}
          onSettings={() => setSettingsOpen(true)}
        />
      </Region>

      {/* The turn. Twelve steps is a lot of horizontal band to spend above a board, so where
          there is width it reads top to bottom in a rail down the left edge; where the viewport
          is square it lies under the header instead; and where neither fits it is the current
          step alone (§3, step 7). Each step is still the control that sets a stop there. */}
      <Region name="turn" rect={regions.turn}>
        <TurnStrip steps={steps(view)} layout={turnLayout} onStop={setStop} />
      </Region>

      {opponents.map((seat, index) => (
        <Region
          key={`seat:${seat.id}`}
          name="opponent-seat"
          rect={share(regions.opponentSeat, index, opponents.length)}
        >
          {seatPanel(seat)}
        </Region>
      ))}

      {opponents.map((seat, index) => {
        const rect = share(regions.opponentField, index, opponents.length)
        return (
          <Region key={`field:${seat.id}`} name="opponent-field" rect={rect}>
            {field(seat, rect)}
          </Region>
        )
      })}

      <Region name="stack" rect={regions.stack}>
        <StackRail
          stack={stackEntries}
          emblems={emblemEntries}
          collapsed={collapsed}
          label={label}
          surface={surface}
        />
      </Region>

      {/* Your half. Its box is the opponent's box, always — the line across the middle of the
          table is drawn once by the viewport and does not move for any game event. */}
      <Region name="your-field" rect={regions.yourField}>
        {local ? (
          field(local, regions.yourField)
        ) : (
          <p className="field__empty">You are watching this table.</p>
        )}
      </Region>

      {local && (
        <Region name="your-seat" rect={regions.yourSeat}>
          {seatPanel(local)}
        </Region>
      )}

      <Region name="dock" rect={regions.dock}>
        <ActionDock
          actions={actions}
          interaction={interaction}
          result={view.result}
          where={`Turn ${view.turn ?? 0} · ${phaseLabel(view.phase)}`}
          labelFor={surface.labelFor}
          take={take}
          update={setInteraction}
          confirm={confirm}
          inspect={setInspecting}
        />
      </Region>

      <Region name="hand" rect={regions.hand}>
        <Hand
          faces={handFaces}
          peek={peek}
          raised={handRaised}
          onRaise={setHandRaised}
          surface={surface}
        />
      </Region>

      {!drawer && (
        <Region name="side" rect={regions.side}>
          {sidePanel}
        </Region>
      )}

      {/* Over the whole table and under nothing: the relationships the server stated, drawn
          between the objects that carry them. Last of the regions so it paints above the cards,
          and it takes no clicks — everything under it stays reachable, and every line it draws
          is also a sentence in the trail beneath the card. It draws from the same join the
          trails do, so the picture and the words cannot describe combat differently. */}
      <RelationOverlay relations={related.all} traced={traced} />

      {/* Nothing this draws; it moves what is already drawn — an object appearing, and a card
          travelling between the two zones the server said it was drawn in. Anything it touches is
          already in its final place, so an interrupted animation, a refresh, or a device that
          asked for no motion at all lands on exactly the board this view describes. */}
      <Motion changes={moved} />

      {/* The look, over the board, where there is no column to put it beside. Suppressed while
          the drawer is standing open, because the drawer is already on that edge and two panels
          fighting for it is one panel too many. */}
      {drawer && !sideOpen && preview && (
        <aside className="preview-over" aria-label="Card preview">
          <CardPreview face={preview} />
        </aside>
      )}

      {drawer && sideOpen && <div className="drawer">{sidePanel}</div>}

      {handRaised && peek && (
        <RaisedHand faces={handFaces} onLower={() => setHandRaised(false)} surface={surface} />
      )}

      {/* An object's actions, beside the object. Opened by the click that already selected it,
          never by a gesture of its own, and taking one goes through the same `take` the dock's
          button does — the dock keeps the identical list for the subject no surface drew. */}
      {menu && (
        <ObjectMenu
          // Remounted per object, so the focus it takes and the focus it gives back are always
          // about the same one.
          key={menu.id}
          menu={menu}
          label={surface.labelFor}
          take={take}
          inspect={setInspecting}
          close={() => setInteraction(clear(interaction))}
        />
      )}

      {/* Last in the tree so they layer over the table without any surface below them needing to
          know they exist. `inspected` is looked up in this frame's faces, so an object that has
          left the view simply stops resolving and the panel closes itself. */}
      {inspected && <CardInspector face={inspected} onClose={() => setInspecting(undefined)} />}

      {settingsOpen && (
        <Settings
          preset={presetOf(view)}
          onPreset={setPace}
          onClose={() => setSettingsOpen(false)}
        />
      )}

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

/**
 * Whether the keyboard currently belongs to something that takes text.
 *
 * A shortcut that fires while someone is filling in the X of an X spell is a bug, and the number
 * slot in the dock is a real text field. `isContentEditable` covers nothing this client renders
 * today and is here because the cost of being wrong about it is a swallowed keystroke.
 */
function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  if (target.isContentEditable) return true
  return ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)
}
