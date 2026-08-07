/**
 * The table, watched: one `SpectatorView`, arranged as the same board a player sits at.
 *
 * **This is not a second board.** Every region on screen is the surface `Board.tsx` draws —
 * `PhaseBar`, `PlayerBar`, `Field`, `SidePanel`, `ZoneView`, `Arrows`, `Motion`, `MatchResult` —
 * over the same derived answers, from the same modules, in the same stylesheet. What it is not is
 * the same *file*: `interaction.ts`, `submission.ts`, `keys.ts` and `send` are absent from this
 * composition, so a spectator's read-only-ness is structural rather than a flag over machinery
 * that could still act. That is the same argument the server makes one layer down — redaction by
 * a type that has no field to leak (`docs/protocol.md`, `SpectatorView`) — and it is why the
 * acceptance criterion "no control capable of mutating the game is rendered" is answered by what
 * this file imports rather than by what it branches on.
 *
 * The arrangement is the stylesheet's, as it is for a seated board, with one row's difference:
 * `.layout-watch` drops the hand and gives the board its share (`styles/board.css`). The action
 * bar's band stays, because where in the turn you are is the thing its colour has always said
 * (§6.5) and a spectator reads that the same way a player does — it simply carries a sentence
 * instead of a question.
 *
 * **Somebody is always nearest.** The view names no seat as the reader's and never will, so the
 * spectator is put behind one chair and the rest of the table sits across from it (`watch.chairOf`);
 * clicking a seat's own bar moves behind that one. Which chair that is, is presentation held here
 * in the manner of which seat is focused — a refresh puts the spectator behind the first seat
 * again and the board is otherwise identical, which is the complete-view principle holding.
 *
 * Leaving is a new session rather than a message: a spectator socket is one-way, so there is no
 * `leave` for this client to send (`watch.ts`).
 */
import { useState } from 'react'

import { emblemFace, permanentFace, stackFace, type CardFace } from './../../card-face'
import { arrowsFor } from './../../arrows'
import { barTone } from './../../dock'
import { changes, NO_CHANGES } from './../../motion'
import { list, playerLabel } from './../../normalize'
import type { GameView } from './../../protocol'
import { entityNames, relationLines, relationNote, relations, UNNAMED } from './../../relations'
import type { ConnectionStatus } from './../../socket'
import { seats, type SeatPile } from './../../table'
import { steps } from './../../turn'
import { chairOf, watchWording } from './../../watch'
import { Arrows } from './Arrows'
import { Field, type FieldEntry } from './Field'
import { MatchResult } from './MatchResult'
import { Motion } from './Motion'
import { PhaseBar } from './PhaseBar'
import { PlayerBar } from './PlayerBar'
import { SidePanel, type StackEntry } from './SidePanel'
import { ZoneView, type OpenZone } from './ZoneView'
import type { Surface } from './surface'

/** How many seats fit across before the band takes another row — the board's own rule (§4). */
const seatGrid = (count: number, maxCols: number) => {
  if (count === 0) return { cols: 1, rows: 1 }
  const rows = Math.ceil(count / Math.min(count, maxCols))
  return { cols: Math.ceil(count / rows), rows }
}

export function Watch({
  view,
  connection,
  epoch,
  leave,
  onSettings,
}: {
  /** A `SpectatorView` in the board's shape (`watch.watched`) — never a seated view. */
  view: GameView
  connection: ConnectionStatus
  /** How many sockets this tab has opened; a change means the board moved unwatched. */
  epoch: number
  leave(): void
  onSettings(): void
}) {
  const [hovering, setHovering] = useState<string | undefined>(undefined)
  const [pinned, setPinned] = useState<string | undefined>(undefined)
  const [browsing, setBrowsing] = useState<{ seat: string; zone: SeatPile['zone'] } | undefined>(
    undefined,
  )
  const [focused, setFocused] = useState<string | undefined>(undefined)
  const [behind, setBehind] = useState<string | undefined>(undefined)
  const [dismissed, setDismissed] = useState(false)
  const [sideOpen, setSideOpen] = useState(() => window.innerWidth > 900)

  // What the last message changed, for exactly as long as this view is the current one — the
  // same transition between two reconstructable states a seated board draws.
  const [moved, setMoved] = useState(NO_CHANGES)
  const [resuming, setResuming] = useState(false)
  let arriving = resuming

  // A spectator is dropped from the room's roster on disconnect and re-joins by asking again
  // (`docs/protocol.md`), so a new socket means the board may have moved a whole turn unwatched.
  // A delta across that gap is real arithmetic and a lie about what the spectator saw.
  const [seenEpoch, setSeenEpoch] = useState(epoch)
  if (seenEpoch !== epoch) {
    setSeenEpoch(epoch)
    setMoved(NO_CHANGES)
    arriving = true
    setResuming(true)
  }

  const [seen, setSeen] = useState(view)
  if (seen !== view) {
    setMoved(changes(arriving ? undefined : seen, view))
    if (resuming) setResuming(false)
    setSeen(view)
  }

  const label = (id: string) => playerLabel(view, id)
  const table = seats(view)
  const related = relations(view)
  const names = entityNames(view)

  const emblemFaces = list(view.emblems).map(emblemFace)
  // Bottom first on the wire, top first in the column: what resolves next is what a watcher is
  // reading. Reversed here for the same reason a seated board reverses it.
  const stackEntries: readonly StackEntry[] = list(view.stack)
    .map((item) => {
      const lines = relationLines(related, item.id, names)
      const targets = lines
        .filter((line) => line.kind === 'targeting' && line.direction === 'from')
        .flatMap((line) => line.ends.map((end) => end.name))
      const face = stackFace(item)
      const description = item.description?.trim() ?? ''
      const detail = description === '' || description === face.name ? undefined : description
      return {
        item,
        face,
        who: item.controller === undefined ? '' : label(item.controller),
        kind: face.markers[0] ?? 'On the stack',
        ...(detail === undefined ? {} : { detail }),
        targets,
      }
    })
    .reverse()

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
      note: relationNote(relationLines(related, permanent.id, names)),
      // Nothing is being drafted, so nothing is turning that the *server* did not turn.
      turning: false,
    }))

  // One face per card-shaped object on screen. `revealed` is not among them and never can be:
  // it is cards a *receiver* alone is being shown, and the spectator type has no such field.
  const faces = new Map<string, CardFace>()
  for (const face of [
    ...emblemFaces,
    ...stackEntries.map((entry) => entry.face),
    ...permanents.map(permanentFace),
    ...table.flatMap((seat) => seat.piles.flatMap((pile) => pile.faces)),
  ]) {
    faces.set(face.id, face)
  }

  // Reading is the whole of what a spectator does, so every gesture resolves to it. `activate`
  // is a click and `inspect` a right-click, and both open the same card — there is no action to
  // take, no slot to fill and no selection to hold, which is why this surface is four lines.
  const surface: Surface = {
    // The server offered this connection no actions and never will, so no object is taking part
    // in one. `idle` is not a default standing in for an unknown — it is the whole answer.
    stateOf: () => 'idle',
    linkOf: (id: string) => {
      const linked = hovering === undefined ? new Set<string>() : related.linked(hovering)
      return linked.size === 0
        ? undefined
        : id === hovering
          ? 'focus'
          : linked.has(id)
            ? 'linked'
            : undefined
    },
    trace: setHovering,
    activate: (id: string) => setPinned((held) => (held === id ? undefined : id)),
    inspect: (id: string) => setPinned((held) => (held === id ? undefined : id)),
    labelFor: (id: string) => names.get(id) ?? UNNAMED,
  }

  const near = chairOf(table, behind)
  const across = table.filter((seat) => seat.id !== near?.id)
  const fieldFor = (id: string) => fieldEntries.filter((entry) => entry.permanent.controller === id)

  const focusedSeat = across.some((seat) => seat.id === focused) ? focused : undefined
  const wide = seatGrid(across.length, 4)
  const narrow = seatGrid(across.length, across.length <= 2 ? 1 : 2)

  const browse = (seat: string, zone: SeatPile['zone']) =>
    setBrowsing((at) => (at?.seat === seat && at.zone === zone ? undefined : { seat, zone }))

  const openSeat = table.find((seat) => seat.id === browsing?.seat)
  const openPile = openSeat?.piles.find((pile) => pile.zone === browsing?.zone)
  const zone: OpenZone | undefined =
    openSeat && openPile
      ? { title: `${openSeat.name} — ${openPile.label}`, faces: openPile.faces }
      : undefined

  const looking = pinned ?? hovering
  const preview = looking === undefined ? undefined : faces.get(looking)
  const wording = watchWording(view, label)

  /** Sit behind another chair — the one gesture watching has that playing does not. */
  const sitBehind = (id: string) => setBehind((at) => (at === id ? undefined : id))

  return (
    <div className={`layout layout-watch${sideOpen ? '' : ' log-hidden'}`}>
      <div className="topbar">
        <span className="topbar-fill" />
        {connection !== 'open' && <span className="topbar-note">Reconnecting…</span>}
        <button className="settings-btn" title="Settings" onClick={onSettings}>
          ⚙
        </button>
        <button
          className="menu-btn"
          title="Stack, log and chat"
          aria-expanded={sideOpen}
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
          {across.map((seat) => {
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
                  note={relationNote(relationLines(related, seat.id, names))}
                  focused={focusedSeat === seat.id}
                  onFocus={() => setFocused((at) => (at === seat.id ? undefined : seat.id))}
                  onOpen={(pile) => browse(seat.id, pile.zone)}
                  // A click on somebody's seat is the only thing it can be for a watcher: go and
                  // sit behind them. There is no action a seat could be the subject of here.
                  onActivate={sitBehind}
                  {...(surface.linkOf(seat.id) ? { link: surface.linkOf(seat.id) } : {})}
                />
                {!collapsed && (
                  <Field
                    entries={fieldFor(seat.id)}
                    label={`${seat.name}: battlefield`}
                    mirrored
                    surface={surface}
                  />
                )}
              </div>
            )
          })}
        </div>

        <PhaseBar
          className="phase-mid"
          turn={view.turn ?? 0}
          active={view.active_player === undefined ? '—' : label(view.active_player)}
          steps={steps(view)}
        />

        <div
          className={`field field-mine${near?.id === view.active_player ? ' field-active' : ''}`}
        >
          {near ? (
            <>
              <PlayerBar
                seat={near}
                note={relationNote(relationLines(related, near.id, names))}
                onOpen={(pile) => browse(near.id, pile.zone)}
                onActivate={sitBehind}
                {...(surface.linkOf(near.id) ? { link: surface.linkOf(near.id) } : {})}
              />
              <Field
                entries={fieldFor(near.id)}
                label={`${near.name}: battlefield`}
                surface={surface}
              />
            </>
          ) : (
            <div className="panel-empty">This table has no seats in it.</div>
          )}
        </div>
      </div>

      <SidePanel
        open={sideOpen}
        {...(preview ? { preview } : {})}
        pinned={pinned !== undefined && preview !== undefined}
        onUnpin={() => setPinned(undefined)}
        // Never anything: `revealed` is a receiver-only field the spectator type does not carry.
        revealed={[]}
        stack={stackEntries}
        log={list(view.log)}
        label={label}
        concedeAsked={false}
        surface={surface}
      />

      {/* The action bar's band, tinted by where in the turn the game is exactly as it is for a
          player (§6.5) — the one thing about pacing a watcher reads the same way. What it carries
          is a sentence rather than a question, and the way out of watching. */}
      <div
        className={`action-bar action-${barTone(view.phase)}`}
        role="region"
        aria-label="Watching"
      >
        <div className="action-text">
          <span className="action-prompt">{wording.prompt}</span>
          <span className="action-phase">{wording.where}</span>
        </div>
        <div className="action-btns">
          <button
            className="action-done"
            onClick={leave}
            title="Leaving ends this session and starts a new one."
          >
            Stop watching
          </button>
        </div>
      </div>

      <Motion changes={moved} />

      <Arrows arrows={[...arrowsFor(related.all)]} />

      {zone && (
        <ZoneView
          zone={zone}
          asking={false}
          onClose={() => setBrowsing(undefined)}
          surface={surface}
        />
      )}

      {view.result && !dismissed && (
        <MatchResult
          result={view.result}
          label={label}
          onLeave={leave}
          onDismiss={() => setDismissed(true)}
        />
      )}
    </div>
  )
}
