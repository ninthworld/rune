/**
 * The ready room's **seats** (issue #546; the approved
 * `docs/ui-concepts/rune-pregame-game-lobby.jpg` baseline).
 *
 * The baseline replaces #506's roster table with the arena itself: each seat is
 * a plaque standing where that player will sit, and everything a seat needs is
 * on the seat.
 *
 * - An **occupied** seat shows identity, readiness, and its deck *state*. The
 *   baseline prints the deck's name there; the wire does not carry one — a
 *   `SeatView` has a redacted `decked` flag and nothing else — so the seat says
 *   what is known instead of a name the client would have had to invent.
 * - The **local** seat expands: the deck dropdown plus quiet edit/import
 *   shortcuts. It is the only seat that grows.
 * - An **empty** seat exposes one small seat-options action. Inviting a friend
 *   and seating an AI — kind and deck — happen inside that contextual flow and
 *   nowhere else, which is what #506's permanent "Add an AI opponent" panel and
 *   its room-header id chip are replaced by.
 *
 * Everything is a presentation read of the `SeatView`. **No legality is computed
 * here**: the seat-options flow offers `add_ai` only while the server advertises
 * it, so host-ness is never inferred client-side, and the AI's deck is validated
 * server-side exactly like a human's.
 *
 * The options surface is a **sibling** of the plaque, not a child: the plaque is
 * a `clip-path` box (that is what draws its chamfer) and a clip-path clips its
 * descendants, so a popover rendered inside one would be cut off at the corners.
 */
import { useState } from 'react';
import { cx } from '../chrome/cx';
import { Glyph } from '../chrome/glyphs';
import { seatDisplayName } from '../playerNames';
import type { AiOption, SeatView } from '../protocol';
import { ControlButton, IconButton } from '../table/controls';
import { LOCAL_PORTRAIT, OPPONENT_PORTRAITS, type SeatPortrait } from '../table/seatPortraits';
import { Plaque } from './MenuFrame';
import { DeckChoice } from './DeckChoice';
import type { DeckChoiceOption } from './deckChoice';
import { seatAccentVars } from './pregameScene';
import { seatFilled, seatMonogram } from './seatIdentity';
import p from './styles';

/**
 * A seat's face inside a seat-accent ring: the **same portrait plate** that seat
 * wears in the match (`table/seatPortraits.ts`, issue #555), falling back to the
 * seat's monogram where the manifest ships no plate for it.
 *
 * Two things are being kept identical across the ready gate on purpose, because
 * "identity is taught once" is only true if both hold: the accent is
 * `SCENE_SEAT_ACCENTS[seat]` — the same index the match uses — and the face is
 * `portraitFor` at the same seat index, since the server builds `seat_order` in
 * room-seat order. A player learns a face and a colour in the room and sits down
 * behind them a second later.
 *
 * Identity is never carried by hue alone: the plate is decorative
 * (`aria-hidden`), and the seat's display name is printed immediately beside it.
 *
 * Exported because the lobby's identity strip wears the local player's chip
 * beside their name, teaching the name and the colour together.
 */
export function CrestChip({
  monogram,
  seat,
  local = false,
  open = false,
  testId,
}: {
  monogram: string;
  /** `undefined` room-less, where no seat and therefore no accent exists yet. */
  seat?: number;
  local?: boolean;
  open?: boolean;
  testId?: string;
}) {
  const accented = seat !== undefined && !open;
  // `portraitFor` keys the opponent cycle off a seat's index in `seat_order`;
  // room-seat order IS that order, so the index is passed directly rather than
  // inventing a player id the room does not have here.
  const portrait = open ? undefined : seatPortrait(seat, local);
  return (
    <span
      className={cx(p.crest, local && accented && p.crestLocal, !accented && p.crestOpen)}
      style={accented ? seatAccentVars(seat) : undefined}
      aria-hidden="true"
      data-testid={testId ?? (seat !== undefined ? `seat-${seat}-crest` : 'crest')}
    >
      {open ? (
        <Glyph name="seat" size={18} />
      ) : portrait !== undefined ? (
        <img className={p.crestPlate} src={portrait.src} alt="" />
      ) : (
        monogram
      )}
    </span>
  );
}

/**
 * The plate a room seat wears, by seat index. `undefined` for a seat with no
 * index (the room-less identity strip) or a build whose manifest ships none —
 * in which case the monogram stands in, exactly as it did before the plates
 * shipped.
 */
function seatPortrait(seat: number | undefined, local: boolean): SeatPortrait | undefined {
  if (local) return LOCAL_PORTRAIT;
  if (seat === undefined || OPPONENT_PORTRAITS.length === 0) return undefined;
  return OPPONENT_PORTRAITS[seat % OPPONENT_PORTRAITS.length];
}

/** The readiness word. The WORD is the channel; the hue only tints it (§11). */
function ReadyBadge({ seat }: { seat: SeatView }) {
  const ready = seat.ready === true;
  return (
    <span
      className={cx(p.badge, ready && p.badgeOn)}
      data-testid={ready ? `seat-${seat.seat}-ready` : `seat-${seat.seat}-not-ready`}
    >
      {ready ? 'Ready' : 'Not ready'}
    </span>
  );
}

/** The local seat's deck dropdown and its two quiet shortcuts. */
export interface DeckChoiceSlot {
  options: readonly DeckChoiceOption[];
  selectedId: string;
  onSelect: (id: string) => void;
  /** Open the builder seeded with this deck (#508 owns the builder itself). */
  onEdit: () => void;
  /** Open the builder at its saved/import surface, which owns that flow. */
  onImport: () => void;
}

/** What an empty seat's contextual options offer. */
export interface SeatOptionsSlot {
  /** The room id a friend pastes under Join — the protocol's own join key. */
  roomId: string;
  /** Copy that id. */
  onCopyRoomId: () => void;
  /** Whether the last copy landed (drives the transient relabel). */
  copied: boolean;
  /** The AI kinds the server advertises; empty ⇒ no AI flow is offered. */
  aiOptions: readonly AiOption[];
  /** The decks an AI may be seated with. */
  deckOptions: readonly DeckChoiceOption[];
  /** Seat an AI. Present only while the server advertises `add_ai`. */
  onAddAi?: (seat: number, kind: string, deckId: string) => void;
}

export interface SeatPlaqueProps {
  seat: SeatView;
  /** Whether this seat is the local player's (drives the expansion). */
  local: boolean;
  /** The local seat's deck choice, when `submit_deck` is advertised. */
  deckChoice?: DeckChoiceSlot;
  /** The empty seat's contextual options; omitted, the seat offers none. */
  seatOptions?: SeatOptionsSlot;
  /** Remove an AI from this seat — offered only while `remove_ai` is advertised. */
  onRemoveAi?: () => void;
}

export function SeatPlaque({ seat, local, deckChoice, seatOptions, onRemoveAi }: SeatPlaqueProps) {
  const [optionsOpen, setOptionsOpen] = useState(false);
  const occupied = seatFilled(seat);
  const isAi = seat.ai !== undefined;
  const optionsId = `seat-${seat.seat}-options`;

  if (!occupied) {
    return (
      <>
        <Plaque faceClass={p.seatFace} testId={`seat-${seat.seat}`}>
          <div className={p.seatHead}>
            <CrestChip monogram="" seat={seat.seat} open />
            <span className={p.seatName}>Open seat</span>
            {seatOptions !== undefined && (
              <span className={p.fit}>
                <IconButton
                  glyph="+"
                  label={`Seat options for seat ${seat.seat + 1}`}
                  onPress={() => setOptionsOpen((wasOpen) => !wasOpen)}
                  expanded={optionsOpen}
                  controls={optionsId}
                  testId={`seat-${seat.seat}-options-button`}
                />
              </span>
            )}
          </div>
        </Plaque>
        {seatOptions !== undefined && optionsOpen && (
          <SeatOptions id={optionsId} seat={seat.seat} {...seatOptions} />
        )}
      </>
    );
  }

  return (
    <Plaque
      faceClass={p.seatFace}
      selected={local}
      style={seatAccentVars(seat.seat)}
      testId={`seat-${seat.seat}`}
    >
      <div className={p.seatHead}>
        <CrestChip monogram={seatMonogram(seat)} seat={seat.seat} local={local} />
        <span className={p.seatName}>
          {isAi ? (seat.name ?? 'Computer') : seatDisplayName(seat)}
        </span>
        {local && <span className={p.badge}>You</span>}
        {isAi && (
          <span className={p.badge} data-testid={`seat-${seat.seat}-ai`}>
            AI
          </span>
        )}
        <ReadyBadge seat={seat} />
        {isAi && onRemoveAi !== undefined && (
          <span className={p.fit}>
            <ControlButton
              variant="cancel"
              label="Remove"
              accessibleName={`Remove the AI in seat ${seat.seat + 1}`}
              onPress={onRemoveAi}
              testId={`remove-ai-${seat.seat}-button`}
            />
          </span>
        )}
      </div>

      {/* The seat's deck, for a seat that is not choosing one. The wire carries
          a redacted `decked` flag and no deck NAME for any seat — not even your
          own after a reload — so this states what is actually known rather than
          printing a name the client would have had to invent (see the module
          header of `RoomPlace.tsx`). */}
      {deckChoice === undefined && (
        <span className={p.seatDeck} data-testid={`seat-${seat.seat}-deck`}>
          {seat.decked === true ? 'Deck submitted' : 'Choosing a deck'}
        </span>
      )}

      {deckChoice !== undefined && (
        <div className={p.deckRow}>
          <DeckChoice
            options={deckChoice.options}
            selectedId={deckChoice.selectedId}
            onSelect={deckChoice.onSelect}
          />
          <span className={p.fit}>
            <ControlButton
              variant="utility"
              label="Edit"
              accessibleName="Edit this deck in the deck builder"
              onPress={deckChoice.onEdit}
              testId="open-deck-builder-button"
            />
          </span>
          <span className={p.fit}>
            <ControlButton
              variant="utility"
              label="Import"
              accessibleName="Import a deck document"
              onPress={deckChoice.onImport}
              testId="import-deck-button"
            />
          </span>
        </div>
      )}
    </Plaque>
  );
}

/**
 * One empty seat's contextual options: invite a friend to it, or fill it with an
 * AI. Opened from the seat, anchored to the seat, and gone when dismissed —
 * never a panel the room carries whether or not anyone wants it.
 */
function SeatOptions({
  id,
  seat,
  roomId,
  onCopyRoomId,
  copied,
  aiOptions,
  deckOptions,
  onAddAi,
}: SeatOptionsSlot & { id: string; seat: number }) {
  const [kind, setKind] = useState(aiOptions[0]?.id);
  const [deckId, setDeckId] = useState(deckOptions[0]?.id ?? '');

  // The picked kind and deck must stay valid as the catalog arrives or changes:
  // nothing here is load-bearing, so fall back to the first still-valid choice.
  const kindValue = aiOptions.some((option) => option.id === kind) ? kind : aiOptions[0]?.id;
  const deckValue = deckOptions.some((option) => option.id === deckId)
    ? deckId
    : (deckOptions[0]?.id ?? '');

  return (
    <div className={p.seatOptions} id={id} data-testid="seat-options">
      <div className={p.seatOptionsField}>
        <span className={p.fieldLabel}>Invite a friend</span>
        {/* The protocol's join key IS a room id — there is no invite link — and
            the words here match the lobby's Join field exactly. */}
        <span className={p.codeRow}>
          <code className={p.codeText} data-testid="room-id">
            {roomId}
          </code>
          <span className={p.fit}>
            <ControlButton
              variant="utility"
              label={copied ? 'Copied' : 'Copy'}
              accessibleName="Copy room id"
              onPress={onCopyRoomId}
              testId="copy-room-id-button"
            />
          </span>
        </span>
        <span className={p.muted}>Send this id to a friend — they paste it under Join.</span>
      </div>

      {onAddAi !== undefined && kindValue !== undefined && (
        <div className={p.seatOptionsField} data-testid="ai-seating">
          <span className={p.fieldLabel}>Or seat an opponent</span>
          <select
            className={p.select}
            value={kindValue}
            onChange={(event) => setKind(event.target.value)}
            aria-label="AI opponent"
            data-testid="ai-kind-select"
          >
            {aiOptions.map((option) => (
              <option key={option.id} value={option.id} data-testid={`ai-kind-${option.id}`}>
                {option.name}
              </option>
            ))}
          </select>
          <select
            className={p.select}
            value={deckValue}
            onChange={(event) => setDeckId(event.target.value)}
            aria-label="AI deck"
            data-testid="ai-deck-select"
          >
            {deckOptions.map((option) => (
              <option key={option.id} value={option.id}>
                {option.name}
              </option>
            ))}
          </select>
          <span className={p.wide}>
            <ControlButton
              variant="secondary"
              label={`Add to seat ${seat + 1}`}
              onPress={() => onAddAi(seat, kindValue, deckValue)}
              testId="add-ai-button"
            />
          </span>
        </div>
      )}
    </div>
  );
}
