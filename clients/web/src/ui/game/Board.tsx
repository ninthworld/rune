/**
 * The table: one `GameView`, arranged as a board.
 *
 * This file composes and derives; it draws almost nothing. Everything visible is a surface
 * beside it, and everything those surfaces need is worked out once here — the faces, what the
 * server named, what an id is called — so no surface holds a second reading of the view.
 *
 * **The arrangement is the stylesheet's** (`styles/board.css`, and `docs/client-design.md` §5).
 * A grid of fr rows: the opponents' band takes one share per row of seats it holds, my half
 * takes the rest, the action bar is a permanent band above the hand, and the side column is a
 * fixed width beside all of it. Nothing here measures a viewport and nothing computes a box: a
 * card's size is the height of the region it is in, and a region that runs out of *width* pans
 * sideways at full card size. No region scrolls vertically, and none of them can grow a
 * scrollbar.
 *
 * The reading order is fixed: opponents across from you, your side nearest you, your hand along
 * the bottom edge, and the bar that moves the game directly above it. That order does not
 * rearrange for any viewport — density changes, the spatial metaphor does not — so what a player
 * learns on a desktop still holds on a phone.
 *
 * It owns the one piece of interaction state and routes every click through `interaction.ts` so
 * the rule is written once. What it holds is presentation and nothing else: what is selected,
 * what is being drafted, and which submission is still unanswered. A new view throws all of it
 * away — `settle` — because a draft assembled against the previous view names slots and ids this
 * one may no longer offer. The submission is the single thing that crosses that boundary, and it
 * is the server that closes it, by echoing the id back in `action_ack`.
 *
 * A refresh mid-game therefore produces this same screen from the next frame the server sends,
 * minus an open pile and minus a click in flight.
 */
import { useEffect, useState } from 'react'

import {
  cardFace,
  emblemFace,
  permanentFace,
  stackFace,
  type CardFace,
  type CardFaceLink,
} from './../../card-face'
import { arrowsFor } from './../../arrows'
import { barTone, dockTone, dockWording } from './../../dock'
import {
  IDLE,
  arm,
  ask,
  clear,
  disarm,
  fill,
  focus,
  gestureFor,
  globalActions,
  highlightFor,
  needsChoices,
  needsConfirmation,
  release,
  select,
  settle,
  submitted,
  unask,
  type Interaction,
} from './../../interaction'
import { claims, intentFor, type KeyPress } from './../../keys'
import { objectMenu } from './../../menu'
import { list, playerLabel } from './../../normalize'
import type { ClientMessage, GameView, Phase, ValidAction } from './../../protocol'
import { entityNames, relationLines, relations, UNNAMED } from './../../relations'
import type { ConnectionStatus } from './../../socket'
import { buildChooseAction, type Draft } from './../../submission'
import { seats, type Seat, type SeatPile } from './../../table'
import {
  phaseLabel,
  presetOf,
  presetStops,
  steps,
  withStop,
  type StopPreset,
  type StopScope,
} from './../../turn'
import { ActionBar } from './ActionBar'
import { Arrows } from './Arrows'
import { Field, type FieldEntry } from './Field'
import { Hand } from './Hand'
import { MatchResult } from './MatchResult'
import { ObjectMenu } from './ObjectMenu'
import { PhaseBar } from './PhaseBar'
import { PlayerBar } from './PlayerBar'
import { SidePanel, type StackEntry } from './SidePanel'
import { ZoneView, type OpenZone } from './ZoneView'
import type { Surface } from './surface'

// Opaque and client-generated: the server echoes it back verbatim and derives nothing from it.
let submissionCounter = 0
const nextSubmissionId = (): string => `s:${++submissionCounter}`

/** How many seats fit across before the band takes another row (§4, "many seats"). */
const seatGrid = (count: number, maxCols: number) => {
  if (count === 0) return { cols: 1, rows: 1 }
  const rows = Math.ceil(count / Math.min(count, maxCols))
  return { cols: Math.ceil(count / rows), rows }
}

export function Board({
  view,
  connection,
  epoch,
  send,
  leave,
  onSettings,
}: {
  view: GameView
  connection: ConnectionStatus
  /** How many sockets this tab has opened; a change means everything in flight was lost. */
  epoch: number
  send(message: ClientMessage): void
  leave(): void
  onSettings(): void
}) {
  const [interaction, setInteraction] = useState<Interaction>(IDLE)
  const [hovering, setHovering] = useState<string | undefined>(undefined)
  const [pinned, setPinned] = useState<string | undefined>(undefined)
  const [browsing, setBrowsing] = useState<{ seat: string; zone: SeatPile['zone'] } | undefined>(
    undefined,
  )
  const [focused, setFocused] = useState<string | undefined>(undefined)
  const [dismissed, setDismissed] = useState(false)
  const [sideOpen, setSideOpen] = useState(() => window.innerWidth > 900)

  // A new socket is the end of every correlation this client was holding: the server drops a
  // seat's `action_ack` when it reconnects (`docs/protocol.md`), so an ack that would have
  // answered the click in flight is never coming.
  const [seenEpoch, setSeenEpoch] = useState(epoch)
  if (seenEpoch !== epoch) {
    setSeenEpoch(epoch)
    setInteraction(release)
  }

  // Settled during render rather than in an effect, so the frame that carries a new view is
  // never painted with the previous view's draft still in the bar.
  const [seen, setSeen] = useState(view)
  if (seen !== view) {
    setSeen(view)
    setInteraction((current) => settle(current, view.action_ack))
  }

  const label = (id: string) => playerLabel(view, id)
  const actions = list(view.valid_actions)
  const table = seats(view)
  // Combat, attachments, targets and sources, joined once from the identifiers the server
  // stated. The arrows over the board and the trails in the stack read from this one graph.
  const related = relations(view)
  const names = entityNames(view)

  const handFaces = list(view.my_hand).map(cardFace)
  const revealedFaces = list(view.revealed).map(cardFace)
  const emblemFaces = list(view.emblems).map(emblemFace)
  const stackEntries: readonly StackEntry[] = list(view.stack).map((item) => {
    const lines = relationLines(related, item.id, names)
    const targets = lines
      .filter((line) => line.kind === 'targeting' && line.direction === 'from')
      .flatMap((line) => line.ends.map((end) => end.name))
    return {
      item,
      face: stackFace(item),
      who: item.controller === undefined ? '' : label(item.controller),
      kind: stackFace(item).markers[0] ?? 'On the stack',
      targets,
    }
  })

  // A permanent attached to another is drawn behind it rather than as a slot of its own, so the
  // board carries one box per thing that acts. Which is attached to which is the server's.
  const permanents = list(view.battlefield)
  const attachedTo = new Map<string, CardFace[]>()
  for (const permanent of permanents) {
    if (permanent.attached_to === undefined) continue
    const carried = attachedTo.get(permanent.attached_to) ?? []
    carried.push(permanentFace(permanent))
    attachedTo.set(permanent.attached_to, carried)
  }
  const fieldEntries: readonly FieldEntry[] = permanents
    .filter((permanent) => permanent.attached_to === undefined)
    .map((permanent) => ({
      permanent,
      face: permanentFace(permanent),
      attached: attachedTo.get(permanent.id) ?? [],
    }))

  // One face per card-shaped object on screen, so the hand, the board, the piles, and the
  // preview cannot disagree about the same object.
  const faces = new Map<string, CardFace>()
  for (const face of [
    ...handFaces,
    ...revealedFaces,
    ...emblemFaces,
    ...stackEntries.map((entry) => entry.face),
    ...permanents.map(permanentFace),
    ...table.flatMap((seat) => seat.piles.flatMap((pile) => pile.faces)),
  ]) {
    faces.set(face.id, face)
  }

  // Which objects the table itself puts a box on, so a question about them is answered on them
  // and the bar carries only the subjects it did not draw (§6.5).
  const drawn = new Set<string>([
    ...handFaces.map((face) => face.id),
    ...revealedFaces.map((face) => face.id),
    ...permanents.map((permanent) => permanent.id),
    ...stackEntries.map((entry) => entry.item.id),
    ...table.map((seat) => seat.id),
    ...(browsing ? (openPileFaces(table, browsing) ?? []) : []),
  ])

  /** Send one action, and start waiting on the answer. */
  const dispatch = (action: ValidAction, draft: Draft) => {
    const submission = nextSubmissionId()
    send(buildChooseAction(action, draft, submission))
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

  const current = focus(actions, interaction)

  const confirm = () => {
    if (!current.action || !current.ready || interaction.pending) return
    dispatch(current.action, interaction.draft)
  }

  /**
   * Pass priority, if that is a thing the server is currently offering.
   *
   * Looked up by the server's own `type` and taken through the same path as a click on the
   * button. A build that does not recognise the classifier finds nothing and the key does
   * nothing, which is the harmless direction to be wrong in.
   */
  const passPriority = () => {
    const pass = actions.find((action) => action.type === 'pass_priority')
    if (pass) take(pass)
  }

  /**
   * Change the pace, then hand the game back. Two messages and no loop: the preference the
   * server will honour, and one pass. Everything after that is the server's settle acting on a
   * stored preference (ADR 0010) — this client never decides that a step was uninteresting.
   */
  const setPace = (preset: StopPreset) => {
    send(presetStops(preset))
    if (preset !== 'everywhere') passPriority()
  }

  const setStop = (phase: Phase, scope: StopScope) => send(withStop(view, phase, scope))

  // Clicking an open pile's own button closes it, so the control that opened it is also the way
  // out and the column needs no second one.
  const browse = (seat: string, zone: SeatPile['zone']) =>
    setBrowsing((at) => (at?.seat === seat && at.zone === zone ? undefined : { seat, zone }))

  const menu = objectMenu(actions, interaction)
  const layered = menu !== undefined

  /** Back out of whatever is open, one layer at a time, innermost first. */
  const back = () => {
    if (pinned !== undefined) return setPinned(undefined)
    if (browsing) return setBrowsing(undefined)
    if (interaction.confirming) return setInteraction(unask(interaction))
    if (interaction.armed) return setInteraction(disarm(interaction))
    if (interaction.selected) return setInteraction(clear(interaction))
    if (focused !== undefined) return setFocused(undefined)
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
      const asked = intentFor(press)
      const intent = layered && asked?.kind !== 'cancel' ? undefined : asked
      if (claims(press, intent)) event.preventDefault()
      if (!intent) return

      switch (intent.kind) {
        case 'confirm':
          // One key for the whole of "go on": answer the question if there is one, pass if there
          // is not. A concede waiting for its second click is deliberately not reachable from
          // here — a match-ending button asked twice must be answered by a deliberate click.
          if (interaction.pending || interaction.confirming) return
          if (current.ready) confirm()
          else passPriority()
          return
        case 'cancel':
          back()
          return
        case 'stops':
          setPace(intent.preset)
          return
        case 'help':
          onSettings()
          return
      }
    }

    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  // Tracing follows the look and falls back to the click: hovering an object emphasises
  // everything the server related it to, which reaches the objects that most need it — a blocker
  // or an enchanted creature usually owns no action.
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
          setPinned((held) => (held === id ? undefined : id))
          return
        case 'select':
          setInteraction(select(interaction, id))
          return
        case 'take': {
          const action = actions.find((candidate) => candidate.id === gesture.action)
          if (action) take(action)
          return
        }
        case 'fill': {
          const slot = current.slots.find((each) => each.slot === gesture.slot)
          if (slot) setInteraction(fill(interaction, slot, id))
          return
        }
      }
    },
    inspect: (id: string) => setPinned((held) => (held === id ? undefined : id)),
    labelFor: (id: string) => names.get(id) ?? UNNAMED,
  }

  // Opponents across the table, you nearest. With no seat the server named as yours, everyone
  // renders as an opponent rather than one of them being promoted into your chair.
  const opponents = table.filter((seat) => !seat.isYou)
  const local = table.find((seat) => seat.isYou)
  const fieldFor = (id: string) => fieldEntries.filter((entry) => entry.permanent.controller === id)

  // A seat that has been stepped away from stops being the focused one.
  const focusedSeat = opponents.some((seat) => seat.id === focused) ? focused : undefined
  const wide = seatGrid(opponents.length, 4)
  const narrow = seatGrid(opponents.length, opponents.length <= 2 ? 1 : 2)

  // Resolved out of this frame's seats rather than remembered as cards, so an opened pile shows
  // what is in it now. A pile the view no longer carries stops resolving, which closes it.
  const openSeat = table.find((seat) => seat.id === browsing?.seat)
  const openPile = openSeat?.piles.find((pile) => pile.zone === browsing?.zone)
  const zone: OpenZone | undefined =
    openSeat && openPile
      ? { title: `${openSeat.name} — ${openPile.label}`, faces: openPile.faces }
      : undefined

  const looking = pinned ?? hovering
  const preview = looking === undefined ? undefined : faces.get(looking)

  const tone = dockTone(actions, interaction, view.result)
  const globals = globalActions(actions)
  const concede = globals.find((action) => needsConfirmation(action))
  const barButtons = globals.filter((action) => action !== concede)

  return (
    <div className={`layout${sideOpen ? '' : ' log-hidden'}`}>
      <div className="topbar">
        <span className="topbar-fill" />
        {connection !== 'open' && <span className="topbar-note">Reconnecting…</span>}
        <button className="settings-btn" title="Settings" onClick={onSettings}>
          ⚙
        </button>
        <button
          className="menu-btn"
          title="Stack, log and chat"
          onClick={() => setSideOpen((on) => !on)}
        >
          ☰
        </button>
      </div>

      <PhaseBar
        className="phase-top"
        turn={view.turn ?? 0}
        active={view.active_player === undefined ? '—' : label(view.active_player)}
        steps={steps(view)}
        onStop={setStop}
      />

      <div
        className="battlefield"
        style={
          {
            '--opp-cols': wide.cols,
            '--opp-rows': wide.rows,
            '--opp-band': focusedSeat ? '1fr' : `${wide.rows}fr`,
            '--opp-cols-n': narrow.cols,
            '--opp-rows-n': narrow.rows,
            '--opp-band-n': focusedSeat ? '1fr' : `${narrow.rows}fr`,
          } as React.CSSProperties
        }
      >
        <div className={`field-opponents${focusedSeat ? ' opp-focused' : ''}`}>
          {opponents.map((seat) => {
            const collapsed = focusedSeat !== undefined && focusedSeat !== seat.id
            return (
              <div
                key={seat.id}
                className={
                  'field field-opponent' +
                  (collapsed ? ' field-collapsed' : '') +
                  (seat.id === view.active_player ? ' field-active' : '')
                }
              >
                <PlayerBar
                  seat={seat}
                  focused={focusedSeat === seat.id}
                  onFocus={() => setFocused((at) => (at === seat.id ? undefined : seat.id))}
                  onOpen={(pile) => browse(seat.id, pile.zone)}
                  onActivate={surface.activate}
                  state={surface.stateOf(seat.id)}
                  {...(surface.linkOf(seat.id) ? { link: surface.linkOf(seat.id) } : {})}
                />
                {!collapsed && <Field entries={fieldFor(seat.id)} mirrored surface={surface} />}
              </div>
            )
          })}
        </div>

        <PhaseBar
          className="phase-mid"
          turn={view.turn ?? 0}
          active={view.active_player === undefined ? '—' : label(view.active_player)}
          steps={steps(view)}
          onStop={setStop}
        />

        <div
          className={`field field-mine${local?.id === view.active_player ? ' field-active' : ''}`}
        >
          {local ? (
            <>
              <PlayerBar
                seat={local}
                handCount={handFaces.length}
                onOpen={(pile) => browse(local.id, pile.zone)}
                onActivate={surface.activate}
                state={surface.stateOf(local.id)}
                {...(surface.linkOf(local.id) ? { link: surface.linkOf(local.id) } : {})}
              />
              <Field entries={fieldFor(local.id)} surface={surface} />
            </>
          ) : (
            <div className="panel-empty">You are watching this table.</div>
          )}
        </div>
      </div>

      <SidePanel
        open={sideOpen}
        {...(preview ? { preview } : {})}
        pinned={pinned !== undefined && preview !== undefined}
        onUnpin={() => setPinned(undefined)}
        revealed={revealedFaces}
        stack={stackEntries}
        log={list(view.log)}
        label={label}
        {...(presetOf(view) ? { preset: presetOf(view) } : {})}
        onPreset={setPace}
        {...(concede ? { onConcede: () => take(concede) } : {})}
        concedeAsked={concede !== undefined && interaction.confirming === concede.id}
        surface={surface}
      />

      <ActionBar
        tone={barTone(view.phase)}
        prompt={interaction.pending?.label ?? dockWording(tone)}
        where={`Turn ${view.turn ?? 0} · ${phaseLabel(view.phase ?? '')}`}
        {...(current.action ? { action: current.action } : {})}
        slots={current.slots}
        ready={current.ready}
        blocked={interaction.pending !== undefined}
        interaction={interaction}
        drawn={drawn}
        buttons={barButtons}
        labelFor={surface.labelFor}
        update={setInteraction}
        confirm={confirm}
        cancel={() => setInteraction(disarm(interaction))}
        take={take}
      />

      <Hand faces={handFaces} surface={surface} />

      {/* Over the whole table and under nothing: the relationships the server stated, drawn
          between the objects that carry them. It takes no clicks, and every line it draws is
          also a sentence beside the object that carries it. */}
      <Arrows arrows={arrowsFor(related.all)} />

      {zone && (
        <ZoneView
          zone={zone}
          asking={interaction.armed !== undefined}
          onClose={() => setBrowsing(undefined)}
          surface={surface}
        />
      )}

      {menu && (
        <ObjectMenu
          // Remounted per object, so the focus it takes and the focus it gives back are always
          // about the same one.
          key={menu.id}
          menu={menu}
          label={surface.labelFor}
          take={take}
          close={() => setInteraction(clear(interaction))}
        />
      )}

      {view.result && !dismissed && (
        <MatchResult
          result={view.result}
          label={label}
          {...(view.you === undefined ? {} : { you: view.you })}
          onLeave={leave}
          onDismiss={() => setDismissed(true)}
        />
      )}
    </div>
  )
}

/** The ids an opened pile is currently drawing, which are answerable in it. */
function openPileFaces(
  table: readonly Seat[],
  browsing: { seat: string; zone: SeatPile['zone'] },
): readonly string[] | undefined {
  const seat = table.find((entry) => entry.id === browsing.seat)
  const pile = seat?.piles.find((entry) => entry.zone === browsing.zone)
  return pile?.faces.map((face) => face.id)
}

/**
 * Whether the keyboard currently belongs to something that takes text.
 *
 * A shortcut that fires while someone is filling in the X of an X spell is a bug, and the number
 * slot in the action bar is a real text field.
 */
function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  if (target.isContentEditable) return true
  return ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)
}
