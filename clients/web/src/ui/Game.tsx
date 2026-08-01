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
 * Nothing is remembered between messages. A refresh mid-game produces this same screen from the
 * next frame the server sends, minus an open inspector.
 */
import { useState } from 'react'

import type { ClientMessage, GameView } from './../protocol'
import { list, playerLabel } from './../normalize'
import { seats } from './../table'
import {
  cardFace,
  emblemFace,
  permanentFace,
  stackFace,
  type CardFace,
  type CardFaceState,
} from './../card-face'
import { CardInspector } from './CardInspector'
import { ActionDock } from './game/ActionDock'
import { Battlefield, type FieldEntry } from './game/Battlefield'
import { Hand } from './game/Hand'
import { MatchHeader } from './game/MatchHeader'
import { PlayerPanel } from './game/PlayerPanel'
import { SidePanel } from './game/SidePanel'
import { StackRail } from './game/StackRail'
import type { Surface } from './game/surface'

export function Game({ view, send }: { view: GameView; send(message: ClientMessage): void }) {
  const label = (id: string) => playerLabel(view, id)

  // The inspector remembers an **id**, never a face. Faces are rebuilt from whatever view
  // arrived last, so an open inspector shows the object as it is now — and an object that has
  // left the view closes it rather than pinning a card that no longer exists.
  const [inspecting, setInspecting] = useState<string | undefined>(undefined)

  const table = seats(view)
  const handFaces = list(view.my_hand).map(cardFace)
  const revealedFaces = list(view.revealed).map(cardFace)
  const stackEntries = list(view.stack).map((item) => ({ item, face: stackFace(item) }))
  const emblemEntries = list(view.emblems).map((emblem) => ({ emblem, face: emblemFace(emblem) }))
  const fieldEntries: readonly FieldEntry[] = list(view.battlefield).map((permanent) => ({
    permanent,
    face: permanentFace(permanent),
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

  // Every object the server named in an offered action or one of its target slots. This is a
  // reading of `valid_actions`, not a judgement about it: the client marks what the server
  // pointed at and works out nothing about why. Acting on one of these is #626's job.
  const named = new Set<string>()
  for (const action of list(view.valid_actions)) {
    for (const id of list(action.subject)) named.add(id)
    for (const requirement of list(action.requirements)) {
      for (const id of list(requirement.candidates)) named.add(id)
    }
  }

  // Names for entity ids the surfaces and the action panel mention. The server labels players;
  // cards and permanents are named from the view's own contents, never resolved client-side.
  const names = new Map<string, string>()
  for (const face of faces.values()) names.set(face.id, face.name)
  // The stack's own description, not its card's name: a target on the stack reads better as
  // "Counterspell targeting Twin Bolt" than as "Counterspell".
  for (const item of list(view.stack)) names.set(item.id, item.description)
  for (const seat of table) names.set(seat.id, seat.name)

  const surface: Surface = {
    stateOf: (face: CardFace): CardFaceState => (named.has(face.id) ? 'candidate' : 'idle'),
    inspect: (face: CardFace) => setInspecting(face.id),
    labelFor: (id: string) => names.get(id) ?? id,
  }

  // Opponents across the table, you nearest. With no seat the server named as yours, everyone
  // renders as an opponent rather than one of them being promoted into your chair.
  const opponents = table.filter((seat) => !seat.isYou)
  const local = table.find((seat) => seat.isYou)
  const fieldFor = (id: string) => fieldEntries.filter((entry) => entry.permanent.controller === id)

  return (
    <div className="screen">
      <MatchHeader view={view} label={label} />

      <div className="table">
        <div className="table__side table__side--opponent">
          {opponents.map((seat) => (
            <div key={seat.id} className="table__seat">
              <PlayerPanel seat={seat} surface={surface} />
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
              <PlayerPanel seat={local} surface={surface} />
            </div>
          )}
          {!local && <p className="field__empty">You are watching this table.</p>}
        </div>
      </div>

      <Hand faces={handFaces} surface={surface} />

      <ActionDock
        actions={list(view.valid_actions)}
        result={view.result}
        labelFor={surface.labelFor}
        send={send}
      />

      <SidePanel
        revealed={revealedFaces}
        settled={list(view.auto_passed_steps)}
        log={list(view.log)}
        label={label}
        surface={surface}
      />

      {/* Last in the tree so it layers over the table without any surface below it needing to
          know it exists. `inspected` is looked up in this frame's faces, so an object that has
          left the view simply stops resolving and the panel closes itself. */}
      {inspected && <CardInspector face={inspected} onClose={() => setInspecting(undefined)} />}
    </div>
  )
}
