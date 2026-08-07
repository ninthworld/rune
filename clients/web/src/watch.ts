/**
 * Watching: a `SpectatorView` as the thing every board module already reads, and the two
 * questions a read-only table asks that a seated one never does.
 *
 * `SpectatorView` is `GameView` with every receiver field removed and `players` in the place of
 * `opponents` (`docs/protocol.md`). That is not a coincidence to work around — it is the shape
 * the server chose so a spectator could be rendered by the board rather than by a second one, and
 * this module is the whole of the join. `table.seats`, `relations`, `entityNames`, `board`,
 * `motion`, `turn.steps`, `card-face` and `game-log` are then reached unmodified, and a spectator
 * cannot end up with a different reading of the same objects than a player has.
 *
 * **The projection adds nothing.** It renames one field and passes the rest through, so there is
 * no key in its output that the server did not send — which is what makes the redaction hold on
 * this side of the wire too. It notably does **not** fill in `you`: a spectator sits behind a
 * chair, and telling `normalize.playerLabel` that the chair is theirs would print *You* over
 * somebody else's seat and *Opponent* over the rest of a game they are not in.
 *
 * Nothing here can send. A spectator socket is one-way — `serve_spectator_connection` ignores
 * every frame the client writes to it — so the way out of watching is a new session, the same
 * way it is out of a finished game.
 */
import type { GameView, SpectatorView } from './protocol'
import type { Seat } from './table'
import { phaseLabel } from './turn'

/**
 * A spectator's view in the shape the board reads.
 *
 * Written as a rename over the rest rather than as a field-by-field copy, deliberately: a
 * copy would have to be revisited every time the public half of a view grows, and the one that
 * was forgotten would be a fact the server sent and the spectator was never shown. What the
 * compiler checks here is that every remaining `SpectatorView` field is a `GameView` field of the
 * same type, which is the invariant `docs/protocol.md` states in prose.
 */
export function watched(view: SpectatorView): GameView {
  const { players, ...rest } = view
  return { ...rest, ...(players === undefined ? {} : { opponents: players }) }
}

/**
 * Which seat the spectator is sitting behind.
 *
 * Somebody has to be nearest — a table is drawn across a dividing line, and a board with both
 * halves empty of a chair is not the board a player learnt. So the spectator is put behind one
 * seat and the rest sit across from it, which makes watching a two-player game the same picture
 * as playing one.
 *
 * It is presentation and it is device-local, in the manner of which seat is focused: the view
 * states no chair for a spectator and never will.
 *
 * Unchosen, it is the first seat **still in the game** rather than simply the first: an
 * eliminated seat controls nothing, and defaulting a four-player game to a dead player's empty
 * half spends the nearest region of the table on nothing. Being out is the server's own
 * `eliminated`, so this concludes nothing — and a chair that is no longer at the table at all
 * falls back the same way, so a view that reorders the table cannot leave the near half empty.
 */
export const chairOf = (table: readonly Seat[], behind: string | undefined): Seat | undefined =>
  table.find((seat) => seat.id === behind) ?? table.find((seat) => !seat.eliminated) ?? table[0]

/**
 * What the watching strip says, in the two lines the action bar's band is built from.
 *
 * The same division of labour the dock makes: the first line is what is being waited on and the
 * second is where in the turn that is. It states priority only where the view stated it —
 * `priority_player` is absent while nobody holds it, and filling that in from `active_player`
 * would be this client claiming a seat is being asked something the server did not say it was.
 */
export function watchWording(
  view: GameView,
  label: (id: string) => string,
): { prompt: string; where: string } {
  const holder = view.priority_player
  const prompt = view.result
    ? 'Watching — the game is over'
    : holder === undefined
      ? 'Watching'
      : `Watching — ${label(holder)} to act`
  return { prompt, where: `Turn ${view.turn ?? 0} · ${phaseLabel(view.phase ?? '')}` }
}
