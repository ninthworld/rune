/**
 * The room roster (issue #506; `front-door-and-lobby.md` §5.3, §5.9).
 *
 * One row per seat, each wearing `SCENE_SEAT_ACCENTS[seat % 6]` as an edge
 * stripe and a **crest chip** — visual-system §6's crest cluster reduced to chip
 * scale: a monogram inside a seat-accent ring, ≥ 44 px. The local player's chip
 * carries the same ring the match's crest wears, which is what makes "that's me"
 * survive the ready gate (fixes P2: the seat's color no longer changes three
 * seconds into the game).
 *
 * Every state keeps its non-color channel (§5.9): the crest monogram and the
 * display name for identity, the "You" and "AI" tags, a dashed bound plus "Open
 * seat" for an empty one, and glyph + word chips for deck and ready. All of it
 * is a presentation read of the `SeatView` — no legality computed here, and the
 * host-only Remove renders only when the server advertises `remove_ai`.
 */
import { cx } from '../chrome/cx';
import { Glyph } from '../chrome/glyphs';
import { seatDisplayName } from '../playerNames';
import type { LobbyView, SeatView } from '../protocol';
import { seatAccentVars } from './pregameScene';
import { seatFilled, seatMonogram } from './seatIdentity';
import p from './styles';

/**
 * The crest chip: a monogram in a seat-accent ring. Exported so the lobby's
 * identity strip can wear the local player's chip beside their name, teaching
 * the name and the color together (§5.2).
 */
export function CrestChip({
  monogram,
  seat,
  local = false,
  open = false,
  testId,
}: {
  /** The chip's monogram; replaced by the seat glyph when `open`. */
  monogram: string;
  /**
   * The seat index the accent is derived from. `undefined` room-less, where no
   * seat — and therefore no deterministic accent — exists yet: the chip then
   * renders unaccented rather than teaching a color the game has not assigned.
   */
  seat?: number;
  /** Whether this is the local player (adds the match's crest ring). */
  local?: boolean;
  /** Whether the seat is unoccupied (dashed invitation, seat glyph). */
  open?: boolean;
  /** Test id override; defaults to the seat's crest id. */
  testId?: string;
}) {
  const accented = seat !== undefined && !open;
  return (
    <span
      className={cx(p.crest, local && accented && p.crestLocal, !accented && p.crestOpen)}
      style={accented ? seatAccentVars(seat) : undefined}
      aria-hidden="true"
      data-testid={testId ?? (seat !== undefined ? `seat-${seat}-crest` : 'crest')}
    >
      {open ? <Glyph name="seat" size={18} /> : monogram}
    </span>
  );
}

/**
 * One roster row. An AI seat (issue #415) reads as filled, tagged as a computer
 * opponent, with a host-only Remove when `remove_ai` is advertised.
 */
export function RosterRow({
  view,
  seat,
  onRemoveAi,
}: {
  view: LobbyView;
  seat: SeatView;
  onRemoveAi?: (seat: number) => void;
}) {
  const isAi = seat.ai !== undefined;
  const occupied = seatFilled(seat);
  const isLocal = seat.occupied_by !== undefined && seat.occupied_by === view.you;

  return (
    <li
      className={cx(p.rosterRow, !occupied && p.seatOpen)}
      style={occupied ? seatAccentVars(seat.seat) : undefined}
      data-testid={`seat-${seat.seat}`}
    >
      <span className={p.rosterWho}>
        <CrestChip
          monogram={seatMonogram(seat)}
          seat={seat.seat}
          local={isLocal}
          open={!occupied}
        />
        <span className={p.rosterNames}>
          <span className={p.seatName}>
            {isAi ? (seat.name ?? 'Computer') : occupied ? seatDisplayName(seat) : 'Open seat'}
          </span>
          {isLocal && <span className={p.youTag}>You</span>}
          {isAi && (
            <span className={p.youTag} data-testid={`seat-${seat.seat}-ai`}>
              AI
            </span>
          )}
        </span>
      </span>
      {occupied ? (
        <span className={p.rosterState}>
          {seat.decked === true ? (
            <span className={p.stateChipOn} data-testid={`seat-${seat.seat}-decked`}>
              <Glyph name="zone-library" size={12} />
              Deck submitted
            </span>
          ) : (
            <span className={p.stateChip}>Choosing a deck</span>
          )}
          {seat.ready === true ? (
            <span className={p.stateChipOn} data-testid={`seat-${seat.seat}-ready`}>
              <Glyph name="ready" size={12} />
              Ready
            </span>
          ) : (
            <span className={p.stateChip}>Not ready</span>
          )}
          {isAi && onRemoveAi !== undefined && (
            <button
              type="button"
              className={p.button}
              onClick={() => onRemoveAi(seat.seat)}
              data-testid={`remove-ai-${seat.seat}-button`}
            >
              Remove
            </button>
          )}
        </span>
      ) : (
        <span className={cx(p.rosterState, p.muted)}>Waiting for a player…</span>
      )}
    </li>
  );
}

/** The seat roster: one row per seat, in seat order. */
export function Roster({
  view,
  seats,
  onRemoveAi,
}: {
  view: LobbyView;
  seats: readonly SeatView[];
  onRemoveAi?: (seat: number) => void;
}) {
  return (
    <ul className={p.rosterList} data-testid="seat-list">
      {seats.map((seat) => (
        <RosterRow key={seat.seat} view={view} seat={seat} onRemoveAi={onRemoveAi} />
      ))}
    </ul>
  );
}
